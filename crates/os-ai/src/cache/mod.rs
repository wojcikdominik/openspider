//! Ergebnis-Cache in der Crawl-Datei (AI-015).
//!
//! * [`CacheKeyInput`] beschreibt alles, was ein Ergebnis bestimmt; [`CacheKey::new`] bildet daraus den Schlüssel als
//!   SHA-256 über eine kanonische JSON-Darstellung ([`canonical_json`]: Objektschlüssel nach Unicode-Codepunkten
//!   sortiert, kein Leerraum, Zeichenketten unverändert in UTF-8). Damit ist der Schlüssel plattformunabhängig.
//! * [`input_hash`] hasht die tatsächlich gesendete Eingabe (nach Kürzung und Schwärzung), Bilder über ihre Bytes.
//! * [`ResultCache`] wendet die Regeln an: nur `Success` und `Refusal` werden gespeichert (Refusal mit Kennzeichen),
//!   Fehler nie; „Refusals erneut versuchen“ umgeht den Cache nur für Refusals; Option „Cache verwenden“.
//!
//! Persistenz: `os_store::ai::cache` (Tabelle `ai_cache`).
use std::cell::Cell;
use std::future::Future;
use std::path::Path;

use os_model::ai::AiStatus;
use os_model::Timestamp;
use os_store::ai::cache::{self as store, CacheImport, CacheRow, Connection};
use os_store::StoreError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::provider::Input;

/// Version der Schlüsseldarstellung; ändert sich nur, wenn sich die Bildungsregel ändert.
pub const KEY_FORMAT_VERSION: u64 = 1;

/// Ausführungsweg einer Frage (AI-015, AI-033/AI-034).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionPath {
    /// `decisions-api`
    DecisionsApi,
    /// `structured-output`
    StructuredOutput,
}

impl ExecutionPath {
    /// Kanonischer Name im Schlüssel.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DecisionsApi => "decisions-api",
            Self::StructuredOutput => "structured-output",
        }
    }
}

/// Alle Bestandteile des Cache-Schlüssels nach AI-015. Schwellwerte, Anzeigenamen und andere nicht
/// ergebnisrelevante Einstellungen gehören bewusst **nicht** hierher.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheKeyInput {
    /// Aufgabentyp, z. B. `decision`, `suggest-title`, `prompt`, `embedding`.
    pub task: String,
    /// Frage-/Prompt-ID bzw. Vorlagen-ID.
    pub definition_id: String,
    /// Version der Frage bzw. bei parametrisierten Vorlagen die Vorlagenversion (AI-031).
    pub definition_version: String,
    /// Eingesetzte Vorlagenparameter (z. B. Sprachcode, Kandidatenliste); `{}` ohne Parameter.
    pub params: Value,
    /// Ausführungsweg; `None` für Aufgaben ohne Wegwahl (z. B. Embeddings).
    pub path: Option<ExecutionPath>,
    pub provider: String,
    pub model: String,
    /// Endpunkt-Basis-URL (global bzw. EU).
    pub endpoint: String,
    /// Ergebnisrelevante Parameter (z. B. Temperatur, Dimensionen); `{}` ohne Parameter.
    pub result_params: Value,
    /// [`input_hash`] der tatsächlich gesendeten Eingabe.
    pub input_hash: String,
}

/// Fertiger Schlüssel mit seinen Bestandteilen.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheKey {
    key: String,
    params_hash: String,
    input: CacheKeyInput,
}

impl CacheKey {
    /// Bildet den Schlüssel: SHA-256 (hex, Kleinbuchstaben) über [`CacheKey::canonical`].
    pub fn new(input: CacheKeyInput) -> Self {
        let key = sha256_hex(canonical_key_json(&input).as_bytes());
        let params_hash = params_hash(&input.params);
        Self {
            key,
            params_hash,
            input,
        }
    }

    /// Der Schlüssel (64 Hex-Zeichen).
    pub fn as_str(&self) -> &str {
        &self.key
    }

    /// SHA-256 der kanonischen Vorlagenparameter.
    pub fn params_hash(&self) -> &str {
        &self.params_hash
    }

    /// Die Bestandteile.
    pub fn input(&self) -> &CacheKeyInput {
        &self.input
    }

    /// Die kanonische JSON-Darstellung, über die gehasht wird.
    pub fn canonical(&self) -> String {
        canonical_key_json(&self.input)
    }
}

fn canonical_key_json(input: &CacheKeyInput) -> String {
    let value = serde_json::json!({
        "v": KEY_FORMAT_VERSION,
        "task": input.task,
        "definition_id": input.definition_id,
        "definition_version": input.definition_version,
        "params": input.params,
        "path": input.path.map(ExecutionPath::as_str),
        "provider": input.provider,
        "model": input.model,
        "endpoint": input.endpoint,
        "result_params": input.result_params,
        "input_hash": input.input_hash,
    });
    canonical_json(&value)
}

/// SHA-256 (hex) der kanonischen Vorlagenparameter.
pub fn params_hash(params: &Value) -> String {
    sha256_hex(canonical_json(params).as_bytes())
}

/// SHA-256 (hex) der tatsächlich gesendeten Eingabe: Textteile mit ihrem Text, Bilder mit Medientyp und dem
/// SHA-256 ihrer **Bytes** (nicht der Base64-Form). Die Reihenfolge der Teile zählt.
pub fn input_hash(parts: &[Input]) -> String {
    let parts: Vec<Value> = parts
        .iter()
        .map(|part| match part {
            Input::Text { text } => serde_json::json!({"type": "text", "text": text}),
            Input::Image { media_type, base64 } => {
                let bytes_hash = match decode_base64(base64) {
                    Some(bytes) => sha256_hex(&bytes),
                    // Kein gültiges Base64: deterministisch über den Text, aber getrennt von gültigen Bildern.
                    None => format!("raw:{}", sha256_hex(base64.as_bytes())),
                };
                serde_json::json!({"type": "image", "media_type": media_type, "sha256": bytes_hash})
            }
        })
        .collect();
    sha256_hex(canonical_json(&Value::Array(parts)).as_bytes())
}

/// Kanonisches JSON: Objektschlüssel aufsteigend nach Unicode-Codepunkten (= UTF-8-Bytefolge), keine Leerzeichen,
/// Zeichenketten und Zahlen in der Schreibweise von `serde_json` (Steuerzeichen als Escape, sonst unverändertes
/// UTF-8). Unabhängig von Plattform, Zeilenenden der Quelldateien und Einfügereihenfolge.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => out.push_str(&Value::String(s.clone()).to_string()),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
    }
}

/// SHA-256 als Hex-Zeichenkette (Kleinbuchstaben).
pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Standard-Base64 (RFC 4648, mit oder ohne Auffüllung, Leerraum wird ignoriert).
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let data = match clean.iter().position(|&b| b == b'=') {
        Some(pos) => {
            if clean[pos..].iter().any(|&b| b != b'=') || !clean.len().is_multiple_of(4) {
                return None;
            }
            &clean[..pos]
        }
        None => &clean[..],
    };
    if data.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    for chunk in data.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= value(c)? << (18 - 6 * i);
        }
        let bytes = acc.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

/// Optionen des Caches (AI-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheOptions {
    /// „Cache verwenden“ (Standard an). Aus: der Cache wird weder gelesen noch beschrieben.
    pub use_cache: bool,
    /// „Refusals erneut versuchen“: gecachte Refusals gelten als nicht vorhanden; erfolgreiche Ergebnisse bleiben.
    pub retry_refusals: bool,
}

impl Default for CacheOptions {
    fn default() -> Self {
        Self {
            use_cache: true,
            retry_refusals: false,
        }
    }
}

/// Ergebnis einer Cache-Abfrage.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// Treffer; `status` ist `Success (cached)` bzw. `Refusal` für gecachte Refusals.
    Hit { status: AiStatus, result: Value },
    /// Kein (verwendbarer) Eintrag: Request nötig.
    Miss,
}

/// Cache-Statistik für das KI-Protokoll (Treffer, Einträge, Größe).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Treffer seit Anlegen dieses [`ResultCache`].
    pub hits: u64,
    /// Fehlversuche seit Anlegen (einschließlich umgangener Refusals).
    pub misses: u64,
    /// Einträge in der Crawl-Datei.
    pub entries: u64,
    /// Davon Refusals.
    pub refusals: u64,
    /// Gespeicherte Bytes (siehe `os_store::ai::cache::CacheTableStats`).
    pub size_bytes: u64,
}

/// Ergebnis von [`ResultCache::get_or_run`].
#[derive(Debug, Clone, PartialEq)]
pub enum CachedRun<T> {
    /// Aus dem Cache, ohne Request.
    Hit { status: AiStatus, result: Value },
    /// Ausgeführt; `stored` sagt, ob das Ergebnis in den Cache ging.
    Ran { outcome: T, stored: bool },
}

/// Ergebnis-Cache über einer Crawl-Datei-Verbindung.
pub struct ResultCache<'c> {
    conn: &'c Connection,
    options: CacheOptions,
    hits: Cell<u64>,
    misses: Cell<u64>,
}

impl<'c> ResultCache<'c> {
    /// Cache über `conn` (Crawl-Datei im aktuellen Schema).
    pub fn new(conn: &'c Connection, options: CacheOptions) -> Self {
        Self {
            conn,
            options,
            hits: Cell::new(0),
            misses: Cell::new(0),
        }
    }

    /// Die Optionen.
    pub fn options(&self) -> CacheOptions {
        self.options
    }

    /// Sucht ein Ergebnis.
    pub fn lookup(&self, key: &CacheKey) -> Result<Lookup, StoreError> {
        if !self.options.use_cache {
            return Ok(Lookup::Miss);
        }
        let hit = store::get(self.conn, key.as_str())?
            .filter(|row| !(row.is_refusal() && self.options.retry_refusals));
        let Some(row) = hit else {
            self.misses.set(self.misses.get() + 1);
            return Ok(Lookup::Miss);
        };
        let result: Value = serde_json::from_str(&row.result_json)
            .map_err(|e| StoreError::InvalidInput(format!("ai_cache.result_json: {e}")))?;
        self.hits.set(self.hits.get() + 1);
        let status = if row.is_refusal() {
            AiStatus::Refusal
        } else {
            AiStatus::SuccessCached
        };
        Ok(Lookup::Hit { status, result })
    }

    /// Speichert ein Ergebnis, wenn es cachebar ist: `Success` (das Ergebnis) und `Refusal` (z. B. der
    /// Refusal-Grund oder `null`). Alle anderen Statuswerte – Fehler, Übersprungene, Ausstehende – werden nie
    /// gespeichert; dann `Ok(false)`.
    pub fn store(
        &self,
        key: &CacheKey,
        status: &AiStatus,
        result: &Value,
        created_at: Timestamp,
    ) -> Result<bool, StoreError> {
        let status_text = match status {
            AiStatus::Success => store::STATUS_SUCCESS,
            AiStatus::Refusal => store::STATUS_REFUSAL,
            _ => return Ok(false),
        };
        if !self.options.use_cache {
            return Ok(false);
        }
        let input = key.input();
        store::put(
            self.conn,
            &CacheRow {
                cache_key: key.as_str().to_string(),
                provider: input.provider.clone(),
                model: input.model.clone(),
                endpoint: input.endpoint.clone(),
                definition_id: input.definition_id.clone(),
                definition_version: input.definition_version.clone(),
                input_hash: input.input_hash.clone(),
                params_hash: key.params_hash().to_string(),
                result_json: canonical_json(result),
                status: status_text.to_string(),
                created_at: created_at.to_db(),
            },
        )?;
        Ok(true)
    }

    /// Sucht im Cache und führt `run` nur bei einem Fehlversuch aus; `run` liefert den Endstatus, das zu cachende
    /// Ergebnis und einen beliebigen Begleitwert (z. B. die Protokolldaten des Requests).
    pub async fn get_or_run<T, F, Fut>(
        &self,
        key: &CacheKey,
        created_at: Timestamp,
        run: F,
    ) -> Result<CachedRun<T>, StoreError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = (AiStatus, Value, T)>,
    {
        if let Lookup::Hit { status, result } = self.lookup(key)? {
            return Ok(CachedRun::Hit { status, result });
        }
        let (status, result, outcome) = run().await;
        let stored = self.store(key, &status, &result, created_at)?;
        Ok(CachedRun::Ran { outcome, stored })
    }

    /// „Cache für Aufgabe/Frage leeren“ (optional nur eine Version).
    pub fn clear_definition(
        &self,
        definition_id: &str,
        definition_version: Option<&str>,
    ) -> Result<u64, StoreError> {
        store::clear_definition(self.conn, definition_id, definition_version)
    }

    /// „KI-Cache aus anderer Crawl-Datei übernehmen…“; die Quelle bleibt unverändert.
    pub fn import_from(&self, source: &Path) -> Result<CacheImport, StoreError> {
        store::import_from(self.conn, source)
    }

    /// Statistik (Treffer dieser Sitzung, Einträge und Größe der Datei).
    pub fn stats(&self) -> Result<CacheStats, StoreError> {
        let table = store::stats(self.conn)?;
        Ok(CacheStats {
            hits: self.hits.get(),
            misses: self.misses.get(),
            entries: table.entries,
            refusals: table.refusals,
            size_bytes: table.size_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-015
    #[test]
    fn base64_decoding_matches_known_vectors() {
        assert_eq!(decode_base64("").unwrap(), b"");
        assert_eq!(decode_base64("Zg==").unwrap(), b"f");
        assert_eq!(decode_base64("Zm8=").unwrap(), b"fo");
        assert_eq!(decode_base64("Zm9v").unwrap(), b"foo");
        assert_eq!(decode_base64("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(decode_base64("Zm9v\nYmE=").unwrap(), b"fooba");
        assert_eq!(decode_base64("Zm9vYg").unwrap(), b"foob");
        assert!(decode_base64("Zm9v!").is_none());
        assert!(decode_base64("Z=m9").is_none());
    }

    // spec: AI-015
    #[test]
    fn canonical_json_sorts_keys_and_drops_whitespace() {
        let a: Value =
            serde_json::from_str(r#"{ "b": [1, 2.5, {"y": null, "x": true}], "a": "ä\n" }"#)
                .unwrap();
        assert_eq!(
            canonical_json(&a),
            r#"{"a":"ä\n","b":[1,2.5,{"x":true,"y":null}]}"#
        );
        // Sortierung nach Codepunkten, nicht nach Gebietsschema.
        let b = serde_json::json!({"é": 1, "z": 2, "Z": 3});
        assert_eq!(canonical_json(&b), r#"{"Z":3,"z":2,"é":1}"#);
    }
}
