//! Schlüsselverwaltung der KI-Anbieter (AI-009, Ablageschema INTEG-002, Entscheidung K4-12).
//!
//! - Ablage nur über [`SecretStoreMut`] (Regel C13): Dienst `OpenSpider` (der Dienstname des
//!   [`os_fetch::secrets::KeyringSecretStore`]), Konto `ai.<anbieter>` (`ai.anthropic`, `ai.openai`, `ai.openrouter`),
//!   Wert = der Schlüssel als Klartext-Zeichenkette (kein JSON). 04 und 08 teilen sich denselben Eintrag.
//! - Umgebungsvariablen `OPENSPIDER_AI_KEY_<ANBIETER>` haben Vorrang, wenn gesetzt, und werden nie in den Schlüsselbund
//!   geschrieben (Headless-Läufe; unter Linux/Docker der einzige Weg, E-7 a).
//! - „Schlüssel merken“ aus bzw. gesperrter Schlüsselbund: Schlüssel nur für die laufende Sitzung im Speicher
//!   (INTEG-002).
//! - Kein Typ dieses Moduls zeigt einen Schlüssel in `Debug`, `Display` oder Fehlertexten; es gibt keine
//!   Längenannahme (Schlüssel über 128 Zeichen sind üblich, Live-Probe Q00).
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use os_fetch::secrets::{KeyringSecretStore, SecretStoreMut};
use os_model::ai::AiStatus;

/// Dienstname im Schlüsselbund (INTEG-002).
pub const KEYCHAIN_SERVICE: &str = KeyringSecretStore::DEFAULT_SERVICE;

/// KI-Anbieter mit eigenem Schlüssel (AI-009). Ollama braucht keinen Schlüssel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AiKeyProvider {
    Anthropic,
    OpenAi,
    OpenRouter,
}

impl AiKeyProvider {
    /// Alle Anbieter in Anzeigereihenfolge.
    pub const ALL: [AiKeyProvider; 3] = [Self::Anthropic, Self::OpenAi, Self::OpenRouter];

    /// Anbieterkennung wie in `provider::Model::provider` (`anthropic`, `openai`, `openrouter`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::OpenRouter => "openrouter",
        }
    }

    /// Anbieter zur Kennung; `None` für Anbieter ohne Schlüssel (z. B. `ollama`).
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Konto des Schlüsselbund-Eintrags (`ai.<anbieter>`, INTEG-002).
    pub fn account(self) -> &'static str {
        match self {
            Self::Anthropic => "ai.anthropic",
            Self::OpenAi => "ai.openai",
            Self::OpenRouter => "ai.openrouter",
        }
    }

    /// Umgebungsvariable mit Vorrang vor dem Schlüsselbund (AI-009).
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Anthropic => "OPENSPIDER_AI_KEY_ANTHROPIC",
            Self::OpenAi => "OPENSPIDER_AI_KEY_OPENAI",
            Self::OpenRouter => "OPENSPIDER_AI_KEY_OPENROUTER",
        }
    }
}

/// Ein API-Schlüssel. `Debug` zeigt den Wert nie; Zugriff nur über [`ApiKeySecret::expose`].
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKeySecret(String);

impl ApiKeySecret {
    /// Schlüssel ohne führenden und abschließenden Leerraum; leer → `None`. Keine Längenbegrenzung.
    pub fn new(value: &str) -> Option<Self> {
        let value = value.trim();
        (!value.is_empty()).then(|| Self(value.to_string()))
    }

    /// Der Klartext – nur für den Authorization-Header des Adapters.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Maskierte Anzeige nach INTEG-002: acht Punkte und die letzten vier Zeichen (`••••••••abcd`); Schlüssel mit
    /// höchstens vier Zeichen werden vollständig maskiert.
    pub fn masked(&self) -> String {
        let chars: Vec<char> = self.0.chars().collect();
        let mask = "\u{2022}".repeat(8);
        if chars.len() <= 4 {
            return mask;
        }
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{mask}{tail}")
    }
}

impl std::fmt::Debug for ApiKeySecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKeySecret([REDACTED])")
    }
}

/// Herkunft eines verwendeten Schlüssels (Anzeige „aus Umgebungsvariable“, AI-009).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// `OPENSPIDER_AI_KEY_<ANBIETER>`.
    Environment,
    /// Schlüsselbund, Dienst `OpenSpider`, Konto `ai.<anbieter>`.
    Keychain,
    /// Nur im Speicher der laufenden Sitzung („merken“ aus oder Schlüsselbund nicht verfügbar).
    Session,
}

/// Ein aufgelöster Schlüssel mit Herkunft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKey {
    pub source: KeySource,
    pub secret: ApiKeySecret,
}

/// Fehler der Schlüsselverwaltung. Meldungen nennen nie den Schlüssel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// Leerer Schlüssel beim Speichern.
    Empty,
    /// Schlüsselbund gesperrt oder nicht verfügbar (Meldung der Ablage, ohne Wert).
    KeychainUnavailable(String),
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("Kein API-Schlüssel eingegeben"),
            Self::KeychainUnavailable(m) => write!(f, "Schlüsselbund nicht verfügbar: {m}"),
        }
    }
}

impl std::error::Error for KeyError {}

/// Ergebnis von [`AiKeyStore::save`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// Im Schlüsselbund abgelegt.
    Keychain,
    /// Nur für die Sitzung gemerkt („merken“ aus).
    SessionOnly,
    /// Schlüsselbund nicht beschreibbar: für die Sitzung im Speicher, mit verständlicher Meldung (INTEG-002).
    SessionFallback { message: String },
}

/// Zustand eines Anbieters ohne Schlüsselwert (für Statusanzeige und Modellwahl).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyStatus {
    Present {
        source: KeySource,
        masked: String,
    },
    Missing,
    /// Schlüsselbund nicht lesbar und kein Sitzungsschlüssel.
    KeychainUnavailable {
        message: String,
    },
}

impl KeyStatus {
    /// Ob ein Schlüssel nutzbar ist.
    pub fn is_present(&self) -> bool {
        matches!(self, Self::Present { .. })
    }

    /// Statuswert für Einträge einer Aufgabe ohne nutzbaren Schlüssel (AI-009, AI-020).
    pub fn missing_status(&self) -> Option<AiStatus> {
        (!self.is_present()).then_some(AiStatus::MissingApiKey)
    }
}

/// Quelle der Umgebungsvariablen (injizierbar, damit Tests die Prozessumgebung nicht verändern).
pub type EnvLookup = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Zugriff auf die Schlüssel der KI-Anbieter.
pub struct AiKeyStore {
    store: Arc<dyn SecretStoreMut>,
    env: EnvLookup,
    session: Mutex<BTreeMap<AiKeyProvider, ApiKeySecret>>,
}

impl std::fmt::Debug for AiKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        f.debug_struct("AiKeyStore")
            .field("backend", &self.store.backend())
            .field("session_keys", &session.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl AiKeyStore {
    /// Store mit der Prozessumgebung als Quelle der Umgebungsvariablen.
    pub fn new(store: Arc<dyn SecretStoreMut>) -> Self {
        Self::with_env(store, Arc::new(|name: &str| std::env::var(name).ok()))
    }

    /// Store mit eigener Quelle der Umgebungsvariablen (Tests, CLI-Läufe).
    pub fn with_env(store: Arc<dyn SecretStoreMut>, env: EnvLookup) -> Self {
        Self {
            store,
            env,
            session: Mutex::new(BTreeMap::new()),
        }
    }

    fn session(&self) -> std::sync::MutexGuard<'_, BTreeMap<AiKeyProvider, ApiKeySecret>> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn env_key(&self, provider: AiKeyProvider) -> Option<ApiKeySecret> {
        (self.env)(provider.env_var()).and_then(|v| ApiKeySecret::new(&v))
    }

    /// Verwendeter Schlüssel: Umgebungsvariable vor Sitzung vor Schlüsselbund. Liest nur, schreibt nie.
    /// `Err` nur, wenn der Schlüsselbund nicht lesbar ist und weder Umgebung noch Sitzung einen Schlüssel liefern.
    pub fn resolve(&self, provider: AiKeyProvider) -> Result<Option<ResolvedKey>, KeyError> {
        if let Some(secret) = self.env_key(provider) {
            return Ok(Some(ResolvedKey {
                source: KeySource::Environment,
                secret,
            }));
        }
        if let Some(secret) = self.session().get(&provider).cloned() {
            return Ok(Some(ResolvedKey {
                source: KeySource::Session,
                secret,
            }));
        }
        match self.store.try_get(provider.account()) {
            Ok(value) => {
                Ok(value
                    .as_deref()
                    .and_then(ApiKeySecret::new)
                    .map(|secret| ResolvedKey {
                        source: KeySource::Keychain,
                        secret,
                    }))
            }
            Err(e) => Err(KeyError::KeychainUnavailable(e.message().to_string())),
        }
    }

    /// Zustand ohne Schlüsselwert.
    pub fn status(&self, provider: AiKeyProvider) -> KeyStatus {
        match self.resolve(provider) {
            Ok(Some(key)) => KeyStatus::Present {
                source: key.source,
                masked: key.secret.masked(),
            },
            Ok(None) => KeyStatus::Missing,
            Err(KeyError::KeychainUnavailable(message)) => {
                KeyStatus::KeychainUnavailable { message }
            }
            Err(KeyError::Empty) => KeyStatus::Missing,
        }
    }

    /// Ob für den Anbieter ein Schlüssel nutzbar ist (Eingang der Modellwahl, AI-011).
    pub fn has_key(&self, provider: AiKeyProvider) -> bool {
        self.status(provider).is_present()
    }

    /// Hinterlegt einen eingegebenen Schlüssel. `remember` = „Schlüssel merken“ (Standard an, INTEG-002): an → Eintrag
    /// `ai.<anbieter>` im Schlüsselbund (ersetzt einen bestehenden); aus → nur Sitzung, ein gespeicherter Eintrag wird
    /// entfernt, damit nach dem Neustart kein Schlüssel mehr vorhanden ist. Ist der Schlüsselbund nicht beschreibbar,
    /// gilt der Schlüssel für die Sitzung ([`SaveOutcome::SessionFallback`]).
    pub fn save(
        &self,
        provider: AiKeyProvider,
        key: &str,
        remember: bool,
    ) -> Result<SaveOutcome, KeyError> {
        let secret = ApiKeySecret::new(key).ok_or(KeyError::Empty)?;
        if !remember {
            self.session().insert(provider, secret);
            return match self.store.delete(provider.account()) {
                Ok(()) => Ok(SaveOutcome::SessionOnly),
                Err(e) => Ok(SaveOutcome::SessionFallback {
                    message: e.message().to_string(),
                }),
            };
        }
        match self.store.set(provider.account(), secret.expose()) {
            Ok(()) => {
                self.session().remove(&provider);
                Ok(SaveOutcome::Keychain)
            }
            Err(e) => {
                self.session().insert(provider, secret);
                Ok(SaveOutcome::SessionFallback {
                    message: e.message().to_string(),
                })
            }
        }
    }

    /// Entfernt den Schlüssel aus Sitzung und Schlüsselbund (eine Umgebungsvariable bleibt unberührt).
    pub fn remove(&self, provider: AiKeyProvider) -> Result<(), KeyError> {
        self.session().remove(&provider);
        self.store
            .delete(provider.account())
            .map_err(|e| KeyError::KeychainUnavailable(e.message().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: INTEG-002
    #[test]
    fn mask_shows_last_four_characters_only() {
        let key = ApiKeySecret::new("sk-TEST-INTEG-002-abcd").unwrap();
        assert_eq!(
            key.masked(),
            "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}abcd"
        );
        assert_eq!(
            ApiKeySecret::new("abc").unwrap().masked().chars().count(),
            8
        );
        assert!(ApiKeySecret::new("   ").is_none());
    }
}
