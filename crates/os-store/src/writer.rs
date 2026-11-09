//! `SqliteSink`: schreibt `WriteBatch`es in die Crawl-Datei (eine Transaktion je Batch).
//!
//! Reihenfolge innerhalb einer Transaktion: `new_urls` → `url_updates` → `pages` (Kindzeilen und Zähler) →
//! `queue` → `sitemaps`/`robots` → `meta`/`events`/`counters`. Alle Anweisungen sind vorbereitet und werden
//! im Statement-Cache der Verbindung gehalten.
//!
//! **Änderungsfeed** (M1-Plan 3.9, 3.14): Je Batch ist `n = crawl_meta.change_seq + 1`. Jede eingefügte oder
//! geänderte `urls`-Zeile – neue URLs, `UrlUpdate`s, Seiten (auch `replace`) – erhält `change_seq = n`, jede
//! Zeile von `url_counters`, deren Zähler sich ändern, ebenfalls; am Ende der Transaktion wird
//! `crawl_meta.change_seq = n` geschrieben ([`crate::queries::changed_since`] vereinigt beide Tabellen). Ein
//! vollständig leerer Batch ist ein No-op und erhöht die Sequenz nicht.
//!
//! **Zähler** (Schema v2, K-75): Die Zeile in `url_counters` legt ein Trigger zusammen mit der `urls`-Zeile an
//! (`0002_url_counters.sql`); [`crate::counters`] pflegt die Differenzen je Batch.
//!
//! **Index `links_target`** (K-74): Der Writer entfernt ihn beim Öffnen und baut ihn beim regulären Ende im letzten
//! Batch auf ([`crate::link_index`]).
//!
//! **Kennungen:** Nach dem Schreiben gelten `next_url_id > max(NewUrl.id)` und `next_seq > max(Push.seq)`
//! in `crawl_meta` (unabhängig davon, was die Engine in `meta` übergibt), damit ein Fortsetzen nie
//! Kennungen doppelt vergibt.
//!
//! **Kindzeilen:** `http_headers.direction` ist `request` oder `response`; `pagination.rel` ist `next` oder
//! `prev`; Quellen (`source`) sind die `as_str()`-Werte der Modell-Enums; `directives.tokens` ist
//! kommagetrennt; `position` der Hinweis-Tabellen ist `order` des Parsers.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use os_model::time::{Clock, SystemClock};
use os_model::{
    CanonicalRow, DirectiveRow, HeaderRow, HintSource, HreflangRow, ImageRow, LinkRow, PageFacts,
    PageWrite, PaginationRow, QueueOp, RefreshRow, RobotsWrite, SitemapWrite, UrlId, UrlUpdate,
    WriteBatch,
};
use os_model::{ContentClass, CrawlState, ElementPair, NewUrl};
use rusqlite::types::Value;
use rusqlite::{
    params, params_from_iter, Connection, OptionalExtension, Transaction, TransactionBehavior,
};

use crate::checkpointer::Checkpointer;
use crate::collation;
use crate::counters::{BatchCounters, Contribution, InternalFlags};
use crate::error::StoreError;
use crate::link_index;
use crate::open::{CrawlFile, OpenMode};
use crate::profile::{self, StorageProfile};
use crate::sink::CrawlSink;

/// Anzahl vorbereiteter Anweisungen, die die Verbindung vorhält.
const STATEMENT_CACHE: usize = 128;

/// Standard-Senke der Engine.
///
/// **WAL:** Standardmäßig checkpointet SQLite im `COMMIT` (`wal_autocheckpoint`); die Schwelle wächst mit der
/// Datei von 256 bis 512 MiB. Ein von außen gesetzter Wert bleibt unangetastet.
///
/// **Hintergrund-Checkpoint (K-76, optional):** [`SqliteSink::with_background_checkpoint`] stellt
/// `wal_autocheckpoint` auf 0; ein eigener Thread ([`crate::checkpointer`]) führt `PASSIVE`-Checkpoints aus.
/// Er ist **nicht** der Standard: Die Messung M1-P32 (Store-Benchmark mit 1 Mio. Seiten und Kurzprobe mit 250 000
/// Seiten) zeigt ihn langsamer als den Checkpoint im Commit, weil SQLite das WAL nur zurücksetzt, wenn der nächste
/// Schreibvorgang nach dem Ende eines Durchlaufs beginnt (siehe `probe-p32/README.md`).
/// [`SqliteSink::finish`] und [`CrawlSink::checkpoint`] halten den Thread an und führen den kurzen
/// `TRUNCATE`-Checkpoint aus (Pause, Ende); der nächste Schreibaufruf startet ihn wieder.
pub struct SqliteSink {
    file: CrawlFile,
    profile: StorageProfile,
    clock: Arc<dyn Clock>,
    /// Welche URL-Kennungen intern sind (für die Zähler, ohne Lesezugriff auf die breite `urls`-Zeile).
    internal: InternalFlags,
    /// Der Einfüge-Trigger für die Zählerzeilen ist entfernt (der Writer legt die Zeilen selbst an); Ende, Pause und
    /// Schließen stellen ihn wieder her, der nächste Schreibaufruf entfernt ihn erneut.
    insert_trigger_dropped: bool,
    /// Zuletzt gesetzte Schwelle von `PRAGMA wal_autocheckpoint` (Seiten); `None`, sobald jemand anderes sie
    /// geändert hat (dann stellt der Sink sie nicht mehr um) oder der Hintergrund-Checkpoint sie auf 0 gestellt hat.
    checkpoint_pages: Option<i64>,
    /// Der Hintergrund-Checkpoint ist eingeschaltet ([`SqliteSink::with_background_checkpoint`]).
    background_enabled: bool,
    /// Jemand anderes hat `PRAGMA wal_autocheckpoint` gesetzt: der Sink startet keinen Hintergrund-Checkpoint.
    external_autocheckpoint: bool,
    /// Der Hintergrund-Thread ließ sich nicht starten (Diagnose-Zähler); der Sink versucht es nicht noch einmal.
    background_failed: bool,
    /// Grenzen des Auslösers in Seiten (Standard: [`profile::WAL_CHECKPOINT_MIN_PAGES`] bis
    /// [`profile::WAL_CHECKPOINT_MAX_PAGES`]).
    checkpoint_bounds: (i64, i64),
    background: Option<Checkpointer>,
    /// Diagnoseschalter `OS_STORE_WAL_EXPLICIT=1` (N22): `wal_autocheckpoint` steht auf 0, der Writer führt das WAL
    /// nach dem Commit selbst zurück (`PASSIVE`, dieselbe Schwelle wie der Auslöser im Commit), damit Commit und
    /// Rückführung getrennt gemessen werden. Standard aus.
    wal_explicit: bool,
    /// Geschätzte Frames im WAL (Summe der vom Cache geschriebenen Seiten seit der letzten vollständigen
    /// Rückführung); nur mit `wal_explicit`.
    wal_frames: u64,
    /// Größte URL-Kennung, ab der die Indizes von `urls` mit zufälligen Schlüsseln ([`crate::url_index`]) entfallen.
    url_index_drop_at: i64,
    /// Größte bisher gesehene URL-Kennung (aus der Datei beim ersten Schreiben und den neuen URLs).
    max_url_id: Option<i64>,
    /// Die Indizes sind in diesem Lauf schon entfernt.
    url_indexes_dropped: bool,
}

/// Obergrenze der WAL-Größe in Seiten, ab der der Writer kurz auf den Hintergrund-Checkpoint wartet (1 GiB bei 4 KiB
/// Seitengröße, mindestens das Vierfache des Auslösers). Im Normalbetrieb wird sie nie erreicht (`wait_checkpoint_n` = 0).
const WAL_THROTTLE_MIN_PAGES: i64 = 262_144;
/// Längste Wartezeit des Writers auf einen Durchlauf des Hintergrund-Checkpoints je Schreibaufruf.
const WAL_THROTTLE_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

impl SqliteSink {
    /// Öffnet die Crawl-Datei zum Schreiben: registriert die Kollation `OS_TEXT`, setzt die PRAGMAs des
    /// Profils (CRAWL-124), entfernt den Index `links_target` (K-74, er entsteht erst beim regulären Ende, siehe
    /// [`crate::link_index`]) und dimensioniert den Statement-Cache.
    pub fn open(path: &Path, profile: StorageProfile) -> Result<Self, StoreError> {
        let file = CrawlFile::open(path, OpenMode::ReadWrite)?;
        collation::ensure(file.conn())?;
        profile::apply(file.conn(), profile)?;
        link_index::drop_index(file.conn())?;
        file.conn()
            .set_prepared_statement_cache_capacity(STATEMENT_CACHE);
        let sink = SqliteSink {
            file,
            profile,
            clock: Arc::new(SystemClock),
            internal: InternalFlags::default(),
            insert_trigger_dropped: false,
            checkpoint_pages: Some(profile::WAL_CHECKPOINT_MIN_PAGES),
            background_enabled: false,
            external_autocheckpoint: false,
            background_failed: false,
            checkpoint_bounds: (
                profile::WAL_CHECKPOINT_MIN_PAGES,
                profile::WAL_CHECKPOINT_MAX_PAGES,
            ),
            background: None,
            wal_explicit: false,
            wal_frames: 0,
            url_index_drop_at: crate::url_index::drop_threshold(),
            max_url_id: None,
            url_indexes_dropped: false,
        };
        // Diagnoseschalter für Großläufe (N22): Grenzen der Checkpoint-Schwelle in Seiten, `min:max`.
        let sink = match std::env::var("OS_STORE_WAL_BOUNDS").ok().and_then(|v| {
            let (a, b) = v.split_once(':')?;
            Some((a.trim().parse::<i64>().ok()?, b.trim().parse::<i64>().ok()?))
        }) {
            Some((min, max)) => sink.with_checkpoint_bounds(min, max)?,
            None => sink,
        };
        // Diagnoseschalter für Großläufe (N21): ohne Codeänderung zwischen beiden Verfahren vergleichen.
        if std::env::var_os("OS_STORE_BACKGROUND_CHECKPOINT").is_some_and(|v| v == "1") {
            return sink.with_background_checkpoint();
        }
        if std::env::var_os("OS_STORE_WAL_EXPLICIT").is_some_and(|v| v == "1") {
            return sink.with_explicit_wal_return();
        }
        Ok(sink)
    }

    /// Lässt die Indizes von `urls` mit zufälligen Schlüsseln ([`crate::url_index`]) auch in einem großen Crawl
    /// stehen (Vergleichsläufe).
    pub fn keep_url_indexes(mut self) -> Self {
        self.url_index_drop_at = i64::MAX;
        self
    }

    /// Setzt die Schwelle (größte URL-Kennung), ab der die Indizes von `urls` entfallen (Standard
    /// [`crate::url_index::DROP_FROM_URLS`]); für Tests mit kleinen Dateien.
    pub fn with_url_index_drop_at(mut self, urls: i64) -> Self {
        self.url_index_drop_at = urls;
        self
    }

    /// Entfernt die Indizes von `urls` vor dem Schreiben des Batches, der die Schwelle erreicht (einmal je Lauf).
    fn drop_url_indexes_when_large(&mut self, batch: &WriteBatch) -> Result<(), StoreError> {
        if self.url_indexes_dropped || self.url_index_drop_at == i64::MAX {
            return Ok(());
        }
        let known =
            match self.max_url_id {
                Some(m) => m,
                None => self.file.conn().query_row(
                    "SELECT COALESCE(MAX(id), 0) FROM urls",
                    [],
                    |r| r.get::<_, i64>(0),
                )?,
            };
        let max = batch
            .new_urls
            .iter()
            .map(|u| u.id.0)
            .chain(batch.serp_seeds.iter().map(|u| u.id.0))
            .fold(known, i64::max);
        self.max_url_id = Some(max);
        if max >= self.url_index_drop_at {
            crate::url_index::drop_indexes(self.file.conn())?;
            self.url_indexes_dropped = true;
        }
        Ok(())
    }

    /// Diagnose (N22): Rückführung des WAL durch den Writer selbst, getrennt vom `COMMIT` gemessen
    /// (`wal_cp_*`). Gleiche Arbeit und gleicher Auslöser wie `wal_autocheckpoint`, nur als eigener Aufruf nach dem
    /// Commit. Nicht der Standard.
    pub fn with_explicit_wal_return(mut self) -> Result<Self, StoreError> {
        self.file
            .conn()
            .pragma_update(None, "wal_autocheckpoint", 0)?;
        self.checkpoint_pages = None;
        self.wal_explicit = true;
        Ok(self)
    }

    /// Verbucht die Länge der `-wal`-Datei und führt im Diagnosemodus das WAL nach dem Commit zurück.
    fn after_commit(&mut self, written: &Written) {
        let mut wal = self.file.path().as_os_str().to_owned();
        wal.push("-wal");
        if let Ok(meta) = std::fs::metadata(&wal) {
            crate::sink::diag::record_wal_bytes(written.bucket, meta.len());
        }
        if !self.wal_explicit {
            return;
        }
        self.wal_frames = self.wal_frames.saturating_add(written.cache_write);
        let conn = self.file.conn();
        let (min_pages, max_pages) = self.checkpoint_bounds;
        let pages = conn
            .pragma_query_value(None, "page_count", |r| r.get::<_, i64>(0))
            .unwrap_or(0);
        let trigger = profile::wal_checkpoint_pages_between(pages, min_pages, max_pages);
        if i64::try_from(self.wal_frames).unwrap_or(i64::MAX) < trigger {
            return;
        }
        let t = std::time::Instant::now();
        // Antwort: (busy, Frames im WAL, davon zurückgeführt)
        let result = conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |r| {
            Ok((r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
        });
        match result {
            Ok((log, done)) => {
                let (log, done) = (log.max(0) as u64, done.max(0) as u64);
                crate::sink::diag::record_wal_return(
                    t,
                    written.bucket,
                    log,
                    log.saturating_sub(done),
                );
                if done >= log {
                    self.wal_frames = 0;
                }
            }
            Err(_) => crate::sink::diag::record_background_error(),
        }
    }

    /// Schaltet den Hintergrund-Checkpoint (K-76) ein: `wal_autocheckpoint = 0`, ein Thread führt `PASSIVE`-Checkpoints
    /// aus, sobald das WAL den Auslöser erreicht (siehe [`SqliteSink::with_checkpoint_bounds`]). Nicht der Standard,
    /// siehe die Beschreibung der Senke.
    pub fn with_background_checkpoint(mut self) -> Result<Self, StoreError> {
        self.file
            .conn()
            .pragma_update(None, "wal_autocheckpoint", 0)?;
        self.checkpoint_pages = None;
        self.background_enabled = true;
        self.limit_wal_file()?;
        Ok(self)
    }

    /// Setzt die Grenzen der Checkpoint-Schwelle (Seiten) und die Schwelle auf die Untergrenze; sie wächst danach
    /// mit der Datei bis zur Obergrenze (Checkpoint im Commit bzw. Auslöser des Hintergrund-Checkpoints). Für Tests und
    /// Messläufe mit kleinen Dateien.
    pub fn with_checkpoint_bounds(
        mut self,
        min_pages: i64,
        max_pages: i64,
    ) -> Result<Self, StoreError> {
        let min_pages = min_pages.max(1);
        self.checkpoint_bounds = (min_pages, max_pages.max(min_pages));
        if self.background_enabled {
            // ein laufender Thread startet mit den neuen Grenzen beim nächsten Schreiben neu
            self.stop_background();
            self.limit_wal_file()?;
        } else if !self.wal_explicit {
            self.file
                .conn()
                .pragma_update(None, "wal_autocheckpoint", min_pages)?;
            self.checkpoint_pages = Some(min_pages);
        }
        Ok(self)
    }

    /// Ersetzt die Zeitquelle (Regel 2.8): sie liefert `robots_files.fetched_at`.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Lässt die Checkpoint-Schwelle im Commit mit der Datei wachsen ([`profile::wal_checkpoint_pages`]); ein von
    /// außen gesetzter Wert (z. B. im Lasttest) bleibt unangetastet. Fehler sind unkritisch: die Schwelle bleibt dann.
    fn adapt_checkpoint_threshold(&mut self) {
        let Some(current) = self.checkpoint_pages else {
            return;
        };
        let conn = self.file.conn();
        let set: Result<i64, _> = conn.pragma_query_value(None, "wal_autocheckpoint", |r| r.get(0));
        if set.ok() != Some(current) {
            self.checkpoint_pages = None;
            return;
        }
        let Ok(pages) = conn.pragma_query_value(None, "page_count", |r| r.get::<_, i64>(0)) else {
            return;
        };
        let (min_pages, max_pages) = self.checkpoint_bounds;
        let want = profile::wal_checkpoint_pages_between(pages, min_pages, max_pages);
        // erst ab 25 % Zuwachs umstellen (die Datei wächst nur)
        if want > current + current / 4
            && conn.pragma_update(None, "wal_autocheckpoint", want).is_ok()
        {
            self.checkpoint_pages = Some(want);
        }
    }

    /// `journal_size_limit`: Nach einem Zurücksetzen des WAL kürzt SQLite die Datei auf diese Größe (ein Viertel des
    /// kleinsten Auslösers, 1 bis 64 MiB), damit die Dateilänge der Zahl der Frames entspricht, auf die der Thread
    /// seine Auslöser stützt.
    fn limit_wal_file(&self) -> Result<(), StoreError> {
        let conn = self.file.conn();
        let page_size: i64 = conn.pragma_query_value(None, "page_size", |r| r.get(0))?;
        let limit = (self.checkpoint_bounds.0.saturating_mul(page_size) / 4)
            .clamp(1024 * 1024, 64 * 1024 * 1024);
        conn.pragma_update(None, "journal_size_limit", limit)?;
        Ok(())
    }

    /// Hält den Hintergrund-Checkpoint am Laufen: erkennt einen von außen gesetzten `wal_autocheckpoint` (dann
    /// bleibt es dabei) und startet den Thread beim ersten Schreiben nach dem Öffnen, nach einer Pause oder
    /// nach geänderten Grenzen.
    fn drive_background(&mut self) {
        if self.external_autocheckpoint {
            return;
        }
        let set: Result<i64, _> =
            self.file
                .conn()
                .pragma_query_value(None, "wal_autocheckpoint", |r| r.get(0));
        if matches!(set, Ok(n) if n != 0) {
            self.external_autocheckpoint = true;
            self.stop_background();
            return;
        }
        if self.background.is_none() && !self.background_failed {
            match Checkpointer::start(
                self.file.path(),
                self.profile.synchronous_during_crawl(),
                self.checkpoint_bounds,
            ) {
                Ok(thread) => self.background = Some(thread),
                Err(_) => {
                    // Der Crawl läuft ohne Hintergrund-Checkpoint weiter; der Abschluss-Checkpoint bleibt.
                    self.background_failed = true;
                    crate::sink::diag::record_background_error();
                }
            }
        }
    }

    /// Wartet kurz auf den Hintergrund-Checkpoint, wenn das WAL weit über den Auslöser hinaus gewachsen ist.
    fn throttle(&self) {
        let Some(background) = &self.background else {
            return;
        };
        let cap = (4 * background.trigger_pages()).max(WAL_THROTTLE_MIN_PAGES);
        if background.wal_frames() > cap {
            let t = std::time::Instant::now();
            background.wait_for_pass(WAL_THROTTLE_WAIT);
            crate::sink::diag::record_checkpoint_wait(t);
        }
    }

    /// Entfernt den Einfüge-Trigger vor dem ersten Schreiben (einmal je Lauf zwischen zwei Wiederherstellungen).
    fn drop_insert_trigger(&mut self) -> Result<(), StoreError> {
        if !self.insert_trigger_dropped {
            crate::counters::drop_insert_trigger(self.file.conn())?;
            self.insert_trigger_dropped = true;
        }
        Ok(())
    }

    /// Stellt den Einfüge-Trigger wieder her, wenn er entfernt war.
    fn restore_insert_trigger(&mut self) -> Result<(), StoreError> {
        if self.insert_trigger_dropped {
            crate::counters::restore_insert_trigger(self.file.conn())?;
            self.insert_trigger_dropped = false;
        }
        Ok(())
    }

    /// Hält den Hintergrund-Thread an und wartet auf ihn.
    fn stop_background(&mut self) {
        if let Some(mut thread) = self.background.take() {
            thread.stop();
        }
    }

    /// Aktuelle Schwelle des Checkpoints in Seiten (WAL-Frames): bei laufendem Hintergrund-Thread dessen Auslöser, der mit
    /// der Dateigröße mitwächst, sonst die Schwelle von `wal_autocheckpoint` bzw. die Untergrenze.
    pub fn checkpoint_trigger_pages(&self) -> i64 {
        match &self.background {
            Some(thread) => thread.trigger_pages(),
            None => self.checkpoint_pages.unwrap_or(self.checkpoint_bounds.0),
        }
    }

    /// Ob ein Hintergrund-Checkpoint läuft (nicht bei von außen gesetztem `wal_autocheckpoint`, nicht vor dem ersten
    /// Schreiben, nicht zwischen Pause und dem nächsten Schreiben).
    pub fn background_checkpoint_running(&self) -> bool {
        self.background.is_some()
    }

    /// Der erste Fehler des Hintergrund-Threads (Diagnose; der Crawl läuft weiter).
    pub fn background_checkpoint_error(&self) -> Option<String> {
        self.background.as_ref().and_then(Checkpointer::last_error)
    }

    /// Die geöffnete Crawl-Datei (Lesezugriff, z. B. für Diagnose und Tests).
    pub fn file(&self) -> &CrawlFile {
        &self.file
    }

    /// Das Leistungsprofil dieser Senke.
    pub fn profile(&self) -> StorageProfile {
        self.profile
    }

    /// Schließt den Crawl ab: hält den Hintergrund-Checkpoint an, stellt `synchronous` auf den Wert nach dem Crawl und
    /// führt das WAL vollständig in die Hauptdatei zurück (CRAWL-124).
    pub fn finish(&mut self) -> Result<(), StoreError> {
        self.stop_background();
        self.restore_insert_trigger()?;
        profile::settle(self.file.conn(), self.profile)?;
        wal_checkpoint(self.file.conn())
    }
}

impl Drop for SqliteSink {
    fn drop(&mut self) {
        self.stop_background();
        let _ = self.restore_insert_trigger();
        // Der Abschluss-Checkpoint beim Schließen der Verbindung soll mit `synchronous = NORMAL` laufen.
        let _ = profile::settle(self.file.conn(), self.profile);
    }
}

/// Längste Wartezeit des abschließenden Checkpoints auf laufende Lesezugriffe (CRAWL-126, OS-Festlegung vom
/// 01.11.2026). Hält ein Leser länger einen älteren Stand, bleibt der Rest im (synchronisierten) WAL; die App führt
/// ihn im Hintergrund zurück ([`crate::open::merge_wal`]).
pub const CHECKPOINT_READER_WAIT: std::time::Duration = std::time::Duration::from_millis(100);

/// Wartezeit der Verbindung auf Sperren außerhalb des abschließenden Checkpoints.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Längste einzelne Schlafpause des begrenzten Busy-Handlers.
const BOUNDED_BUSY_STEP: std::time::Duration = std::time::Duration::from_millis(2);

thread_local! {
    /// Wanduhr-Frist des laufenden begrenzten Aufrufs auf diesem Thread.
    static BUSY_DEADLINE: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// Busy-Handler mit Wanduhr-Frist. `busy_timeout` begrenzt nur die Summe der *angeforderten* Schlafpausen; macOS
/// überschreitet einzelne Pausen unter Last deutlich (CI: 677 ms statt 100 ms). Hier zählt die gemessene Zeit.
fn bounded_busy(_attempts: i32) -> bool {
    BUSY_DEADLINE.with(|deadline| {
        let Some(deadline) = deadline.get() else {
            return false;
        };
        let now = std::time::Instant::now();
        if now >= deadline {
            return false;
        }
        std::thread::sleep((deadline - now).min(BOUNDED_BUSY_STEP));
        true
    })
}

/// Führt einen `wal_checkpoint`-Aufruf aus, der höchstens `wait` (Wanduhr) auf Leser und Schreiber wartet, und stellt
/// danach den normalen Busy-Timeout der Verbindung wieder her (CRAWL-126). Auch für den abschließenden Checkpoint der
/// Crawl-Analyse (CRAWL-141 Ablaufpunkt 7).
pub fn checkpoint_within<T>(
    conn: &Connection,
    wait: std::time::Duration,
    sql: &str,
    row: impl FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Result<T, StoreError> {
    /// Clears the deadline and restores the normal busy timeout, also when the call panics.
    struct Restore<'c>(&'c Connection);
    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            BUSY_DEADLINE.with(|d| d.set(None));
            let _ = self.0.busy_timeout(BUSY_TIMEOUT);
        }
    }
    BUSY_DEADLINE.with(|d| d.set(Some(std::time::Instant::now() + wait)));
    let guard = Restore(conn);
    set_bounded_busy(conn)?;
    let value = conn.query_row(sql, [], row)?;
    drop(guard);
    // The restore in the guard cannot report errors; check the normal timeout explicitly.
    conn.busy_timeout(BUSY_TIMEOUT)?;
    Ok(value)
}

/// Installiert [`bounded_busy`] direkt über die C-Schnittstelle. Seit rusqlite 0.38 verlangt
/// `Connection::busy_handler` `&mut Connection` (es legt eine Closure in den Client-Daten der Verbindung ab); die
/// Aufrufer von [`checkpoint_within`] halten die Verbindung aber nur geteilt (Crawl-Datei, Analyse). Verhalten wie mit
/// rusqlite 0.37: ein Funktionszeiger ohne Nutzdaten, den `busy_timeout` wieder ersetzt.
fn set_bounded_busy(conn: &Connection) -> rusqlite::Result<()> {
    unsafe extern "C" fn callback(
        _data: *mut std::ffi::c_void,
        attempts: std::ffi::c_int,
    ) -> std::ffi::c_int {
        std::ffi::c_int::from(std::panic::catch_unwind(|| bounded_busy(attempts)).unwrap_or(false))
    }
    // SAFETY: `handle()` ist der gültige Zeiger dieser offenen Verbindung; der Rückruf ist eine statische Funktion
    // ohne Nutzdaten und fängt Panics ab, bevor sie die C-Grenze erreichen.
    let rc = unsafe {
        rusqlite::ffi::sqlite3_busy_handler(conn.handle(), Some(callback), std::ptr::null_mut())
    };
    if rc == rusqlite::ffi::SQLITE_OK {
        Ok(())
    } else {
        Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rc),
            None,
        ))
    }
}

fn wal_checkpoint(conn: &Connection) -> Result<(), StoreError> {
    let phase = scale_phase_start(conn);
    let t = std::time::Instant::now();
    // Antwort: (busy, Frames im WAL, davon zurückgeführt)
    let result = checkpoint_within(
        conn,
        CHECKPOINT_READER_WAIT,
        "PRAGMA wal_checkpoint(TRUNCATE)",
        |r| r.get::<_, i64>(1),
    );
    crate::sink::diag::record_checkpoint(t, result.as_ref().map_or(0, |n| (*n).max(0) as u64));
    result?;
    scale_phase_end(conn, "checkpoint", phase);
    Ok(())
}

fn scale_phase_start(conn: &Connection) -> Option<(std::time::Instant, u64)> {
    std::env::var_os("OS_SCALE_DIAG").map(|_| (std::time::Instant::now(), scale_wal_bytes(conn)))
}

fn scale_wal_bytes(conn: &Connection) -> u64 {
    conn.path()
        .and_then(|p| std::fs::metadata(format!("{p}-wal")).ok())
        .map_or(0, |m| m.len())
}

fn scale_phase_end(conn: &Connection, name: &str, start: Option<(std::time::Instant, u64)>) {
    if let Some((start, before)) = start {
        eprintln!(
            "OS_SCALE_DIAG {name} duration_ms={} wal_before={} wal_after={}",
            start.elapsed().as_millis(),
            before,
            scale_wal_bytes(conn)
        );
    }
}

impl CrawlSink for SqliteSink {
    fn durable(&mut self) -> Result<(), StoreError> {
        self.file
            .conn()
            .pragma_update(None, "synchronous", "FULL")?;
        Ok(())
    }

    fn write(&mut self, batch: WriteBatch) -> Result<(), StoreError> {
        // Nur Batches mit neuen URLs brauchen den Trigger nicht (ein leerer Batch bleibt ein No-op ohne Schreibzugriff).
        if !batch.new_urls.is_empty() {
            self.drop_insert_trigger()?;
        }
        if !is_empty(&batch) {
            self.drop_url_indexes_when_large(&batch)?;
        }
        if self.background_enabled {
            self.drive_background();
            self.throttle();
        } else {
            // Die Schwelle gilt für den nächsten Commit; sie wächst mit der Datei, bevor dieser Commit eine
            // Checkpoint-Entscheidung trifft.
            self.adapt_checkpoint_threshold();
        }
        // Das reguläre Ende baut die Indizes über `urls` nur in einer Datei, die sie behalten hat (K-83): in einem
        // großen Crawl fehlen sie danach (kein Leser braucht sie), und es gibt nichts vorzuwärmen. Fehlt in einer
        // kleinen Datei ausnahmsweise einer, wärmen die Leser den Dateicache für den Aufbau vor (K-80).
        let build_url_indexes = !self.url_indexes_dropped && completes_crawl(&batch);
        let prefetch = (build_url_indexes && !crate::url_index::present(self.file.conn())?)
            .then(|| crate::url_index::Prefetch::start(self.file.path()))
            .flatten();
        let end_diag = completes_crawl(&batch) && std::env::var_os("OS_SCALE_DIAG").is_some();
        let diag = end_diag.then(|| {
            let mut wal_path = self.file.path().as_os_str().to_os_string();
            wal_path.push("-wal");
            let before = std::fs::metadata(&wal_path).map_or(0, |m| m.len());
            (std::time::Instant::now(), wal_path, before)
        });
        let written = write_batch(
            self.file.conn(),
            &batch,
            self.clock.as_ref(),
            &mut self.internal,
            build_url_indexes,
        );
        drop(prefetch);
        if let Some((diag_start, wal_path, wal_before)) = diag {
            let wal_after = std::fs::metadata(&wal_path).map_or(0, |m| m.len());
            eprintln!(
                "OS_SCALE_DIAG end_batch duration_ms={} wal_before={} wal_after={}",
                diag_start.elapsed().as_millis(),
                wal_before,
                wal_after
            );
        }
        let written = written?;
        if let Some(written) = written {
            self.after_commit(&written);
        }
        Ok(())
    }

    /// Dauerhafter Zeitpunkt: WAL in die Hauptdatei zurückführen. Unter „Maximale Geschwindigkeit“
    /// (`synchronous = OFF`) wird dafür kurz auf `NORMAL` geschaltet, damit der Checkpoint wirklich auf den
    /// Datenträger gelangt.
    fn checkpoint(&mut self) -> Result<(), StoreError> {
        // Kein Durchlauf des Hintergrund-Threads neben dem `TRUNCATE`; der nächste Schreibaufruf startet ihn neu.
        self.stop_background();
        self.restore_insert_trigger()?;
        let conn = self.file.conn();
        let relaxed =
            self.profile.synchronous_during_crawl() != self.profile.synchronous_after_crawl();
        if relaxed {
            profile::settle(conn, self.profile)?;
        }
        let result = wal_checkpoint(conn);
        if relaxed {
            conn.pragma_update(None, "synchronous", self.profile.synchronous_during_crawl())?;
        }
        result
    }
}

/// Fehlerhafter Batch (unbekannte URL-ID, doppelte Adresse): `StoreError::InvalidInput`.
fn batch_error(message: String) -> StoreError {
    StoreError::InvalidInput(message)
}

fn is_empty(batch: &WriteBatch) -> bool {
    batch.new_urls.is_empty()
        && batch.serp_seeds.is_empty()
        && batch.url_updates.is_empty()
        && batch.pages.is_empty()
        && batch.queue.is_empty()
        && batch.sitemaps.is_empty()
        && batch.robots.is_empty()
        && batch.meta.is_empty()
        && batch.events.is_empty()
        && batch.counters.is_empty()
        && batch.url_flags.is_empty()
        && crate::writer_m2::parts_empty(batch)
}

fn int(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

/// Was `write_batch` für die Messzähler zurückgibt: der Eimer des Batches und die Seiten, die der Cache in das WAL
/// schrieb.
struct Written {
    bucket: usize,
    cache_write: u64,
}

/// Schreibt einen Batch in einer Transaktion. `None`: leerer Batch (No-op).
fn write_batch(
    conn: &Connection,
    batch: &WriteBatch,
    clock: &dyn Clock,
    internal: &mut InternalFlags,
    build_url_indexes: bool,
) -> Result<Option<Written>, StoreError> {
    use crate::sink::diag;
    if is_empty(batch) {
        return Ok(None);
    }
    let built = std::time::Instant::now();
    let mut g = diag::Groups::default();
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let seq = read_change_seq(&tx)? + 1;

    g.time(
        |g| &mut g.urls_ins,
        || insert_new_urls(&tx, &batch.new_urls, seq, internal),
    )?;
    insert_serp_seeds(&tx, &batch.serp_seeds, seq, internal)?;
    g.time(
        |g| &mut g.urls_upd,
        || apply_url_updates(&tx, &batch.url_updates, seq),
    )?;
    let mut batch_counters = BatchCounters::new(internal);
    for page in &batch.pages {
        write_page(&tx, page, seq, &mut batch_counters, &mut g)?;
    }
    g.time(|g| &mut g.counters, || batch_counters.flush(&tx, seq))?;
    write_url_flags(&tx, &batch.url_flags, seq)?;
    // Schema v3: die M2-Teile in derselben Transaktion (ext, strukturierte Daten, Rendering, …).
    crate::writer_m2::write(&tx, batch, seq)?;
    g.time(|g| &mut g.queue, || apply_queue(&tx, &batch.queue))?;
    for sitemap in &batch.sitemaps {
        write_sitemap(&tx, sitemap)?;
    }
    for robots in &batch.robots {
        write_robots(&tx, robots, clock)?;
    }
    write_meta(&tx, batch, seq)?;
    write_events_and_counters(&tx, batch)?;
    // Die Bauzeit endet vor dem Aufbau des Index `links_target` (Endbatch, `link_index_build_ns` gesondert).
    let bucket = diag::record_build(built, &g);
    let data = built.elapsed();
    // K-74: Der Index entsteht mit dem regulären Ende, im letzten Batch und derselben Transaktion – ein
    // Fehlschlag lässt den Crawl dann nicht als beendet erscheinen.
    let index_phase = completes_crawl(batch)
        .then(|| scale_phase_start(conn))
        .flatten();
    if build_url_indexes {
        // Sekundärindizes über `urls` (N22): nur eine Datei, die sie behalten hat, prüft sie (K-83: der Writer
        // baut entfernte Indizes nicht mehr auf, `url_index::ensure` bleibt für Leser, die sie brauchen).
        crate::url_index::ensure(&tx)?;
    }
    if completes_crawl(batch) {
        crate::url_index::ensure_deferred(&tx)?;
    }
    if completes_crawl(batch) && link_index::ensure(&tx)? {
        // Das Ereignis (ohne Dauer, deterministisch) macht den Aufbau in der Ereignisliste sichtbar; die Dauer steht in
        // den Messzählern (`link_index_build_ns`).
        tx.prepare_cached(
            "INSERT INTO crawl_events (at, kind, code, message) VALUES (?1, 'info', 'links_index_built', ?2)",
        )?
        .execute(params![
            clock.now().to_db(),
            "Index links_target für die Listen nach Linkziel aufgebaut"
        ])?;
    }

    scale_phase_end(conn, "indexes", index_phase);
    let committing = std::time::Instant::now();
    tx.commit()?;
    diag::record_commit(committing, bucket);
    if completes_crawl(batch) {
        diag::record_end_batch(data, committing.elapsed());
    }
    let cache = diag::cache_delta(conn);
    diag::record_cache(bucket, &cache);
    Ok(Some(Written {
        bucket,
        cache_write: cache.write,
    }))
}

/// Ob der Batch den Crawl regulär beendet (Zustand `Completed` als letzter `state`-Eintrag der Metadaten:
/// Ende, Stopp oder Limit; nicht Pause oder Fehler).
fn completes_crawl(batch: &WriteBatch) -> bool {
    batch
        .meta
        .iter()
        .rev()
        .find(|(key, _)| key == "state")
        .is_some_and(|(_, value)| value == CrawlState::Completed.as_str())
}

fn read_change_seq(tx: &rusqlite::Connection) -> Result<i64, StoreError> {
    let value: Option<String> = {
        let mut stmt =
            tx.prepare_cached("SELECT value FROM crawl_meta WHERE key = 'change_seq'")?;
        let mut rows = stmt.query([])?;
        match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        }
    };
    Ok(value.and_then(|v| v.parse().ok()).unwrap_or(0))
}

fn insert_new_urls(
    tx: &Transaction<'_>,
    urls: &[NewUrl],
    seq: i64,
    internal: &mut InternalFlags,
) -> Result<(), StoreError> {
    if urls.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        "INSERT INTO urls (id, url, address, origin, is_internal, in_tables, state, not_crawled_reason, \
         discovered_via, crawl_depth, folder_depth, url_length, change_seq) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )?;
    let mut counters_row =
        tx.prepare_cached("INSERT OR IGNORE INTO url_counters (url_id) VALUES (?1)")?;
    for u in urls {
        let inserted = stmt.execute(params![
            u.id.0,
            u.url,
            u.address,
            u.origin,
            u.is_internal,
            u.in_tables,
            u.state.as_str(),
            u.not_crawled_reason.map(|r| r.as_str()),
            u.discovered_via.as_str(),
            u.crawl_depth,
            u.folder_depth,
            u.url_length,
            seq
        ]);
        match inserted {
            Ok(_) => {
                counters_row.execute([u.id.0])?;
                internal.set(u.id.0, u.is_internal);
            }
            Err(rusqlite::Error::SqliteFailure(f, _))
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                return Err(batch_error(format!(
                    "NewUrl: URL-ID {} oder Adresse {:?} ist bereits vorhanden",
                    u.id, u.url
                )));
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn apply_url_updates(
    tx: &Transaction<'_>,
    updates: &[UrlUpdate],
    seq: i64,
) -> Result<(), StoreError> {
    if updates.is_empty() {
        return Ok(());
    }
    let mut depth =
        tx.prepare_cached("UPDATE urls SET crawl_depth = ?1, change_seq = ?2 WHERE id = ?3")?;
    let mut state = tx.prepare_cached(
        "UPDATE urls SET state = ?1, not_crawled_reason = ?2, change_seq = ?3 WHERE id = ?4",
    )?;
    let mut in_tables =
        tx.prepare_cached("UPDATE urls SET in_tables = ?1, change_seq = ?2 WHERE id = ?3")?;
    for update in updates {
        let (changed, id) = match update {
            UrlUpdate::Depth { id, depth: d } => (depth.execute(params![d, seq, id.0])?, id),
            UrlUpdate::State {
                id,
                state: s,
                reason,
            } => (
                state.execute(params![s.as_str(), reason.map(|r| r.as_str()), seq, id.0])?,
                id,
            ),
            UrlUpdate::InTables { id, value } => {
                (in_tables.execute(params![value, seq, id.0])?, id)
            }
        };
        if changed == 0 {
            return Err(batch_error(format!("UrlUpdate: URL {id} existiert nicht")));
        }
    }
    Ok(())
}

/// Spalten der Zeile `urls`, die ein `PageWrite` setzt (ohne `state`, `not_crawled_reason`, `change_seq`).
const PAGE_COLUMNS: &[&str] = &[
    "crawl_timestamp",
    "status_code",
    "status",
    "no_response_reason",
    "content_type",
    "content_class",
    "http_version",
    "size_bytes",
    "transferred_bytes",
    "total_transferred_bytes",
    "response_time_ms",
    "last_modified_raw",
    "last_modified",
    "redirect_url",
    "redirect_target_id",
    "redirect_type",
    "meta_refresh_delay",
    "hash_md5",
    "charset",
    "robots_blocked",
    "robots_matched_line",
    "robots_txt_url",
    "title_count",
    "title_1",
    "title_1_len",
    "title_1_px",
    "title_1_in_head",
    "title_2",
    "title_2_len",
    "title_2_px",
    "title_2_in_head",
    "meta_desc_count",
    "meta_desc_1",
    "meta_desc_1_len",
    "meta_desc_1_px",
    "meta_desc_1_in_head",
    "meta_desc_2",
    "meta_desc_2_len",
    "meta_desc_2_px",
    "meta_desc_2_in_head",
    "meta_kw_count",
    "meta_kw_1",
    "meta_kw_1_len",
    "meta_kw_2",
    "meta_kw_2_len",
    "h1_count",
    "h1_1",
    "h1_1_len",
    "h1_2",
    "h1_2_len",
    "h2_count",
    "h2_1",
    "h2_1_len",
    "h2_2",
    "h2_2_len",
    "word_count",
    "text_ratio",
    "canonical_count",
    "canonical_url",
    "canonical_target_id",
    "canonical_self",
    "effective_index",
    "effective_follow",
    "has_rel_next",
    "has_rel_prev",
    "hreflang_count",
    "indexability",
    "indexability_status",
];

fn page_update_sql() -> &'static str {
    static SQL: OnceLock<String> = OnceLock::new();
    SQL.get_or_init(|| {
        let mut sql = String::from("UPDATE urls SET state = 'done', not_crawled_reason = NULL, ");
        for (i, column) in PAGE_COLUMNS.iter().enumerate() {
            sql.push_str(&format!("{column} = ?{}, ", i + 1));
        }
        let n = PAGE_COLUMNS.len();
        sql.push_str(&format!("change_seq = ?{} WHERE id = ?{}", n + 1, n + 2));
        sql
    })
}

fn push_text(values: &mut Vec<Value>, e: &Option<os_model::ExtractedText>, with_px: bool) {
    match e {
        Some(t) => {
            values.push(Value::from(t.text.clone()));
            values.push(Value::from(i64::from(t.len)));
            if with_px {
                values.push(Value::from(t.px.map(i64::from)));
                values.push(Value::from(i64::from(t.in_head)));
            }
        }
        None => {
            values.push(Value::Null);
            values.push(Value::Null);
            if with_px {
                values.push(Value::Null);
                values.push(Value::Null);
            }
        }
    }
}

/// `count`, danach erstes und zweites Vorkommen. `with_px`: Titel und Description haben Pixelbreite und
/// `in_head`; Keywords und Überschriften nur Text und Länge.
fn push_pair(values: &mut Vec<Value>, pair: Option<&ElementPair>, with_px: bool) {
    match pair {
        Some(p) => {
            values.push(Value::from(i64::from(p.count)));
            push_text(values, &p.first, with_px);
            push_text(values, &p.second, with_px);
        }
        None => {
            values.push(Value::Null);
            let width = if with_px { 4 } else { 2 };
            for _ in 0..2 * width {
                values.push(Value::Null);
            }
        }
    }
}

fn page_values(page: &PageWrite, seq: i64) -> Vec<Value> {
    let f = &page.fetch;
    let mut v: Vec<Value> = Vec::with_capacity(PAGE_COLUMNS.len() + 2);
    v.push(Value::from(f.crawl_timestamp.to_db()));
    v.push(Value::from(i64::from(f.status_code)));
    v.push(Value::from(f.status.clone()));
    v.push(Value::from(
        f.no_response_reason.map(|r| r.as_str().to_string()),
    ));
    v.push(Value::from(f.content_type.clone()));
    v.push(Value::from(f.content_class.as_str().to_string()));
    v.push(Value::from(f.http_version.clone()));
    v.push(Value::from(int(f.size_bytes)));
    v.push(Value::from(int(f.transferred_bytes)));
    v.push(Value::from(int(f.total_transferred_bytes)));
    v.push(Value::from(f.response_time_ms.map(i64::from)));
    v.push(Value::from(f.last_modified_raw.clone()));
    v.push(Value::from(f.last_modified.map(|t| t.to_db())));
    v.push(Value::from(f.redirect_url.clone()));
    v.push(Value::from(f.redirect_target.map(|t| t.0)));
    v.push(Value::from(f.redirect_type.map(|t| t.as_str().to_string())));
    v.push(Value::from(f.meta_refresh_delay.map(i64::from)));
    v.push(Value::from(f.hash_md5.clone()));
    v.push(Value::from(f.charset.clone()));
    v.push(Value::from(f.robots_blocked));
    v.push(Value::from(f.robots_matched_line.clone()));
    v.push(Value::from(f.robots_txt_url.clone()));

    let p: Option<&PageFacts> = page.page.as_ref();
    push_pair(&mut v, p.map(|p| &p.title), true);
    push_pair(&mut v, p.map(|p| &p.meta_description), true);
    push_pair(&mut v, p.map(|p| &p.meta_keywords), false);
    push_pair(&mut v, p.map(|p| &p.h1), false);
    push_pair(&mut v, p.map(|p| &p.h2), false);
    v.push(Value::from(p.and_then(|p| p.word_count).map(i64::from)));
    v.push(Value::from(p.and_then(|p| p.text_ratio).map(f64::from)));
    v.push(Value::from(p.map_or(0, |p| i64::from(p.canonical_count))));
    v.push(Value::from(p.and_then(|p| p.canonical_url.clone())));
    v.push(Value::from(p.and_then(|p| p.canonical_target).map(|t| t.0)));
    v.push(Value::from(p.and_then(|p| p.canonical_self)));
    v.push(Value::from(p.map(|p| p.effective_index)));
    v.push(Value::from(p.map(|p| p.effective_follow)));
    v.push(Value::from(p.map(|p| p.has_rel_next)));
    v.push(Value::from(p.map(|p| p.has_rel_prev)));
    v.push(Value::from(p.map_or(0, |p| i64::from(p.hreflang_count))));
    v.push(Value::from(page.indexability.as_str().to_string()));
    v.push(Value::from(
        page.indexability_status.map(|s| s.as_str().to_string()),
    ));
    debug_assert_eq!(v.len(), PAGE_COLUMNS.len());
    v.push(Value::from(seq));
    v.push(Value::from(page.url_id.0));
    v
}

/// Zustand der Zeile einer Seite vor dem Überschreiben: ob sie intern ist und ob sie bisher eine HTML-Seite mit
/// Status 2xx war (Beitrag zu `html_2xx_sources` ihrer Linkziele). Ein Lesezugriff auf dieselbe Zeile, die das
/// `UPDATE` danach ändert.
fn source_state(tx: &Transaction<'_>, id: i64) -> Result<(bool, bool), StoreError> {
    let mut stmt = tx
        .prepare_cached("SELECT is_internal, content_class, status_code FROM urls WHERE id = ?1")?;
    let mut rows = stmt.query([id])?;
    Ok(match rows.next()? {
        Some(row) => {
            let internal = row.get::<_, i64>(0)? != 0;
            let class: Option<String> = row.get(1)?;
            let status: Option<i64> = row.get(2)?;
            (
                internal,
                class.as_deref() == Some(ContentClass::Html.as_str())
                    && status.is_some_and(|s| (200..=299).contains(&s)),
            )
        }
        None => (false, false),
    })
}

/// Ob eine Seite beim Schreiben eine HTML-Seite mit Status 2xx ist.
fn is_html_2xx(page: &PageWrite) -> bool {
    page.fetch.content_class == ContentClass::Html && (200..=299).contains(&page.fetch.status_code)
}

fn delete_children(tx: &Transaction<'_>, id: i64) -> Result<(), StoreError> {
    for sql in [
        "DELETE FROM links WHERE source_id = ?1",
        "DELETE FROM image_instances WHERE page_id = ?1",
        "DELETE FROM canonicals WHERE url_id = ?1",
        "DELETE FROM directives WHERE url_id = ?1",
        "DELETE FROM refresh WHERE url_id = ?1",
        // hreflang aus Sitemaps gehören der Sitemap, nicht der Seite (M1.md 3.11)
        "DELETE FROM hreflang WHERE url_id = ?1 AND source <> 'sitemap'",
        // Weiterleitungs-Flags werden beim erneuten Abruf neu bestimmt
        "DELETE FROM url_flags WHERE url_id = ?1 AND flag IN ('redirect_loop', 'redirect_limit_reached')",
        "DELETE FROM pagination WHERE url_id = ?1",
        "DELETE FROM http_headers WHERE url_id = ?1",
    ] {
        tx.prepare_cached(sql)?.execute([id])?;
    }
    Ok(())
}

fn write_page(
    tx: &Transaction<'_>,
    page: &PageWrite,
    seq: i64,
    batch_counters: &mut BatchCounters<'_>,
    g: &mut crate::sink::diag::Groups,
) -> Result<(), StoreError> {
    let id = page.url_id.0;
    // Der bisherige Zustand der Zeile steht vor dem Überschreiben fest (nur ein Lesezugriff auf dieselbe Zeile).
    let (internal, old_html_2xx) = source_state(tx, id)?;
    let old_status: (Option<u16>, Option<String>) = tx
        .query_row(
            "SELECT status_code, indexability FROM urls WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| StoreError::InvalidInput(format!("unbekannte URL {id}")))?;
    let changed = g.time(
        |g| &mut g.urls_upd,
        || -> Result<usize, StoreError> {
            Ok(tx
                .prepare_cached(page_update_sql())?
                .execute(params_from_iter(page_values(page, seq).iter()))?)
        },
    )?;
    if changed == 0 {
        return Err(batch_error(format!("PageWrite: URL {id} existiert nicht")));
    }

    if old_status
        != (
            Some(page.fetch.status_code),
            Some(page.indexability.as_str().to_owned()),
        )
        && tx.prepare_cached("SELECT EXISTS(SELECT 1 FROM canonicals WHERE target_id = ?1) OR EXISTS(SELECT 1 FROM pagination WHERE target_id = ?1) OR EXISTS(SELECT 1 FROM hreflang WHERE target_id = ?1)")?
            .query_row([id], |r| r.get::<_, bool>(0))?
    {
        stamp_dependants(tx, id, seq)?;
    }
    let old = if page.replace {
        let old = if internal {
            Contribution::load(tx, id, old_html_2xx)?
        } else {
            Contribution::default()
        };
        delete_children(tx, id)?;
        crate::writer_m2::clear_page_children(tx, id)?;
        old
    } else {
        Contribution::default()
    };

    g.time(|g| &mut g.links, || insert_links(tx, id, &page.links))?;
    insert_images(tx, id, &page.images)?;
    insert_canonicals(tx, id, &page.canonicals)?;
    insert_directives(tx, id, &page.directives)?;
    insert_refresh(tx, id, &page.refresh)?;
    insert_hreflang(tx, id, &page.hreflang)?;
    insert_pagination(tx, id, &page.pagination)?;
    insert_headers(tx, id, &page.headers)?;
    if let Some(html) = &page.html {
        crate::writer_m2::write_page_html(tx, id, html)?;
    }

    if internal {
        let new = Contribution::from_rows(&page.links, &page.images, is_html_2xx(page));
        g.time(
            |g| &mut g.counters,
            || batch_counters.add_page(tx, id, &old, &new),
        )?;
    }
    Ok(())
}

fn insert_links(tx: &Transaction<'_>, id: i64, links: &[LinkRow]) -> Result<(), StoreError> {
    if links.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        "INSERT INTO links (source_id, target_id, target_raw, link_type, position, anchor, alt_text, follow, \
         rel, target_attr, path_type, link_path, link_position, link_origin, in_head, crawlable, uncrawlable_reason, pdf_page) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
    )?;
    for l in links {
        stmt.execute(params![
            id,
            l.target_id.map(|t| t.0),
            l.target_raw,
            l.link_type.as_str(),
            l.position,
            l.anchor,
            l.alt_text,
            l.follow,
            l.rel,
            l.target_attr,
            l.path_type.map(|p| p.as_str()),
            l.link_path,
            l.link_position.as_ref().map(|p| p.as_str()),
            l.link_origin.as_str(),
            l.in_head,
            l.crawlable,
            l.uncrawlable_reason.map(|r| r.as_str()),
            l.pdf_page
        ])?;
    }
    Ok(())
}

fn insert_images(tx: &Transaction<'_>, id: i64, images: &[ImageRow]) -> Result<(), StoreError> {
    if images.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        "INSERT INTO image_instances (page_id, image_id, src_raw, source, alt_present, alt, alt_len, \
         width_attr, height_attr, link_path, position, resolved, link_position, origin, real_w, real_h, display_w, display_h, in_viewport, bytes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
    )?;
    for i in images {
        let m = &i.instance;
        stmt.execute(params![
            id,
            i.image_id.map(|t| t.0),
            m.src_raw,
            m.source.as_str(),
            m.alt_present,
            m.alt,
            m.alt_len,
            m.width_attr,
            m.height_attr,
            m.xpath,
            m.doc_order,
            m.resolved,
            m.link_position.as_ref().map(|p| p.as_str()),
            m.origin.as_str(),
            m.render.as_ref().and_then(|r| r.real_w),
            m.render.as_ref().and_then(|r| r.real_h),
            m.render.as_ref().and_then(|r| r.display_w),
            m.render.as_ref().and_then(|r| r.display_h),
            m.render.as_ref().and_then(|r| r.in_viewport),
            m.render.as_ref().and_then(|r| r.bytes)
        ])?;
    }
    Ok(())
}

fn insert_canonicals(
    tx: &Transaction<'_>,
    id: i64,
    rows: &[CanonicalRow],
) -> Result<(), StoreError> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        "INSERT OR REPLACE INTO canonicals (url_id, source, position, raw, resolved, target_id, in_head) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for c in rows {
        stmt.execute(params![
            id,
            c.r.source.as_str(),
            c.r.order,
            c.r.raw,
            c.r.resolved,
            c.target_id.map(|t| t.0),
            c.r.in_head
        ])?;
    }
    Ok(())
}

fn insert_directives(
    tx: &Transaction<'_>,
    id: i64,
    rows: &[DirectiveRow],
) -> Result<(), StoreError> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        "INSERT OR REPLACE INTO directives (url_id, source, position, agent, raw, tokens, applies, in_head) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for DirectiveRow(d) in rows {
        stmt.execute(params![
            id,
            d.source.as_str(),
            d.order,
            d.agent,
            d.raw,
            d.tokens.join(","),
            d.applies,
            d.in_head
        ])?;
    }
    Ok(())
}

fn insert_refresh(tx: &Transaction<'_>, id: i64, rows: &[RefreshRow]) -> Result<(), StoreError> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        "INSERT OR REPLACE INTO refresh (url_id, source, position, raw, delay, target_raw, target_url, \
         target_id, in_head) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for r in rows {
        stmt.execute(params![
            id,
            r.r.source.as_str(),
            r.r.order,
            r.r.raw,
            r.r.delay,
            r.r.target_raw,
            r.r.target,
            r.target_id.map(|t| t.0),
            r.r.in_head
        ])?;
    }
    Ok(())
}

fn insert_hreflang(tx: &Transaction<'_>, id: i64, rows: &[HreflangRow]) -> Result<(), StoreError> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        "INSERT OR REPLACE INTO hreflang (url_id, source, position, sitemap_id, code_raw, code_norm, \
         code_valid, invalid_reason, href_raw, url, target_id, in_head) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;
    for h in rows {
        stmt.execute(params![
            id,
            h.r.source.as_str(),
            h.r.order,
            h.sitemap_id.map(|s| s.0),
            h.r.code_raw,
            h.r.code_norm,
            h.r.code_valid,
            h.r.invalid_reason,
            h.r.href_raw,
            h.r.resolved,
            h.target_id.map(|t| t.0),
            h.r.in_head
        ])?;
    }
    Ok(())
}

fn insert_pagination(
    tx: &Transaction<'_>,
    id: i64,
    rows: &[PaginationRow],
) -> Result<(), StoreError> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        "INSERT OR REPLACE INTO pagination (url_id, rel, source, position, raw, url, target_id, in_head) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for p in rows {
        stmt.execute(params![
            id,
            if p.r.rel_next { "next" } else { "prev" },
            p.r.source.as_str(),
            p.r.order,
            p.r.raw,
            p.r.resolved,
            p.target_id.map(|t| t.0),
            p.r.in_head
        ])?;
    }
    Ok(())
}

fn insert_headers(tx: &Transaction<'_>, id: i64, rows: &[HeaderRow]) -> Result<(), StoreError> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        "INSERT OR REPLACE INTO http_headers (url_id, direction, position, name, value) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for h in rows {
        stmt.execute(params![
            id,
            if h.request { "request" } else { "response" },
            h.position,
            h.name,
            h.value
        ])?;
    }
    Ok(())
}

fn apply_queue(tx: &Transaction<'_>, ops: &[QueueOp]) -> Result<(), StoreError> {
    if ops.is_empty() {
        return Ok(());
    }
    let mut push = tx.prepare_cached(
        "INSERT OR REPLACE INTO queue (url_id, seq, depth, priority) VALUES (?1, ?2, ?3, ?4)",
    )?;
    let mut pop = tx.prepare_cached("DELETE FROM queue WHERE url_id = ?1")?;
    let mut max_seq: Option<i64> = None;
    for op in ops {
        match op {
            QueueOp::Push {
                id,
                seq,
                depth,
                priority,
            } => {
                push.execute(params![id.0, int(*seq), depth, priority])?;
                max_seq = Some(max_seq.map_or(int(*seq), |m| m.max(int(*seq))));
            }
            QueueOp::Pop { id } => {
                pop.execute([id.0])?;
            }
        }
    }
    if let Some(max) = max_seq {
        raise_meta_counter(tx, "next_seq", max.saturating_add(1))?;
    }
    Ok(())
}

/// Hebt einen numerischen `crawl_meta`-Wert auf mindestens `value` an.
fn raise_meta_counter(tx: &rusqlite::Connection, key: &str, value: i64) -> Result<(), StoreError> {
    tx.prepare_cached(
        "INSERT INTO crawl_meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value WHERE CAST(value AS INTEGER) < ?2",
    )?
    .execute(params![key, value])?;
    Ok(())
}

/// Schreibt `url_flags` (`INSERT OR REPLACE`) und stempelt `change_seq` der betroffenen URLs (K-20).
fn write_url_flags(
    tx: &Transaction<'_>,
    flags: &[(UrlId, String, Option<String>)],
    seq: i64,
) -> Result<(), StoreError> {
    if flags.is_empty() {
        return Ok(());
    }
    let mut insert = tx.prepare_cached(
        "INSERT OR REPLACE INTO url_flags (url_id, flag, value) VALUES (?1, ?2, ?3)",
    )?;
    let mut stamp = tx.prepare_cached("UPDATE urls SET change_seq = ?1 WHERE id = ?2")?;
    for (id, flag, value) in flags {
        if stamp.execute(params![seq, id.0])? == 0 {
            return Err(batch_error(format!("url_flags: URL {id} existiert nicht")));
        }
        insert.execute(params![id.0, flag, value])?;
    }
    Ok(())
}

fn write_sitemap(tx: &Transaction<'_>, s: &SitemapWrite) -> Result<(), StoreError> {
    tx.prepare_cached(
        "INSERT OR REPLACE INTO sitemaps (id, url, url_id, parent_id, kind, entries, invalid_entries, \
         size_bytes, status_code, depth, found_via, error) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?
    .execute(params![
        s.id.0,
        s.url,
        s.url_id.map(|u| u.0),
        s.parent.map(|p| p.0),
        s.kind.as_str(),
        s.entries,
        s.invalid_entries,
        int(s.size_bytes),
        s.status_code,
        s.depth,
        s.found_via,
        s.error
    ])?;
    tx.prepare_cached("DELETE FROM sitemap_entries WHERE sitemap_id = ?1")?
        .execute([s.id.0])?;
    if !s.entry_urls.is_empty() {
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO sitemap_entries (sitemap_id, url_id, loc_raw, lastmod) \
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (url_id, loc_raw, lastmod) in &s.entry_urls {
            stmt.execute(params![s.id.0, url_id.0, loc_raw, lastmod])?;
        }
    }
    // Zusatzangaben der Einträge (CRAWL-060-Rest, Schema v3).
    tx.execute(
        "DELETE FROM sitemap_entry_ext WHERE sitemap_id = ?1",
        [s.id.0],
    )?;
    for m in &s.entry_meta {
        for (position, ext) in m.extensions.iter().enumerate() {
            tx.execute("INSERT INTO sitemap_entry_ext (sitemap_id, url_id, position, kind, json) VALUES (?1, ?2, ?3, ?4, ?5)", params![s.id.0, m.url.0, position as i64, ext.kind.as_str(), ext.json])?;
        }
    }
    if !s.entry_meta.is_empty() {
        let mut stmt = tx.prepare_cached(
            "UPDATE sitemap_entries SET changefreq = ?1, priority = ?2, images = ?3, videos = ?4, news = ?5 \
             WHERE sitemap_id = ?6 AND url_id = ?7",
        )?;
        for m in &s.entry_meta {
            let changed = stmt.execute(params![
                m.changefreq,
                m.priority,
                m.images,
                m.videos,
                m.news,
                s.id.0,
                m.url.0
            ])?;
            if changed == 0 {
                return Err(batch_error(format!(
                    "SitemapWrite {}: entry_meta für URL {} ohne Eintrag in entry_urls",
                    s.id.0, m.url
                )));
            }
        }
    }
    // Eine erneute Übergabe derselben Sitemap ersetzt auch deren hreflang-Angaben.
    tx.prepare_cached("DELETE FROM hreflang WHERE source = 'sitemap' AND sitemap_id = ?1")?
        .execute([s.id.0])?;
    if !s.hreflang.is_empty() {
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO hreflang (url_id, source, position, sitemap_id, code_raw, code_norm, \
             code_valid, invalid_reason, href_raw, url, target_id, in_head) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;
        for (url_id, h) in &s.hreflang {
            stmt.execute(params![
                url_id.0,
                HintSource::Sitemap.as_str(),
                h.r.order,
                s.id.0,
                h.r.code_raw,
                h.r.code_norm,
                h.r.code_valid,
                h.r.invalid_reason,
                h.r.href_raw,
                h.r.resolved,
                h.target_id.map(|t| t.0),
                h.r.in_head
            ])?;
        }
    }
    Ok(())
}

fn write_robots(
    tx: &Transaction<'_>,
    r: &RobotsWrite,
    clock: &dyn Clock,
) -> Result<(), StoreError> {
    tx.prepare_cached(
        "INSERT OR REPLACE INTO robots_files (origin, robots_url, fetch_status, status_code, attempts, body, \
         note, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?
    .execute(params![
        r.origin,
        r.robots_url,
        r.fetch_status,
        r.status_code,
        r.attempts,
        r.body,
        r.note,
        clock.now().to_db()
    ])?;
    Ok(())
}

fn write_meta(tx: &rusqlite::Connection, batch: &WriteBatch, seq: i64) -> Result<(), StoreError> {
    if !batch.meta.is_empty() {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO crawl_meta (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )?;
        for (key, value) in &batch.meta {
            stmt.execute([key, value])?;
        }
    }
    if let Some(max_id) = batch
        .new_urls
        .iter()
        .map(|u| u.id.0)
        .chain(batch.serp_seeds.iter().map(|u| u.id.0))
        .max()
    {
        raise_meta_counter(tx, "next_url_id", max_id.saturating_add(1))?;
    }
    // Der Änderungsfeed gehört dem Writer – zuletzt, damit `meta` ihn nicht überschreiben kann.
    tx.prepare_cached(
        "INSERT INTO crawl_meta (key, value) VALUES ('change_seq', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )?
    .execute([seq.to_string()])?;
    Ok(())
}

fn write_events_and_counters(tx: &Transaction<'_>, batch: &WriteBatch) -> Result<(), StoreError> {
    if !batch.events.is_empty() {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO crawl_events (at, kind, code, message) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for e in &batch.events {
            stmt.execute(params![e.at.to_db(), e.kind, e.code, e.message])?;
        }
    }
    if !batch.counters.is_empty() {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO crawl_counters (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = value + excluded.value",
        )?;
        for (key, delta) in &batch.counters {
            stmt.execute(params![key, delta])?;
        }
    }
    Ok(())
}

fn stamp_dependants(tx: &rusqlite::Connection, id: i64, seq: i64) -> Result<(), StoreError> {
    tx.prepare_cached("UPDATE urls SET change_seq = ?1 WHERE id IN (SELECT url_id FROM pagination WHERE target_id = ?2 UNION SELECT url_id FROM canonicals WHERE target_id = ?2 UNION SELECT url_id FROM hreflang WHERE target_id = ?2)")?.execute(params![seq, id])?;
    Ok(())
}
/// Atomare Neuberechnung gespeicherter Fakten ohne Recrawl (K2-43).
#[derive(Debug, Default)]
pub struct FactUpdates {
    pub ext: Vec<os_model::ExtWrite>,
    pub link_positions: Vec<(i64, Option<String>)>,
    pub indexability: Vec<(UrlId, os_model::Indexability, Option<String>)>,
    pub hash: Vec<(UrlId, Option<String>)>,
    pub word_count: Vec<(UrlId, Option<u32>)>,
}
pub fn apply_fact_updates(
    file: &mut crate::CrawlFile,
    updates: &FactUpdates,
) -> Result<Vec<UrlId>, StoreError> {
    let tx = crate::profile::RecomputeSavepoint::new(file.conn())?;
    let seq = read_change_seq(&tx)? + 1;
    let mut changed = std::collections::BTreeSet::new();
    for (id, position) in &updates.link_positions {
        let source: i64 = tx.query_row("SELECT source_id FROM links WHERE id = ?1", [id], |r| {
            r.get(0)
        })?;
        tx.execute(
            "UPDATE links SET link_position = ?1 WHERE id = ?2",
            params![position, id],
        )?;
        changed.insert(UrlId(source));
    }
    for (id, value, status) in &updates.indexability {
        tx.execute(
            "UPDATE urls SET indexability = ?1, indexability_status = ?2 WHERE id = ?3",
            params![value.as_str(), status, id.0],
        )?;
        stamp_dependants(&tx, id.0, seq)?;
        changed.insert(*id);
    }
    for (id, hash) in &updates.hash {
        tx.execute(
            "UPDATE urls SET hash_md5 = ?1 WHERE id = ?2",
            params![hash, id.0],
        )?;
        changed.insert(*id);
    }
    for (id, count) in &updates.word_count {
        tx.execute(
            "UPDATE urls SET word_count = ?1 WHERE id = ?2",
            params![count, id.0],
        )?;
        changed.insert(*id);
    }
    let batch = WriteBatch {
        ext: updates.ext.clone(),
        ..Default::default()
    };
    crate::writer_m2::write(&tx, &batch, seq)?;
    changed.extend(updates.ext.iter().map(|e| e.url));
    for id in &changed {
        if tx.execute(
            "UPDATE urls SET change_seq = ?1 WHERE id = ?2",
            params![seq, id.0],
        )? == 0
        {
            return Err(StoreError::InvalidInput(format!("Unbekannte URL {id}")));
        }
    }
    if !changed.is_empty() {
        write_meta(&tx, &batch, seq)?;
    }
    tx.commit()?;
    Ok(changed.into_iter().collect())
}

/// Dedicated additive path: no PageWrite and no invented response measurements.
fn insert_serp_seeds(
    tx: &Transaction<'_>,
    seeds: &[os_model::SerpSeed],
    seq: i64,
    internal: &mut InternalFlags,
) -> Result<(), StoreError> {
    if seeds.is_empty() {
        return Ok(());
    }
    let mut insert = tx.prepare_cached(
        "INSERT INTO urls (id,url,address,origin,is_internal,in_tables,state,discovered_via,folder_depth,url_length,content_class,
         title_count,title_1,title_1_len,title_1_px,meta_desc_count,meta_desc_1,meta_desc_1_len,meta_desc_1_px,
         effective_index,effective_follow,indexability,change_seq,status_code)
         VALUES (?1,?2,?3,'',1,1,'done','list_input',0,?4,'HTML',1,?5,?6,?7,1,?8,?9,?10,1,1,'Indexable',?11,200)"
    )?;
    let mut counters = tx.prepare_cached("INSERT INTO url_counters (url_id,change_seq) VALUES (?1,?2) ON CONFLICT(url_id) DO UPDATE SET change_seq=excluded.change_seq")?;
    for row in seeds {
        insert.execute(params![
            row.id.0,
            format!("urn:openspider:serp:{}", row.id.0),
            row.address,
            row.url_length,
            row.title,
            row.title_length,
            row.title_px,
            row.description,
            row.description_length,
            row.description_px,
            seq
        ])?;
        counters.execute(params![row.id.0, seq])?;
        internal.set(row.id.0, true);
    }
    Ok(())
}
