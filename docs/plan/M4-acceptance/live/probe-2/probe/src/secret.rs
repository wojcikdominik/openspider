//! Schlüssel der Anbieter (AI-009, K4-12): nur im Live-Modus aus dem Schlüsselbund gelesen, nie ausgegeben.
//!
//! [`ApiSecret`] hat eine geschwärzte `Debug`-Ausgabe, kein `Display`, kein `Serialize` und kein `Clone`. Den Wert
//! sieht nur der HTTP-Aufbau dieser Crate (`expose` ist `pub(crate)`). Zusätzlich läuft jede Ausgabe und jede
//! Ergebnisdatei durch [`Scrubber`].

use crate::plan::Provider;
use serde_json::Value;

/// Dienst der Testschlüssel des Nutzers für diese Probe (K4-12, Nachtrag 24.11.2026).
pub const PROBE_SERVICE: &str = "OpenSpider-Probe";
/// Dienst der App (AI-009); nur ersatzweise.
pub const APP_SERVICE: &str = "OpenSpider";

/// Ein API-Schlüssel.
pub struct ApiSecret(String);

impl ApiSecret {
    /// Übernimmt den Schlüssel (führende/folgende Leerzeichen und Zeilenumbrüche werden entfernt).
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().trim().to_string())
    }

    /// Der Wert für den Authentifizierungs-Header bzw. die Adapter-Konfiguration.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    /// Ob kein Wert vorhanden ist.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for ApiSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiSecret([REDACTED])")
    }
}

/// Ergebnis einer Schlüsselsuche.
#[derive(Debug)]
pub enum KeyLookup {
    /// Gefunden unter diesem Dienst (Konto ist [`Provider::account`]).
    Found {
        secret: ApiSecret,
        service: &'static str,
    },
    /// Nicht gefunden; `note` nennt nur Dienst und Art (nie einen Wert).
    Missing { note: String },
}

/// Quelle der Schlüssel. Das Programm nutzt nur [`KeyringSource`]; Tests reichen eigene Quellen herein.
pub trait KeySource {
    fn lookup(&self, provider: Provider) -> KeyLookup;
}

/// Betriebssystem-Schlüsselbund über die `keyring`-Crate: zuerst Dienst [`PROBE_SERVICE`], dann [`APP_SERVICE`],
/// Konto `ai.<anbieter>`, Wert = Schlüssel als Klartext.
#[derive(Debug, Default)]
pub struct KeyringSource;

impl KeySource for KeyringSource {
    fn lookup(&self, provider: Provider) -> KeyLookup {
        let mut notes = Vec::new();
        for service in [PROBE_SERVICE, APP_SERVICE] {
            let read =
                keyring::Entry::new(service, provider.account()).and_then(|e| e.get_password());
            match read {
                Ok(value) => {
                    let secret = ApiSecret::new(value);
                    if secret.is_empty() {
                        notes.push(format!("{service}/{}: leerer Eintrag", provider.account()));
                    } else {
                        return KeyLookup::Found { secret, service };
                    }
                }
                Err(keyring::Error::NoEntry) => {
                    notes.push(format!("{service}/{}: kein Eintrag", provider.account()));
                }
                // Plattformfehler werden nicht im Wortlaut übernommen (kein Risiko, Werte mitzuschreiben).
                Err(_) => notes.push(format!(
                    "{service}/{}: Schlüsselbund nicht lesbar (Zugriff verweigert oder gesperrt)",
                    provider.account()
                )),
            }
        }
        KeyLookup::Missing {
            note: notes.join("; "),
        }
    }
}

/// Ersetzt Schlüssel (und deren letzte Zeichen, wie sie Anbieter in maskierten Fehlermeldungen zeigen) in allen
/// Ausgaben und Ergebnisdateien durch `[REDACTED]`.
#[derive(Debug, Default, Clone)]
pub struct Scrubber {
    needles: Vec<String>,
}

const REDACTED: &str = "[REDACTED]";

impl Scrubber {
    /// Nimmt einen Schlüssel auf.
    pub fn add(&mut self, secret: &ApiSecret) {
        let value = secret.expose();
        if value.is_empty() {
            return;
        }
        self.needles.push(value.to_string());
        let chars: Vec<char> = value.chars().collect();
        if chars.len() >= 20 {
            let tail: String = chars[chars.len() - 4..].iter().collect();
            self.needles.push(tail);
        }
        // Längere Treffer zuerst ersetzen.
        self.needles.sort_by_key(|n| std::cmp::Reverse(n.len()));
        self.needles.dedup();
    }

    /// Text ohne Schlüssel.
    pub fn text(&self, text: &str) -> String {
        let mut out = text.to_string();
        for needle in &self.needles {
            if out.contains(needle.as_str()) {
                out = out.replace(needle.as_str(), REDACTED);
            }
        }
        out
    }

    /// JSON ohne Schlüssel (Werte und Feldnamen).
    pub fn json(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.text(s)),
            Value::Array(items) => Value::Array(items.iter().map(|v| self.json(v)).collect()),
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| (self.text(k), self.json(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}
