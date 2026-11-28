//! Laufrahmen aller KI-Läufe (08 AI-020 [M4] mit den Nachträgen K4-13, 26.11.2026 und 27.11.2026; AI-012, AI-013,
//! AI-032, AI-039; Klärung K4-25 Variante A; M4-D05).
//!
//! Jeder KI-Lauf startet über diesen Rahmen – die Decision-Fragen jetzt, die Vorschläge (V01) und spätere Aufgaben
//! hängen sich an dieselben Bausteine:
//! * **Besitz:** [`job::owner_start`] bzw. [`job::owner_of`] binden `os_ai::run_owner::RunOwner` (M4-K11) als
//!   `OwnerLease` an `DecisionRun::owner` (`impl OwnerLease for RunOwner`); Beginn vor dem ersten Request, `finish` in
//!   jedem Ende-Pfad, nie beim Verwerfen (= Absturz). `StoreError::Interrupted` und `StoreError::Occupied` gehen
//!   unverändert an den Aufrufer ([`job::JobError`]).
//! * **Steuerung:** [`controls`] hält je Crawl-Datei die `RunControl` aller laufenden KI-Läufe (Pause, Stopp,
//!   Schließen der Datei, Beenden der App) und den gemeinsamen Ledger der Datei.
//! * **Umgebung:** [`env::RunEnv`] mit eigenen Adapter-Instanzen je Lauf ([`env::ProviderSetup::adapters`], Auflage E8:
//!   Kontingentsperre je KI-Lauf).
//! * **Lauf nach dem Crawl bzw. in der Pause:** [`job::run_decisions`] (App `ai.decisionsRun`; CLI- und MCP-Befehle
//!   bauen M4-C01/M4-C02 darauf).
//! * **Während des Crawls:** [`queue::DecisionQueue`] setzt `os_model::ai::hook::AiQueue` um (Übergabe in
//!   `EngineDeps::ai_queue`; ohne Schalter „KI (Vorschau)“ keine Warteschlange).
pub mod consent_file;
pub mod control;
pub mod env;
pub mod items;
pub mod job;
pub mod queue;

pub use consent_file::{FileConsentStore, CONSENT_FILE};
pub use control::{controls, ControlGuard, ControlRegistry, LedgerGuard};
pub use env::{AdapterSet, ProviderSetup, RunEnv, TablePrices};
pub use job::{
    estimate_decisions, run_decisions, run_decisions_blocking, DecisionsJob, JobConfirmation,
    JobError, JobOutcome, INTERRUPTED_DE, INTERRUPTED_EN,
};
pub use queue::{confirm_running, DecisionQueue, EnvFactory};

/// Letzte Meldungen der KI-Warteschlange (os-ai hat keine Protokollbibliothek; die Anwendung kann sie für die Diagnose
/// abholen). Höchstens 100 Einträge.
pub fn notes() -> Vec<String> {
    notes_store()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .cloned()
        .collect()
}

fn notes_store() -> &'static std::sync::Mutex<std::collections::VecDeque<String>> {
    static NOTES: std::sync::OnceLock<std::sync::Mutex<std::collections::VecDeque<String>>> =
        std::sync::OnceLock::new();
    NOTES.get_or_init(Default::default)
}

pub(crate) fn note(message: String) {
    let mut notes = notes_store().lock().unwrap_or_else(|p| p.into_inner());
    if notes.len() >= 100 {
        notes.pop_front();
    }
    notes.push_back(message);
}

/// KI-Warteschlange für CLI und MCP STDIO (M4-D05, Schritt 4): App-Einstellungen aus `settings_dir` (Schalter
/// „KI (Vorschau)“, Endpunkte, Schwärzung, Schwelle), Schlüssel aus `OPENSPIDER_AI_KEY_<ANBIETER>` bzw. dem Schlüsselbund
/// (AI-009), Einwilligung je Installation aus `ai-consent.json` (AI-018). `None` mit Schalter aus (Verhalten von 1.0).
pub fn standalone_queue(
    settings_dir: &std::path::Path,
    owner_kind: &'static str,
    clock: std::sync::Arc<dyn os_model::Clock>,
) -> Option<std::sync::Arc<dyn os_model::ai::hook::AiQueue>> {
    let settings = os_model::settings::AppSettings::load(settings_dir).ok()?;
    if !crate::PreviewSwitch::from_settings(&settings).is_on() {
        return None;
    }
    let dir = settings_dir.to_path_buf();
    let factory: EnvFactory = std::sync::Arc::new(move |cfg, kind| {
        let consent = crate::policy::ConsentGate::with_store(std::sync::Arc::new(
            FileConsentStore::new(dir.join(CONSENT_FILE)),
        ))?;
        let keys = crate::keys::AiKeyStore::new(os_fetch::secrets::secret_store_from_env());
        RunEnv::from_settings(&settings.ai_app, cfg, &keys, consent, clock.clone(), kind)
    });
    let _ = owner_kind;
    Some(std::sync::Arc::new(DecisionQueue::new(factory)))
}
