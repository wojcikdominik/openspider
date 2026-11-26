//! Serialisierte Schreibwarteschlange der App (K-20): eigener Thread, eigene Verbindung, `busy_timeout` 5 s.
//!
//! Alle Schreibzugriffe der App neben dem Engine-Writer (`config.set`, SERP-Bearbeitungen, Segmente, …) laufen als
//! Aufträge nacheinander auf einem einzigen Thread. Der Thread hält je Crawl eine eigene Lese-/Schreibverbindung
//! (mit `OS_TEXT` und `busy_timeout` 5 s); sperrt der Engine-Writer die Datei kurz, wartet der Auftrag, statt mit
//! `SQLITE_BUSY` zu scheitern. Die Datei wird nie migriert: ein Schema, das nicht dem der App entspricht, ist ein
//! Fehler (die Engine migriert beim Öffnen).
//!
//! Hintergrund-Verwaltung (M2-N31): Aufträge, die keine registrierte Crawl-Datei beschreiben und nicht auf eine Antwort
//! des Nutzers warten (Aufbewahrung beim Start und stündlich, OUT-229), laufen über [`WriteQueue::submit_maintenance`]
//! auf einer eigenen Wartungsspur mit eigenem Thread. Ein Nutzerauftrag der Hauptspur (`config.set`, Segmente,
//! View-Cache …) wartet daher nie hinter einer langen Verwaltungsrunde; untereinander bleiben die Wartungsaufträge
//! strikt geordnet. [`WriteQueue::flush`] wartet nur auf die Hauptspur.
//! Beenden (`quit::drain`, CRAWL-126) wartet ebenfalls nur auf die Hauptspur, nicht auf eine laufende Aufbewahrungsrunde:
//! Eine abgebrochene Runde ist absturzsicher (Löschen = Checkpoint, Datei entfernen, Indexeintrag; der Index liegt im
//! WAL, `list()` räumt Einträge ohne Datei auf) und wird beim nächsten Start wiederholt.
//! Abbau ([`WriteQueue::shutdown`], Drop der `App`) dagegen wartet begrenzt auf einen laufenden Wartungsauftrag und
//! tritt dem Thread bei, damit keine Verbindung mehr in Dateien schreibt, deren Verzeichnis der Besitzer der `App`
//! danach entfernt (SQLite blendet `-shm` per mmap ein; ein Zugriff nach dem Entfernen kann mit SIGBUS enden).
//!
//! Dies ist neben `pool` die einzige Datei von `os-app`, in der `rusqlite` direkt benannt werden darf.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use os_store::schema::{self, APPLICATION_ID, MIGRATIONS};
use os_store::StoreError;
use rusqlite::{Connection, OpenFlags};

use crate::dto::CmdError;
use crate::pool::CrawlKey;
use crate::runtime::error::{internal, no_crawl, panic_text, store_error};

/// Wartezeit auf eine gesperrte Datei, bevor ein Auftrag mit `Busy` scheitert (K-20).
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Höchste Wartezeit beim Abbau der Schreibwarteschlange auf einen laufenden Wartungsauftrag (OUT-229).
///
/// Ein Wartungsauftrag (Aufbewahrungsrunde) besteht aus kurzen SQLite-Schritten, von denen jeder höchstens
/// [`BUSY_TIMEOUT`] auf eine Sperre wartet; eine gewöhnliche Runde endet weit darunter. Länger wird nicht gewartet: Eine
/// abgebrochene Runde ist absturzsicher und wird beim nächsten Start wiederholt. Das Beenden der Desktop-App ist davon
/// nicht betroffen: Sie endet nach `quit::drain` mit dem Prozess, ohne die `App` abzubauen.
pub const MAINTENANCE_SHUTDOWN_WAIT: Duration = BUSY_TIMEOUT;

type Registry = Arc<Mutex<HashMap<CrawlKey, PathBuf>>>;

enum Msg {
    Job(Box<dyn FnOnce(&mut Worker) + Send>),
    Close,
}

struct Worker {
    paths: Registry,
    conns: HashMap<CrawlKey, Connection>,
    files: HashMap<CrawlKey, os_store::CrawlFile>,
}

impl Worker {
    fn connection(&mut self, key: &CrawlKey) -> Result<&mut Connection, CmdError> {
        if !self.conns.contains_key(key) {
            let path = self
                .paths
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(key)
                .cloned()
                .ok_or_else(|| no_crawl(&key.0))?;
            let conn = open_write(&path).map_err(|e| store_error(&e))?;
            self.conns.insert(key.clone(), conn);
        }
        self.conns
            .get_mut(key)
            .ok_or_else(|| internal("WriteQueue", "Verbindung fehlt"))
    }
}

fn open_write(path: &std::path::Path) -> Result<Connection, StoreError> {
    if !path.exists() {
        return Err(StoreError::NotFound(path.to_path_buf()));
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    if schema::application_id(&conn)? != APPLICATION_ID {
        return Err(StoreError::NotACrawlFile {
            path: path.to_path_buf(),
        });
    }
    let found = schema::user_version(&conn)?;
    let latest = schema::latest_version(MIGRATIONS);
    if found != latest {
        // Älter: die Engine migriert beim Öffnen. Neuer: die Datei bleibt unverändert.
        return Err(if found > latest {
            StoreError::NewerVersion {
                path: path.to_path_buf(),
                found,
                supported: latest,
                generator: "einer neueren OpenSpider-Version".to_string(),
            }
        } else {
            StoreError::MigrationRequired {
                path: path.to_path_buf(),
                found,
                supported: latest,
            }
        });
    }
    os_store::collation::ensure(&conn)?;
    Ok(conn)
}

/// Die einzige Schreibstelle der App neben dem Engine-Writer.
pub struct WriteQueue {
    tx: Mutex<Option<mpsc::Sender<Msg>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    paths: Registry,
    maintenance: Lane,
}

type Task = Box<dyn FnOnce() + Send>;

/// Wartungsspur: eigener Thread, Aufträge strikt in Einreihungsreihenfolge, unabhängig von der Hauptspur.
struct Lane {
    tx: Mutex<Option<mpsc::Sender<Task>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// Gesetzt beim Abbau: eingereihte, noch nicht begonnene Aufträge werden verworfen statt ausgeführt.
    closed: Arc<AtomicBool>,
    /// Wird getrennt, sobald der Thread endet (begrenztes Warten in [`Lane::shutdown`]).
    finished: Mutex<Option<mpsc::Receiver<()>>>,
}

impl Lane {
    fn start(name: &str) -> Lane {
        let (tx, rx) = mpsc::channel::<Task>();
        let (finished_tx, finished) = mpsc::channel::<()>();
        let closed = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&closed);
        let thread = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let _finished = finished_tx;
                while let Ok(task) = rx.recv() {
                    if stop.load(Ordering::SeqCst) {
                        // Verwerfen schließt den Rückkanal: der Aufrufer erhält „Auftrag verworfen“.
                        drop(task);
                        continue;
                    }
                    task();
                }
            })
            .ok();
        Lane {
            tx: Mutex::new(Some(tx)),
            thread: Mutex::new(thread),
            closed,
            finished: Mutex::new(Some(finished)),
        }
    }

    fn send(&self, task: Task) -> bool {
        let guard = self.tx.lock().unwrap_or_else(|p| p.into_inner());
        guard.as_ref().is_some_and(|tx| tx.send(task).is_ok())
    }

    /// Schließt die Spur: Noch nicht begonnene Aufträge werden verworfen, ein laufender Auftrag darf höchstens `wait`
    /// lang zu Ende laufen; danach wird der Thread beigetreten. Läuft er länger, bleibt er losgelöst (eine abgebrochene
    /// Aufbewahrungsrunde ist absturzsicher, s. Moduldoku). Aus dem Thread der Spur selbst wird nicht gewartet.
    fn shutdown(&self, wait: Duration) {
        self.closed.store(true, Ordering::SeqCst);
        self.tx.lock().unwrap_or_else(|p| p.into_inner()).take();
        let handle = self.thread.lock().unwrap_or_else(|p| p.into_inner()).take();
        let finished = self
            .finished
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        let Some(handle) = handle else {
            return;
        };
        if handle.thread().id() == std::thread::current().id() {
            return;
        }
        let ended = finished
            .is_none_or(|f| !matches!(f.recv_timeout(wait), Err(mpsc::RecvTimeoutError::Timeout)));
        if ended {
            let _ = handle.join();
        } else {
            tracing::warn!(
                wait_ms = wait.as_millis() as u64,
                "Wartungsauftrag läuft beim Abbau der Schreibwarteschlange noch; Thread bleibt losgelöst"
            );
        }
    }
}

impl std::fmt::Debug for WriteQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteQueue").finish_non_exhaustive()
    }
}

impl Default for WriteQueue {
    fn default() -> Self {
        WriteQueue::new()
    }
}

impl WriteQueue {
    /// Create a new crawl on the serialized writer thread (API-only sessions).
    pub async fn create_file(
        &self,
        key: CrawlKey,
        path: PathBuf,
        init: os_store::CrawlInit,
        initial: os_model::WriteBatch,
    ) -> Result<(), CmdError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let dest = path.clone();
        if !self.send(Msg::Job(Box::new(move |_| {
            let result = (|| -> Result<(), StoreError> {
                drop(os_store::CrawlFile::create(
                    &dest,
                    &init,
                    os_store::StorageProfile::Balanced,
                )?);
                let mut sink =
                    os_store::SqliteSink::open(&dest, os_store::StorageProfile::Balanced)?;
                os_store::CrawlSink::write(&mut sink, initial)?;
                sink.finish()
            })()
            .map_err(|e| store_error(&e));
            let _ = tx.send(result);
        }))) {
            return Err(internal("WriteQueue::create_file", "Writer closed"));
        }
        rx.await
            .map_err(|e| internal("WriteQueue::create_file", e.to_string()))??;
        self.register(key, path);
        Ok(())
    }

    /// Startet den Schreib-Thread.
    pub fn new() -> WriteQueue {
        let paths: Registry = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = mpsc::channel::<Msg>();
        let mut worker = Worker {
            paths: Arc::clone(&paths),
            conns: HashMap::new(),
            files: HashMap::new(),
        };
        let thread = std::thread::Builder::new()
            .name("os-app-write-queue".into())
            .spawn(move || {
                while let Ok(msg) = rx.recv() {
                    match msg {
                        Msg::Job(job) => job(&mut worker),
                        Msg::Close => break,
                    }
                }
            })
            .ok();
        WriteQueue {
            tx: Mutex::new(Some(tx)),
            thread: Mutex::new(thread),
            paths,
            maintenance: Lane::start("os-app-maintenance"),
        }
    }

    /// Macht eine Crawl-Datei unter `key` beschreibbar. Eine vorhandene Verbindung zu einem früheren Pfad wird beim
    /// nächsten Auftrag neu geöffnet.
    pub fn register(&self, key: CrawlKey, path: impl Into<PathBuf>) {
        let path = path.into();
        let changed = {
            let mut paths = self.paths.lock().unwrap_or_else(|p| p.into_inner());
            paths
                .insert(key.clone(), path.clone())
                .is_none_or(|old| old != path)
        };
        if changed {
            self.drop_connection(key);
        }
    }

    /// Entfernt den Crawl und schließt die Verbindung (nach den bereits eingereihten Aufträgen).
    pub fn unregister(&self, key: &CrawlKey) {
        self.paths
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(key);
        self.drop_connection(key.clone());
    }

    fn drop_connection(&self, key: CrawlKey) {
        self.send(Msg::Job(Box::new(move |w: &mut Worker| {
            w.conns.remove(&key);
            w.files.remove(&key);
        })));
    }

    fn send(&self, msg: Msg) -> bool {
        let guard = self.tx.lock().unwrap_or_else(|p| p.into_inner());
        guard.as_ref().is_some_and(|tx| tx.send(msg).is_ok())
    }

    /// Führt `f` mit der Schreibverbindung des Crawls aus. Aufträge laufen strikt nacheinander in der Reihenfolge des
    /// Einreihens.
    pub async fn submit<T: Send + 'static>(
        &self,
        crawl: CrawlKey,
        f: impl FnOnce(&mut rusqlite::Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, CmdError> {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<Result<T, CmdError>>();
        let job = Box::new(move |w: &mut Worker| {
            let result = match w.connection(&crawl) {
                Err(e) => Err(e),
                Ok(conn) => {
                    match std::panic::catch_unwind(AssertUnwindSafe(|| {
                        f(conn).map_err(|e| store_error(&e))
                    })) {
                        Ok(r) => r,
                        Err(p) => {
                            // nach einem Panic mitten in einer Transaktion ist die Verbindung unklar
                            w.conns.remove(&crawl);
                            Err(internal("WriteQueue::submit", panic_text(p.as_ref())))
                        }
                    }
                }
            };
            let _ = done_tx.send(result);
        });
        if !self.send(Msg::Job(job)) {
            return Err(internal("WriteQueue::submit", "Warteschlange geschlossen"));
        }
        done_rx
            .await
            .map_err(|_| internal("WriteQueue::submit", "Auftrag ohne Ergebnis verworfen"))?
    }

    /// Führt einen Verwaltungsauftrag auf demselben Schreib-Thread aus.
    pub async fn submit_admin<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> Result<T, CmdError> + Send + 'static,
    ) -> Result<T, CmdError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if !self.send(Msg::Job(Box::new(move |_| {
            let result = std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|p| {
                Err(internal("WriteQueue::submit_admin", panic_text(p.as_ref())))
            });
            let _ = tx.send(result);
        }))) {
            return Err(internal(
                "WriteQueue::submit_admin",
                "Warteschlange geschlossen",
            ));
        }
        rx.await
            .map_err(|_| internal("WriteQueue::submit_admin", "Auftrag verworfen"))?
    }

    /// Führt einen Verwaltungsauftrag im Hintergrund auf der Wartungsspur aus (eigener Thread, M2-N31).
    ///
    /// Für Arbeit, auf die kein Nutzer wartet und die keine registrierte Crawl-Datei beschreibt (Aufbewahrung,
    /// OUT-229). Aufträge der Hauptspur ([`submit`](Self::submit), [`submit_file`](Self::submit_file),
    /// [`submit_admin`](Self::submit_admin)) überholen einen laufenden Wartungsauftrag; Wartungsaufträge laufen
    /// untereinander strikt in Einreihungsreihenfolge.
    pub async fn submit_maintenance<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> Result<T, CmdError> + Send + 'static,
    ) -> Result<T, CmdError> {
        self.enqueue_maintenance(f).await
    }

    /// Wie [`submit_maintenance`](Self::submit_maintenance), reiht aber sofort ein; das Ergebnis hängt nicht an der
    /// Warteschlange. So kann eine Hintergrundaufgabe auf das Ergebnis warten, ohne die `App` am Leben zu halten: Wird
    /// die Warteschlange währenddessen abgebaut, wartet der Abbau auf den laufenden Auftrag (höchstens
    /// [`MAINTENANCE_SHUTDOWN_WAIT`]) und verwirft nicht begonnene ("Auftrag verworfen").
    pub fn enqueue_maintenance<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> Result<T, CmdError> + Send + 'static,
    ) -> impl std::future::Future<Output = Result<T, CmdError>> + Send + 'static {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let sent = self.maintenance.send(Box::new(move || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|p| {
                Err(internal(
                    "WriteQueue::submit_maintenance",
                    panic_text(p.as_ref()),
                ))
            });
            let _ = tx.send(result);
        }));
        async move {
            if !sent {
                return Err(internal(
                    "WriteQueue::submit_maintenance",
                    "Warteschlange geschlossen",
                ));
            }
            rx.await
                .map_err(|_| internal("WriteQueue::submit_maintenance", "Auftrag verworfen"))?
        }
    }

    /// Die Closure muss jede selbst eröffnete Transaktion vor der Rückkehr committen oder zurückrollen;
    /// sie darf keine Transaktion offen lassen (auch nicht im Fehlerfall).
    /// Datei-Auftrag, strikt mit SQL- und Verwaltungsaufträgen serialisiert, ohne implizite Migration.
    pub async fn submit_file<T: Send + 'static>(
        &self,
        key: &CrawlKey,
        f: impl FnOnce(&mut os_store::CrawlFile) -> Result<T, CmdError> + Send + 'static,
    ) -> Result<T, CmdError> {
        let key = key.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        if !self.send(Msg::Job(Box::new(move |w| {
            let result = (|| {
                if !w.files.contains_key(&key) {
                    let path = w
                        .paths
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| no_crawl(&key.0))?;
                    // Prüfen ohne Migration, genau wie bei submit.
                    drop(open_write(&path).map_err(|e| store_error(&e))?);
                    let file = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite)
                        .map_err(|e| store_error(&e))?;
                    file.conn()
                        .busy_timeout(BUSY_TIMEOUT)
                        .map_err(|e| store_error(&e.into()))?;
                    w.files.insert(key.clone(), file);
                }
                let file = w.files.get_mut(&key).expect("Datei gerade geöffnet");
                match std::panic::catch_unwind(AssertUnwindSafe(|| f(file))) {
                    Ok(result) => result,
                    Err(p) => {
                        w.files.remove(&key);
                        Err(internal("WriteQueue::submit_file", panic_text(p.as_ref())))
                    }
                }
            })();
            let _ = tx.send(result);
        }))) {
            return Err(internal(
                "WriteQueue::submit_file",
                "Warteschlange geschlossen",
            ));
        }
        rx.await
            .map_err(|_| internal("WriteQueue::submit_file", "Auftrag verworfen"))?
    }

    /// Wartet, bis alle bisher eingereihten Aufträge der Hauptspur gelaufen sind (die Wartungsspur zählt nicht).
    pub async fn flush(&self) {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        if self.send(Msg::Job(Box::new(move |_w: &mut Worker| {
            let _ = tx.send(());
        }))) {
            let _ = rx.await;
        }
    }

    /// Beendet beide Threads und schließt alle Verbindungen: die Hauptspur nach ihren eingereihten Aufträgen, die
    /// Wartungsspur nach dem laufenden Auftrag (höchstens [`MAINTENANCE_SHUTDOWN_WAIT`]); dort noch nicht begonnene
    /// Aufträge werden verworfen. Läuft beim Abbau der `App` (Drop), bevor ihre Verzeichnisse verschwinden können.
    pub fn shutdown(&self) {
        let _ = self.send(Msg::Close);
        self.tx.lock().unwrap_or_else(|p| p.into_inner()).take();
        let handle = self.thread.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(h) = handle {
            if h.thread().id() != std::thread::current().id() {
                let _ = h.join();
            }
        }
        self.maintenance.shutdown(MAINTENANCE_SHUTDOWN_WAIT);
    }
}

impl Drop for WriteQueue {
    fn drop(&mut self) {
        self.shutdown();
    }
}
