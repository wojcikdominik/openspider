//! Besitzer eines KI-Laufs (08 AI-020 [M4], Unterpunkt „Aktiver Besitzer außerhalb von `Running`“, Abschnitt
//! „Wer schreibt“; Heartbeat und Besitzerangaben nach 01 CRAWL-125, Nachtrag K4-22).
//!
//! Ein KI-Lauf auf einem Crawl ohne laufenden Besitzer (pausiert, abgeschlossen, unterbrochen) trägt den eigenen
//! Prozess für seine Dauer als Besitzer ein: zu Beginn `owner_kind`, `owner_pid`, `heartbeat_at`
//! ([`os_store::owner::claim`]), danach alle [`HEARTBEAT_INTERVAL`] `heartbeat_at` ([`os_store::owner::beat`]) – auch
//! während der Aufrufer auf Antworten wartet –, am Ende einmal `heartbeat_at` = jetzt mit leerem `owner_pid`
//! ([`os_store::owner::release`]). `state` wird nie geschrieben.
//!
//! * Führt der eigene Prozess den Crawl bereits (laufend nach CRAWL-125 mit eigener PID, z. B. KI während des eigenen
//!   Crawls), ist der [`RunOwner`] **passiv**: Er schreibt nichts und leert `owner_pid` nicht; den Heartbeat schreibt
//!   allein die Engine. Endet oder pausiert der eigene Crawl, während der KI-Lauf weiterläuft, leert die Engine
//!   `owner_pid`; beim nächsten Schlag trägt sich der KI-Lauf dann selbst ein und führt den Heartbeat bis zu seinem Ende
//!   („Übernahme nach dem Ende des eigenen Crawls“, AI-020) – nie über einen fremden aktiven Besitzer.
//! * Mehrere KI-Läufe desselben Prozesses auf derselben Datei teilen sich einen Eintrag: Der erste trägt ein und
//!   schlägt den Takt, der letzte [`RunOwner::finish`] gibt frei.
//! * [`Drop`] ohne [`RunOwner::finish`] hält den Takt nur an (gilt wie ein Absturz: `owner_pid` bleibt, nach mehr als
//!   10 s gibt es keinen aktiven Besitzer mehr).
//!
//! Der Takt läuft über tokio-Zeit (in Tests anhaltbar), die geschriebenen Zeitpunkte stammen aus der injizierten
//! [`Clock`]. Jeder Schreibzugriff ist ein eigener Commit über eine eigene Verbindung zur Crawl-Datei, die außerhalb
//! der tokio-Arbeitsthreads läuft (`spawn_blocking`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use os_model::Clock;
use os_store::owner::{self, Claim};
use os_store::{CrawlFile, OpenMode, StoreError};
use tokio::sync::watch;

pub use os_store::owner::HEARTBEAT_INTERVAL;

/// Gemeinsamer Zustand eines eingetragenen Besitzers (eine Datei, ein Prozess).
struct Shared {
    file: Mutex<CrawlFile>,
    clock: Arc<dyn Clock>,
    pid: u32,
    kind: &'static str,
    /// Der Prozess hat sich eingetragen (beim Start oder später durch Übernahme).
    claimed: AtomicBool,
    stopped: AtomicBool,
    beats: watch::Sender<u64>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Shared {
    /// Ein Heartbeat; nach dem Anhalten nicht mehr.
    fn beat(&self) {
        let file = self.file.lock().unwrap_or_else(|p| p.into_inner());
        if self.stopped.load(Ordering::SeqCst) {
            return;
        }
        // Ein fehlgeschlagener Schlag (z. B. Sperre länger als das Busy-Timeout) wird nicht nachgeholt; der nächste
        // folgt im Takt (CRAWL-125: kein Nachholen).
        if let Ok(true) = owner::beat(file.conn(), self.kind, self.pid, self.clock.now()) {
            self.claimed.store(true, Ordering::SeqCst);
            self.beats.send_modify(|n| *n += 1);
        }
    }

    fn halt(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(task) = self.task.lock().unwrap_or_else(|p| p.into_inner()).take() {
            task.abort();
        }
    }
}

/// Eingetragene Besitzer dieses Prozesses je Datei, mit der Zahl ihrer KI-Läufe.
type Registry = Mutex<HashMap<PathBuf, (Arc<Shared>, usize)>>;

fn registry() -> &'static Registry {
    static ACTIVE: OnceLock<Registry> = OnceLock::new();
    ACTIVE.get_or_init(Default::default)
}

fn key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Besitzer eines KI-Laufs auf einer Crawl-Datei; siehe Moduldokumentation.
#[must_use = "ohne finish() bleibt owner_pid gesetzt (gilt wie ein Absturz)"]
pub struct RunOwner {
    key: PathBuf,
    shared: Option<Arc<Shared>>,
}

impl std::fmt::Debug for RunOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunOwner")
            .field("path", &self.key)
            .field("passive", &self.is_passive())
            .finish()
    }
}

impl RunOwner {
    /// Beginn eines KI-Laufs auf `crawl_path` mit `owner_kind` (`app`, `cli` oder `mcp-stdio`, CRAWL-125).
    ///
    /// * Führt ein anderer Prozess den Crawl (laufend nach CRAWL-125 oder aktiver Besitzer mit fremder PID) →
    ///   [`StoreError::Occupied`] mit Art und PID des Besitzers; nichts geschrieben.
    /// * Läuft der Crawl im eigenen Prozess → passiver Wert ([`RunOwner::is_passive`]), nichts geschrieben; endet oder
    ///   pausiert der Crawl, trägt sich der Wert beim nächsten Schlag ein (nie über einen fremden aktiven Besitzer).
    /// * Sonst trägt sich der Prozess ein (ein Commit) und startet den Takt; ein weiterer KI-Lauf desselben Prozesses
    ///   auf derselben Datei teilt diesen Eintrag.
    ///
    /// Muss innerhalb einer tokio-Laufzeit aufgerufen werden (der Takt ist eine tokio-Aufgabe). Der Eintrag ist ein
    /// kurzer synchroner Dateizugriff (höchstens das Busy-Timeout der Datei, 5 s); aus einem asynchronen Kontext mit
    /// knappen Arbeitsthreads ggf. über `spawn_blocking` aufrufen.
    pub fn start(
        crawl_path: &Path,
        owner_kind: &'static str,
        clock: Arc<dyn Clock>,
    ) -> Result<RunOwner, StoreError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            StoreError::InvalidInput("RunOwner::start braucht eine tokio-Laufzeit".into())
        })?;
        let key = key(crawl_path);
        let mut active = registry().lock().unwrap_or_else(|p| p.into_inner());
        if let Some((shared, users)) = active.get_mut(&key) {
            *users += 1;
            return Ok(RunOwner {
                key,
                shared: Some(shared.clone()),
            });
        }
        let pid = std::process::id();
        let file = CrawlFile::open(crawl_path, OpenMode::ReadWrite)?;
        let claimed = owner::claim(file.conn(), owner_kind, pid, clock.now())? == Claim::Claimed;
        // Auch ein passiver Wert schlägt den Takt: Endet oder pausiert der eigene Crawl, trägt `beat` den Prozess ein
        // (Übernahme nach dem Ende des eigenen Crawls, AI-020); solange der Crawl läuft, schreibt `beat` nichts.
        let shared = Arc::new(Shared {
            file: Mutex::new(file),
            clock,
            pid,
            kind: owner_kind,
            claimed: AtomicBool::new(claimed),
            stopped: AtomicBool::new(false),
            beats: watch::Sender::new(0),
            task: Mutex::new(None),
        });
        // Erster Schlag ein Intervall nach dem Eintrag; ein verspäteter Schlag verschiebt die folgenden.
        let mut next = tokio::time::Instant::now() + HEARTBEAT_INTERVAL;
        let weak = Arc::downgrade(&shared);
        let task = runtime.spawn(async move {
            loop {
                tokio::time::sleep_until(next).await;
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                if shared.stopped.load(Ordering::SeqCst) {
                    return;
                }
                let _ = tokio::task::spawn_blocking(move || shared.beat()).await;
                next = tokio::time::Instant::now() + HEARTBEAT_INTERVAL;
            }
        });
        *shared.task.lock().unwrap_or_else(|p| p.into_inner()) = Some(task);
        active.insert(key.clone(), (shared.clone(), 1));
        Ok(RunOwner {
            key,
            shared: Some(shared),
        })
    }

    /// Der Prozess ist (noch) nicht eingetragen, weil der eigene Crawl nach CRAWL-125 läuft; bis zur Übernahme schreibt
    /// dieser Wert nichts.
    pub fn is_passive(&self) -> bool {
        self.shared
            .as_ref()
            .is_none_or(|s| !s.claimed.load(Ordering::SeqCst))
    }

    /// Zahl der geschriebenen Heartbeats (ohne den Eintrag beim Start; eine Übernahme zählt als Heartbeat).
    pub fn heartbeats(&self) -> Option<watch::Receiver<u64>> {
        self.shared.as_ref().map(|s| s.beats.subscribe())
    }

    /// Ende des KI-Laufs (fertig, pausiert, gestoppt, Budget erreicht, Fehler, geordnetes Beenden): hält den Takt an
    /// und schreibt einmal `heartbeat_at` = jetzt mit leerem `owner_pid` – beim letzten KI-Lauf dieses Prozesses auf
    /// der Datei und nur, solange `owner_pid` noch den eigenen Prozess nennt und der Crawl nicht inzwischen in der
    /// eigenen Engine läuft. Ein (noch) passiver Wert schreibt daher nichts.
    pub fn finish(mut self) -> Result<(), StoreError> {
        let Some(shared) = self.detach() else {
            return Ok(());
        };
        shared.halt();
        let file = shared.file.lock().unwrap_or_else(|p| p.into_inner());
        owner::release(file.conn(), shared.pid, shared.clock.now()).map(|_| ())
    }

    /// Meldet diesen KI-Lauf ab; liefert den gemeinsamen Zustand, wenn es der letzte war.
    fn detach(&mut self) -> Option<Arc<Shared>> {
        let shared = self.shared.take()?;
        let mut active = registry().lock().unwrap_or_else(|p| p.into_inner());
        match active.get_mut(&self.key) {
            Some((current, users)) if Arc::ptr_eq(current, &shared) => {
                *users -= 1;
                if *users > 0 {
                    return None;
                }
                active.remove(&self.key);
                Some(shared)
            }
            _ => Some(shared),
        }
    }
}

impl Drop for RunOwner {
    /// Ohne [`RunOwner::finish`]: nur anhalten, nichts schreiben (gilt wie ein Absturz).
    fn drop(&mut self) {
        if let Some(shared) = self.detach() {
            shared.halt();
        }
    }
}
