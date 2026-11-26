//! Der Index `links_target (target_id, link_type)` und sein Lebenszyklus (K-74).
//!
//! Gleichverteilte Linkziele machen die Pflege dieses Index zur Hauptursache des Durchsatzeinbruchs großer
//! Crawls: Jede Seite berührt 30 verschiedene Stellen eines Index, der bei 1 Mio. Seiten nicht mehr in den Cache
//! passt. Deshalb gilt:
//!
//! * Der Index **fehlt** während eines laufenden oder pausierten Crawls und nach einem Abbruch. Der Writer
//!   entfernt ihn, wenn er eine Crawl-Datei zum Schreiben öffnet ([`crate::SqliteSink::open`]).
//! * Beim **regulären Ende** (Zustand `Completed` im letzten Batch: Ende, Stopp und Limit; nicht bei Pause oder
//!   Fehler) baut der Writer ihn in der Transaktion dieses Batches sortiert auf (`CREATE INDEX` sortiert die
//!   Schlüssel und füllt den Baum in einem Zug). Das Crawl-Ende und der Analyse-Hook verschieben sich um diese Zeit
//!   (Vorprüfung: 37 s bei 30 Mio. Links).
//! * [`ensure`] baut ihn bei Bedarf nach; die Crawl-Analyse und die Exporte einer nicht laufenden Datei rufen
//!   [`ensure_if_idle`] auf.
//!
//! Ohne den Index bleibt jede Abfrage nach `links.target_id` korrekt, kostet aber einen sequenziellen Lauf über
//! `links` (nie einen je Ergebniszeile); die Zähler je URL stehen immer in `url_counters`
//! (Tabelle in `docs/ARCHITECTURE.md`, Abschnitt Crawl-Datei).

use os_model::time::SystemClock;
use os_model::CrawlState;
use rusqlite::Connection;

use crate::error::StoreError;
use crate::open::CrawlFile;

/// Name des Index.
pub const INDEX_NAME: &str = "links_target";

/// Definition wie in Migration 0001.
const CREATE_SQL: &str = "CREATE INDEX IF NOT EXISTS links_target ON links(target_id, link_type)";

/// Cache-Größe (KiB) während des Indexaufbaus. Der Sortierer von `CREATE INDEX` hält bis zur Größe des Seitencaches
/// Schlüssel im Speicher, bevor er auf die Platte ausweicht; bei 256 MiB Cache (Profil Balanced) kostete das
/// zusätzlich rund 270 MiB Spitzen-RSS (Messung M1-P32, Schritt 1, PERF-003 verlangt unter 500 MB). Mit 4 MiB
/// bleibt der Aufbau schnell (1 Mio. Seiten x 30 Links: 31 s gegen 34 s bei 32 MiB; der Baum wird sortiert gefüllt) und der
/// Spitzen-RSS beim Stand des Crawls (364 gegen 391 MiB). Größere Sortierpuffer sind nicht schneller (N27, sqlite3-Shell, 15 Mio.
/// Links, 4 Threads: 4 MiB 15,9 s, 64 MiB 18,6 s; die In-Memory-Listen des Sortierers sind klein cachefreundlicher).
const BUILD_CACHE_KIB: i64 = 4 * 1024;

/// Ob der Index in der Datei steht.
pub fn present(conn: &Connection) -> Result<bool, StoreError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [INDEX_NAME],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Entfernt den Index (`DROP INDEX IF EXISTS`); ohne Index passiert nichts.
pub fn drop_index(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch("DROP INDEX IF EXISTS links_target")?;
    Ok(())
}

/// Baut den Index auf, wenn er fehlt. `true`, wenn er gebaut wurde. Braucht Schreibzugriff; dauert bei vielen
/// Links (30 Mio.) rund eine halbe Minute. Die Dauer geht in die Messzähler ([`crate::sink::diag`]).
pub fn ensure(conn: &Connection) -> Result<bool, StoreError> {
    if present(conn)? {
        return Ok(false);
    }
    let t = std::time::Instant::now();
    with_build_settings(conn, || conn.execute_batch(CREATE_SQL))?;
    crate::sink::diag::record_link_index(t);
    Ok(true)
}

/// Hilfsthreads des Sortierers beim Indexaufbau (`PRAGMA threads`, N22): Sortieren und Mischen der Läufe überlappen mit
/// dem Lesen der Tabelle. Zwei Threads halten den Zusatzspeicher bei rund 8 MB (je Thread ein Sortierpuffer des kleinen
/// Caches). **Vier Threads** (N27/K-84 geprüft, verworfen): im Mikrobenchmark (`crates/os-store/tests/bench_links_build.rs`,
/// 1 Mio. Seiten x 30 Links mit breiten Zeilen) 18 bis 21 s statt 20 bis 22 s, im Crawl `links_target` 18,5 statt 20 s,
/// aber der Spitzen-RSS von `perf-003 --size large` stieg in drei von vier Läufen auf 500, 577 und 590 MB (mit zwei Threads
/// 462, 466, 478 MB; Grenze 500 MB, Reserve 10 MB). Mehr Threads helfen ohnehin nicht, weil die Hauptlinie
/// (Schlussmischen, Füllen des Baums, rund 15 s Benutzerzeit) den Aufbau bestimmt.
const BUILD_THREADS: i64 = 2;

/// Führt einen Indexaufbau mit dem kleinen Cache ([`BUILD_CACHE_KIB`]) und den Hilfsthreads des Sortierers aus und stellt
/// beide Einstellungen danach wieder her (auch bei einem Fehler).
pub(crate) fn with_build_settings<T>(
    conn: &Connection,
    build: impl FnOnce() -> Result<T, rusqlite::Error>,
) -> Result<T, StoreError> {
    let cache: i64 = conn.pragma_query_value(None, "cache_size", |r| r.get(0))?;
    let threads: i64 = conn.pragma_query_value(None, "threads", |r| r.get(0))?;
    let shrink = cache >= 0 || -cache > BUILD_CACHE_KIB;
    if shrink {
        conn.pragma_update(None, "cache_size", -BUILD_CACHE_KIB)?;
    }
    let helpers =
        threads < BUILD_THREADS && conn.pragma_update(None, "threads", BUILD_THREADS).is_ok();
    let built = build();
    if helpers {
        let _ = conn.pragma_update(None, "threads", threads);
    }
    if shrink {
        conn.pragma_update(None, "cache_size", cache)?;
    }
    Ok(built?)
}

/// Wie [`ensure`], aber nur für eine Datei mit Schreibzugriff, die nicht läuft (gespeicherter Zustand nicht
/// `Running`, oder ein `Running` mit ausgebliebenem Heartbeat). Ein laufender Crawl baut den Index selbst am
/// Ende; eine gerade gesperrte Datei (`SQLITE_BUSY`) bleibt unverändert und liefert `false`.
///
/// Die Indizes über `urls` ([`crate::url_index`]) zieht die Funktion nur für eine **kleine** Datei nach (K-83): In
/// einer Datei ab der Schwelle ([`crate::url_index::is_large`]) braucht sie kein Leser, und ihr Aufbau kostet bei
/// 1 Mio. URLs rund 50 s.
pub fn ensure_if_idle(file: &CrawlFile) -> Result<bool, StoreError> {
    let conn = file.conn();
    if conn.is_readonly("main")? {
        return Ok(false);
    }
    let want_url_indexes = !crate::url_index::is_large(conn)?;
    let deferred_present = crate::url_index::DEFERRED_INDEXES.iter().all(|index| {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1)",
            [index.name],
            |r| r.get::<_, bool>(0),
        )
        .unwrap_or(false)
    });
    if present(conn)? && deferred_present && (!want_url_indexes || crate::url_index::present(conn)?)
    {
        return Ok(false);
    }
    // 08 AI-020 [M4] (Wirkung b): Führt ein anderer Prozess den Crawl als aktiver Besitzer (z. B. KI-Lauf), schreibt
    // dieser Prozess nichts in die Datei – wie bei einem laufenden Crawl.
    if file.effective_state(&SystemClock)? == CrawlState::Running
        || file
            .occupancy(&SystemClock)?
            .foreign_active_owner(std::process::id())
    {
        return Ok(false);
    }
    // Zuerst die Indizes über `urls` einer kleinen Datei (N22), dann `links_target`; beides sortiert.
    let built = (|| -> Result<bool, StoreError> {
        let urls = if want_url_indexes {
            crate::url_index::ensure(conn)? > 0
        } else {
            false
        };
        crate::url_index::ensure_deferred(conn)?;
        Ok(ensure(conn)? || urls || !deferred_present)
    })();
    match built {
        Err(StoreError::Sqlite(rusqlite::Error::SqliteFailure(f, _)))
            if matches!(
                f.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            Ok(false)
        }
        other => other,
    }
}
