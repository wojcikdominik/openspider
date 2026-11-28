//! Register der laufenden KI-Läufe je Crawl-Datei (08 AI-020 [M4], Nachtrag K4-13): Pause, Stopp und Abbruch der
//! Crawl-Verwaltung erreichen jeden KI-Lauf der Datei über seine [`RunControl`]; „Beenden der App“ bricht alle ab
//! ([`ControlRegistry::abort_all`]) und wartet höchstens das Zeitbudget aus CRAWL-126 auf ihr Ende
//! ([`ControlRegistry::wait_idle`]). Daneben hält das Register den gemeinsamen Ledger einer Datei, solange ein Lauf
//! auf ihr läuft (AI-013: eine Sperre je Crawl; Befund B3-2 der Abnahme U01: `ai.costBudget` zeigt die offenen
//! Reservierungen des laufenden KI-Laufs).
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use os_model::budget::CrawlBudgetLedger;
use os_store::StoreError;

use crate::decisions::exec::RunControl;

fn key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[derive(Default)]
struct Entry {
    runs: Vec<(u64, RunControl)>,
    ledger: Option<(Arc<CrawlBudgetLedger>, usize)>,
}

/// Register aller KI-Läufe dieses Prozesses.
#[derive(Default)]
pub struct ControlRegistry {
    files: Mutex<HashMap<PathBuf, Entry>>,
    next: AtomicU64,
}

/// Das Register dieses Prozesses.
pub fn controls() -> &'static ControlRegistry {
    static REGISTRY: OnceLock<ControlRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ControlRegistry::default)
}

/// Abmeldung eines Laufs beim Verwerfen.
#[must_use]
pub struct ControlGuard {
    path: PathBuf,
    id: u64,
}

impl Drop for ControlGuard {
    fn drop(&mut self) {
        let mut files = controls().files.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(e) = files.get_mut(&self.path) {
            e.runs.retain(|(id, _)| *id != self.id);
            if e.runs.is_empty() && e.ledger.is_none() {
                files.remove(&self.path);
            }
        }
    }
}

/// Anteil am gemeinsamen Ledger einer Datei; der letzte gibt ihn frei.
pub struct LedgerGuard {
    path: PathBuf,
    ledger: Arc<CrawlBudgetLedger>,
}

impl std::ops::Deref for LedgerGuard {
    type Target = CrawlBudgetLedger;
    fn deref(&self) -> &CrawlBudgetLedger {
        &self.ledger
    }
}

impl Drop for LedgerGuard {
    fn drop(&mut self) {
        let mut files = controls().files.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(e) = files.get_mut(&self.path) {
            if let Some((_, users)) = e.ledger.as_mut() {
                *users -= 1;
                if *users == 0 {
                    e.ledger = None;
                }
            }
            if e.runs.is_empty() && e.ledger.is_none() {
                files.remove(&self.path);
            }
        }
    }
}

impl ControlRegistry {
    /// Meldet einen Lauf auf `path` an.
    pub fn register(&self, path: &Path, control: RunControl) -> ControlGuard {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let path = key(path);
        self.files
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(path.clone())
            .or_default()
            .runs
            .push((id, control));
        ControlGuard { path, id }
    }

    fn each(&self, path: Option<&Path>, f: impl Fn(&RunControl)) {
        let files = self.files.lock().unwrap_or_else(|p| p.into_inner());
        match path {
            Some(p) => {
                if let Some(e) = files.get(&key(p)) {
                    e.runs.iter().for_each(|(_, c)| f(c));
                }
            }
            None => files
                .values()
                .flat_map(|e| e.runs.iter())
                .for_each(|(_, c)| f(c)),
        }
    }

    /// Pause des Crawls: kein neuer Request, laufende werden übernommen.
    pub fn pause(&self, path: &Path) {
        self.each(Some(path), RunControl::pause);
    }

    /// „Gestoppt (endgültig)“: kein weiterer Request, laufende werden übernommen.
    pub fn stop(&self, path: &Path) {
        self.each(Some(path), RunControl::stop);
    }

    /// Schließen der Crawl-Datei: laufende Requests werden abgebrochen (Einträge `Pending`, keine Protokollzeile).
    pub fn abort(&self, path: &Path) {
        self.each(Some(path), RunControl::abort);
    }

    /// Geordnetes Beenden der App: alle Läufe abbrechen.
    pub fn abort_all(&self) {
        self.each(None, RunControl::abort);
    }

    /// Zahl der laufenden KI-Läufe auf `path`.
    pub fn running(&self, path: &Path) -> usize {
        self.files
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&key(path))
            .map_or(0, |e| e.runs.len())
    }

    /// Zahl aller laufenden KI-Läufe.
    pub fn total(&self) -> usize {
        self.files
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .map(|e| e.runs.len())
            .sum()
    }

    /// Wartet höchstens `budget`, bis kein KI-Lauf mehr läuft; `true` = keiner mehr.
    pub async fn wait_idle(&self, budget: Duration) -> bool {
        let deadline = std::time::Instant::now() + budget;
        loop {
            if self.total() == 0 {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Gemeinsamer Ledger der Datei: der vorhandene eines laufenden Laufs bzw. ein neu geöffneter (`open`).
    pub fn ledger(
        &self,
        path: &Path,
        open: impl FnOnce() -> Result<CrawlBudgetLedger, StoreError>,
    ) -> Result<LedgerGuard, StoreError> {
        let path = key(path);
        let mut files = self.files.lock().unwrap_or_else(|p| p.into_inner());
        let entry = files.entry(path.clone()).or_default();
        let ledger = match entry.ledger.as_mut() {
            Some((ledger, users)) => {
                *users += 1;
                ledger.clone()
            }
            None => {
                let ledger = Arc::new(open()?);
                entry.ledger = Some((ledger.clone(), 1));
                ledger
            }
        };
        Ok(LedgerGuard { path, ledger })
    }

    /// Ledger eines laufenden KI-Laufs auf `path` (für die Anzeige `ai.costBudget` und für Budgetänderungen).
    pub fn active_ledger(&self, path: &Path) -> Option<Arc<CrawlBudgetLedger>> {
        self.files
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&key(path))
            .and_then(|e| e.ledger.as_ref().map(|(l, _)| l.clone()))
    }
}
