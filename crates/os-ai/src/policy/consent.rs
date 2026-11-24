//! Consent per (provider, task type, field set) (AI-018).
//!
//! A request is covered when a consent of the same provider and task type exists whose field set contains every
//! field of the request; a field that is added therefore asks again. Revocation works per provider and takes effect
//! for the next check, i.e. before the next request of any open entry. Persistence is injected through
//! [`ConsentStore`] (the app decides where the book lives, see report M4-K07).
use super::{dataflow::DataFlowRow, fields::FieldSet};
use os_model::ai::AiStatus;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, RwLock};

/// Message for CLI runs without `--ai-consent` (AI-018 via AI-022); the CLI prints it and runs no AI task.
pub const CLI_CONSENT_MISSING: &str =
    "AI tasks were not run: consent to the AI data flow is missing (pass --ai-consent).";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConsentKey {
    pub provider: String,
    pub task: String,
    pub fields: FieldSet,
}

impl ConsentKey {
    pub fn new(provider: impl Into<String>, task: impl Into<String>, fields: FieldSet) -> Self {
        Self {
            provider: provider.into(),
            task: task.into(),
            fields,
        }
    }
}

/// How a consent was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentSource {
    /// Dialog „Datenfluss bestätigen" (UI, M4-U01).
    Dialog,
    /// CLI switch `--ai-consent` (AI-022).
    CliFlag,
}

/// Logged consent: key, time and the data-flow row exactly as shown when consenting (AI-018 "protokolliert").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub key: ConsentKey,
    /// Unix time in milliseconds (UTC).
    pub given_at_ms: i64,
    pub source: ConsentSource,
    pub shown: Option<DataFlowRow>,
}

impl ConsentRecord {
    /// `Consent Given At` as RFC 3339 UTC (`2026-11-24T09:30:00Z`).
    pub fn given_at(&self) -> String {
        rfc3339_utc(self.given_at_ms)
    }
}

/// Result of a consent check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentCheck {
    Granted {
        given_at_ms: Option<i64>,
    },
    /// Ask (again). `added` are the fields not covered by an earlier consent of this provider and task (empty on a
    /// first request; all fields if there was none).
    Required {
        previous: Option<FieldSet>,
        added: FieldSet,
    },
}

impl ConsentCheck {
    pub fn is_granted(&self) -> bool {
        matches!(self, Self::Granted { .. })
    }

    /// Status of an entry that may not be sent (AI-020 `Pending (consent)`).
    pub fn status(&self) -> Option<AiStatus> {
        (!self.is_granted()).then_some(AiStatus::PendingConsent)
    }
}

/// All consents; serialisable for the app's persistence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentBook {
    #[serde(default = "format_version")]
    pub format_version: u32,
    records: Vec<ConsentRecord>,
}

fn format_version() -> u32 {
    1
}

impl Default for ConsentBook {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsentBook {
    pub fn new() -> Self {
        Self {
            format_version: 1,
            records: Vec::new(),
        }
    }

    pub fn records(&self) -> &[ConsentRecord] {
        &self.records
    }

    /// Record a consent; an existing consent with the same key is replaced (new time, new snapshot).
    pub fn grant(&mut self, record: ConsentRecord) {
        self.records.retain(|r| r.key != record.key);
        self.records.push(record);
        self.records.sort_by(|a, b| a.key.cmp(&b.key));
    }

    /// Remove all consents of a provider; returns the removed keys.
    pub fn revoke_provider(&mut self, provider: &str) -> Vec<ConsentKey> {
        let (removed, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.records)
            .into_iter()
            .partition(|r| r.key.provider == provider);
        self.records = kept;
        removed.into_iter().map(|r| r.key).collect()
    }

    /// Covering consent for (provider, task, fields), the most recent one if several cover.
    pub fn covering(
        &self,
        provider: &str,
        task: &str,
        fields: &FieldSet,
    ) -> Option<&ConsentRecord> {
        self.records
            .iter()
            .filter(|r| {
                r.key.provider == provider && r.key.task == task && r.key.fields.covers(fields)
            })
            .max_by_key(|r| r.given_at_ms)
    }

    pub fn check(&self, provider: &str, task: &str, fields: &FieldSet) -> ConsentCheck {
        if let Some(r) = self.covering(provider, task, fields) {
            return ConsentCheck::Granted {
                given_at_ms: Some(r.given_at_ms),
            };
        }
        let previous = self
            .records
            .iter()
            .filter(|r| r.key.provider == provider && r.key.task == task)
            .max_by_key(|r| r.given_at_ms)
            .map(|r| r.key.fields.clone());
        let added = match &previous {
            Some(p) => fields.added_to(p),
            None => fields.clone(),
        };
        ConsentCheck::Required { previous, added }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("consent book serialises")
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// Persistence of the consent book (implemented by the app/store; [`MemoryConsentStore`] for tests).
pub trait ConsentStore: Send + Sync {
    fn load(&self) -> Result<ConsentBook, String>;
    fn save(&self, book: &ConsentBook) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct MemoryConsentStore {
    book: Mutex<ConsentBook>,
    saves: Mutex<usize>,
}

impl MemoryConsentStore {
    pub fn saves(&self) -> usize {
        *self.saves.lock().expect("store lock")
    }
}

impl ConsentStore for MemoryConsentStore {
    fn load(&self) -> Result<ConsentBook, String> {
        Ok(self.book.lock().expect("store lock").clone())
    }
    fn save(&self, book: &ConsentBook) -> Result<(), String> {
        *self.book.lock().expect("store lock") = book.clone();
        *self.saves.lock().expect("store lock") += 1;
        Ok(())
    }
}

/// Where consent comes from in this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentMode {
    /// Interactive app: consent book from the dialog.
    Dialog,
    /// Headless run: `--ai-consent` replaces the dialog (AI-018 via AI-022); `false` = no AI task runs.
    Cli { flag: bool },
}

struct GateState {
    mode: ConsentMode,
    book: ConsentBook,
}

/// Shared consent gate. Cloning shares the state, so a revocation is visible to every worker before its next
/// request.
#[derive(Clone)]
pub struct ConsentGate {
    state: Arc<RwLock<GateState>>,
    store: Option<Arc<dyn ConsentStore>>,
}

impl std::fmt::Debug for ConsentGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.state.read().expect("consent lock");
        f.debug_struct("ConsentGate")
            .field("mode", &s.mode)
            .field("records", &s.book.records.len())
            .finish()
    }
}

impl ConsentGate {
    /// Interactive gate over a book, without persistence.
    pub fn dialog(book: ConsentBook) -> Self {
        Self {
            state: Arc::new(RwLock::new(GateState {
                mode: ConsentMode::Dialog,
                book,
            })),
            store: None,
        }
    }

    /// Interactive gate that loads from and saves to `store` on every grant/revocation.
    pub fn with_store(store: Arc<dyn ConsentStore>) -> Result<Self, String> {
        let book = store.load()?;
        let mut gate = Self::dialog(book);
        gate.store = Some(store);
        Ok(gate)
    }

    /// Headless gate: `--ai-consent` given or not.
    pub fn cli(flag: bool) -> Self {
        Self {
            state: Arc::new(RwLock::new(GateState {
                mode: ConsentMode::Cli { flag },
                book: ConsentBook::new(),
            })),
            store: None,
        }
    }

    pub fn mode(&self) -> ConsentMode {
        self.state.read().expect("consent lock").mode
    }

    /// Message for the CLI output when AI tasks were requested without `--ai-consent`; `None` otherwise.
    pub fn cli_message(&self) -> Option<&'static str> {
        matches!(self.mode(), ConsentMode::Cli { flag: false }).then_some(CLI_CONSENT_MISSING)
    }

    pub fn check(&self, key: &ConsentKey) -> ConsentCheck {
        let s = self.state.read().expect("consent lock");
        match s.mode {
            ConsentMode::Cli { flag: true } => ConsentCheck::Granted { given_at_ms: None },
            ConsentMode::Cli { flag: false } => ConsentCheck::Required {
                previous: None,
                added: key.fields.clone(),
            },
            ConsentMode::Dialog => s.book.check(&key.provider, &key.task, &key.fields),
        }
    }

    /// Record the dialog confirmation for `key` with the row that was shown.
    pub fn grant(
        &self,
        key: ConsentKey,
        at_ms: i64,
        shown: Option<DataFlowRow>,
    ) -> Result<(), String> {
        self.update(|book| {
            book.grant(ConsentRecord {
                key,
                given_at_ms: at_ms,
                source: ConsentSource::Dialog,
                shown,
            })
        })
    }

    /// Revoke all consents of a provider (AI-018 "Widerruf je Anbieter"); open entries of that provider get
    /// `Pending (consent)` at their next check.
    pub fn revoke_provider(&self, provider: &str) -> Result<Vec<ConsentKey>, String> {
        self.update(|book| book.revoke_provider(provider))
    }

    pub fn book(&self) -> ConsentBook {
        self.state.read().expect("consent lock").book.clone()
    }

    fn update<T>(&self, f: impl FnOnce(&mut ConsentBook) -> T) -> Result<T, String> {
        let mut s = self.state.write().expect("consent lock");
        let mut next = s.book.clone();
        let out = f(&mut next);
        if let Some(store) = &self.store {
            store.save(&next)?;
        }
        s.book = next;
        Ok(out)
    }
}

/// RFC 3339 UTC with second precision for a Unix time in milliseconds.
pub fn rfc3339_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 → (year, month, day) in the proleptic Gregorian calendar.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
