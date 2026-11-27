//! Gespeicherte KI-Eingaben je interner HTML-Seite mit Status 200 (08 AI-032, Nachtrag „Quelle der Eingabefelder“,
//! K4-24, berichtigt 27.11.2026 mit K4-26; M4-D05).
//!
//! Gespeichert wird nur, was AI-032 als Eingabe kennt und die Crawl-Datei nicht ohnehin enthält: Text des
//! Inhaltsbereichs (ONPAGE-140), gekürzt auf [`STORED_PAGE_TEXT_MAX_CHARS`] an einer Zeichengrenze, alle H2 in
//! Dokumentreihenfolge und je Bild der Bildkontext – mit zstd komprimiert wie `page_html` (Spalte `body`) –, dazu in eigenen
//! Spalten `html_lang` (Rohwert) und `structured_data_types` (JSON-Liste; NULL = Extraktion war aus, unterscheidbar von
//! einer leeren Liste). Titel, Meta Description, H1, URL, Status und Wortzahl stehen in `urls`, `hreflang_self` in
//! `hreflang`. Die Tabellen von 1.0 ändern sich nicht.
use std::collections::BTreeMap;

use os_model::ai::hook::STORED_PAGE_TEXT_MAX_CHARS;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::error::StoreError;

/// Bildkontext eines Bildes der Seite.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredImage {
    pub src: String,
    pub alt: Option<String>,
    pub context: Option<String>,
}

/// Gespeicherte KI-Eingaben einer URL.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredInputs {
    /// Text des Inhaltsbereichs (höchstens [`STORED_PAGE_TEXT_MAX_CHARS`] Zeichen).
    pub page_text: Option<String>,
    /// Alle H2 in Dokumentreihenfolge.
    pub h2: Vec<String>,
    /// Bildkontext je Bild der Seite.
    pub images: Vec<StoredImage>,
    /// Rohwert von `<html lang>` (ohne `lang` `xml:lang`).
    pub html_lang: Option<String>,
    /// Typen der strukturierten Daten; `None` = Extraktion war aus.
    pub structured_data_types: Option<Vec<String>>,
}

/// Kürzt auf höchstens `max` Zeichen an einer Zeichengrenze (wie INTEG-172).
fn truncate(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((byte, _)) => &text[..byte],
        None => text,
    }
}

fn opt_text(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

/// Komprimierter Inhalt der Spalte `body` (JSON, zstd Stufe 3 wie `page_html`).
fn encode_body(inputs: &StoredInputs) -> Result<Vec<u8>, StoreError> {
    let body = json!({
        "page_text": inputs.page_text.as_deref().map(|t| truncate(t, STORED_PAGE_TEXT_MAX_CHARS)),
        "h2": inputs.h2,
        "images": inputs.images.iter().map(|i| json!({
            "src": i.src, "alt": i.alt, "context": i.context,
        })).collect::<Vec<_>>(),
    });
    zstd::stream::encode_all(body.to_string().as_bytes(), 3)
        .map_err(|e| StoreError::InvalidInput(format!("KI-Eingaben-Kompression: {e}")))
}

fn decode_body(
    bytes: &[u8],
    html_lang: Option<String>,
    sd: Option<String>,
) -> Result<StoredInputs, StoreError> {
    let raw = zstd::stream::decode_all(bytes)
        .map_err(|e| StoreError::InvalidInput(format!("KI-Eingaben-Dekompression: {e}")))?;
    let v: Value = serde_json::from_slice(&raw)
        .map_err(|e| StoreError::InvalidInput(format!("KI-Eingaben: {e}")))?;
    let structured_data_types = match sd {
        None => None,
        Some(text) => Some(
            serde_json::from_str::<Vec<String>>(&text)
                .map_err(|e| StoreError::InvalidInput(format!("structured_data_types: {e}")))?,
        ),
    };
    Ok(StoredInputs {
        page_text: opt_text(&v["page_text"]),
        h2: v["h2"]
            .as_array()
            .map(|a| a.iter().filter_map(opt_text).collect())
            .unwrap_or_default(),
        images: v["images"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|i| StoredImage {
                        src: opt_text(&i["src"]).unwrap_or_default(),
                        alt: opt_text(&i["alt"]),
                        context: opt_text(&i["context"]),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        html_lang,
        structured_data_types,
    })
}

/// Schreibt (bzw. ersetzt) die KI-Eingaben einer URL; Rückgabe: Bytes der komprimierten Spalte `body`.
pub fn write(conn: &Connection, url_id: i64, inputs: &StoredInputs) -> Result<usize, StoreError> {
    let body = encode_body(inputs)?;
    let sd = inputs
        .structured_data_types
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".into()));
    conn.prepare_cached(
        "INSERT INTO ai_inputs (url_id, compression, body, html_lang, structured_data_types) \
         VALUES (?1, 'zstd', ?2, ?3, ?4) ON CONFLICT(url_id) DO UPDATE SET compression = excluded.compression, \
         body = excluded.body, html_lang = excluded.html_lang, structured_data_types = excluded.structured_data_types",
    )?
    .execute(params![url_id, body, inputs.html_lang, sd])?;
    Ok(body.len())
}

/// Schreibt mehrere Einträge in einer Transaktion; Rückgabe: Summe der Bytes von `body`.
pub fn write_all(conn: &Connection, rows: &[(i64, StoredInputs)]) -> Result<usize, StoreError> {
    if rows.is_empty() {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction()?;
    let mut bytes = 0;
    for (url_id, inputs) in rows {
        bytes += write(&tx, *url_id, inputs)?;
    }
    tx.commit()?;
    Ok(bytes)
}

/// Liest die KI-Eingaben einer URL.
pub fn read(conn: &Connection, url_id: i64) -> Result<Option<StoredInputs>, StoreError> {
    let row: Option<(Vec<u8>, Option<String>, Option<String>)> = conn
        .prepare_cached(
            "SELECT body, html_lang, structured_data_types FROM ai_inputs WHERE url_id = ?1",
        )?
        .query_row([url_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    row.map(|(body, lang, sd)| decode_body(&body, lang, sd))
        .transpose()
}

/// Liest alle gespeicherten KI-Eingaben (Schlüssel `url_id`).
pub fn read_all(conn: &Connection) -> Result<BTreeMap<i64, StoredInputs>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT url_id, body, html_lang, structured_data_types FROM ai_inputs ORDER BY url_id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = BTreeMap::new();
    for (id, body, lang, sd) in rows {
        out.insert(id, decode_body(&body, lang, sd)?);
    }
    Ok(out)
}

/// Größe der gespeicherten KI-Eingaben: Zahl der Einträge, Bytes der komprimierten Spalte `body` und der beiden
/// übrigen Spalten (für die Größenabschätzung in 08 AI-032).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputsSize {
    pub entries: u64,
    pub body_bytes: u64,
    pub other_bytes: u64,
}

/// Misst die gespeicherten KI-Eingaben.
pub fn size(conn: &Connection) -> Result<InputsSize, StoreError> {
    Ok(conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(length(body)), 0), \
         COALESCE(SUM(length(COALESCE(html_lang, '')) + length(COALESCE(structured_data_types, ''))), 0) \
         FROM ai_inputs",
        [],
        |r| {
            Ok(InputsSize {
                entries: r.get::<_, i64>(0)? as u64,
                body_bytes: r.get::<_, i64>(1)? as u64,
                other_bytes: r.get::<_, i64>(2)? as u64,
            })
        },
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{migrate, MIGRATIONS};

    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        // Wie jede Crawl-Datei ohne Fremdschlüsselprüfung (`profile::apply_pragmas`).
        db.pragma_update(None, "foreign_keys", false).unwrap();
        migrate(&mut db, 0, MIGRATIONS).unwrap();
        db
    }

    // spec: AI-032 [M4]
    // Nachtrag K4-24/K4-26: Inhalt zstd-komprimiert, Text höchstens 20.000 Zeichen an einer Zeichengrenze,
    // `structured_data_types` NULL (Extraktion aus) unterscheidbar von leerer Liste.
    #[test]
    fn roundtrip_limit_and_null_versus_empty_types() {
        let db = db();
        let long = "ä".repeat(STORED_PAGE_TEXT_MAX_CHARS + 50);
        let a = StoredInputs {
            page_text: Some(long),
            h2: vec!["Eins".into(), "Zwei".into()],
            images: vec![StoredImage {
                src: "http://ai.test:8080/a.png".into(),
                alt: Some("Alt".into()),
                context: Some("Unterschrift".into()),
            }],
            html_lang: Some("de-DE".into()),
            structured_data_types: None,
        };
        let b = StoredInputs {
            structured_data_types: Some(Vec::new()),
            ..StoredInputs::default()
        };
        write_all(&db, &[(1, a.clone()), (2, b.clone())]).unwrap();
        let got = read(&db, 1).unwrap().unwrap();
        assert_eq!(
            got.page_text.as_ref().unwrap().chars().count(),
            STORED_PAGE_TEXT_MAX_CHARS
        );
        assert_eq!(got.h2, a.h2);
        assert_eq!(got.images, a.images);
        assert_eq!(got.html_lang.as_deref(), Some("de-DE"));
        assert_eq!(got.structured_data_types, None);
        assert_eq!(read(&db, 2).unwrap().unwrap().structured_data_types, Some(Vec::new()));
        let raw: Option<String> = db
            .query_row("SELECT structured_data_types FROM ai_inputs WHERE url_id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(raw.is_none());
        let compression: String = db
            .query_row("SELECT compression FROM ai_inputs WHERE url_id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(compression, "zstd");
        let size = size(&db).unwrap();
        assert_eq!(size.entries, 2);
        assert!(size.body_bytes > 0 && size.body_bytes < 40_000);
        assert_eq!(read_all(&db).unwrap().len(), 2);
        assert!(read(&db, 3).unwrap().is_none());
    }
}
