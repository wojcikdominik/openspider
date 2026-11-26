//! Unabhängige Abnahme M4-K11 (os-engine): Fortsetzen, erneutes Crawlen, Entfernen und Stoppen ohne Engine bei einem
//! aktiven Besitzer außerhalb von `Running`; unverändertes Verhalten für Dateien aus 1.0; Intervall-Konstante.
//!
//! Erwartetes Verhalten ausschließlich aus 08 AI-020, Nachtrag 26.11.2026, Unterpunkt „Aktiver Besitzer außerhalb
//! von `Running`“ (Wirkung b, Kriterien 2 und 6) und 01 CRAWL-125, Nachtrag K4-22 („Wer liest“: die Engine weist mit
//! „Der Crawl läuft noch (Prozess <pid>) …“ ab). Heartbeat simuliert, Uhr `ManualClock`; nur `remove_urls` liest die
//! Systemuhr (kein Parameter) – dort liegt der simulierte Heartbeat 60 s in der Zukunft bzw. 11 s zurück.

#[path = "../../os-store/tests/acceptance_k11_support/mod.rs"]
mod k11;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use k11::*;
use os_engine::lifecycle::stop_unowned;
use os_engine::testing::test_deps;
use os_engine::{recrawl_urls, remove_urls, resume_crawl, EngineDeps};
use os_fetch::{FetchConfig, HttpFetcher};
use os_model::{Clock, CrawlState, ManualClock, SystemClock, UrlId};
use os_store::{CrawlFile, OpenMode};

fn deps_at(clock: Arc<ManualClock>) -> EngineDeps {
    let mut d = test_deps();
    d.clock = clock;
    d
}

fn fetcher(path: &Path, deps: &EngineDeps) -> Arc<HttpFetcher> {
    let cfg = CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .config()
        .unwrap();
    let fetch = FetchConfig::from_crawl_config(&cfg, &*deps.secrets, deps.clock.clone()).unwrap();
    Arc::new(HttpFetcher::new(fetch).unwrap())
}

async fn resume_err(path: &Path, deps: EngineDeps) -> String {
    let f = fetcher(path, &deps);
    match resume_crawl(f, path.to_path_buf(), None, deps).await {
        Ok(handle) => {
            handle.stop();
            let _ = handle.wait().await;
            "Fortsetzen wurde nicht abgewiesen".to_string()
        }
        Err(e) => e.to_string(),
    }
}

fn running_hint(pid: &str) -> String {
    format!("läuft noch (Prozess {pid})")
}

fn add_queue(path: &Path, n: i64) {
    let f = CrawlFile::open(path, OpenMode::ReadWrite).unwrap();
    for i in 1..=n {
        f.conn()
            .execute(
                "INSERT INTO queue(url_id, seq, depth, priority) VALUES (?1, ?1, 0, 0)",
                [i],
            )
            .unwrap();
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test]
async fn resume_recrawl_and_stop_are_rejected_while_a_foreign_process_is_the_active_owner() {
    // Wirkung (b): Fortsetzen, erneutes Crawlen von URLs und Stoppen werden abgewiesen wie bei einem fremd geführten
    // laufenden Crawl – Engine mit dem Hinweis „läuft noch (Prozess <pid>)“. Kriterium 2 nennt `Completed`.
    let clock = Arc::new(ManualClock::new(base_now()));
    for (i, state) in [
        CrawlState::Completed,
        CrawlState::Paused,
        CrawlState::Incomplete,
    ]
    .into_iter()
    .enumerate()
    {
        let dir = TempDir::new("engine-foreign");
        let (_, path) = new_crawl(dir.path(), 10 + i as u64, state);
        if state == CrawlState::Completed {
            // Abgeschlossen mit Restwarteschlange (Limit) wäre ohne Besitzer fortsetzbar.
            set(&path, "end_reason", "limit_reached");
            add_queue(&path, 2);
        }
        set_owner(&path, "mcp-stdio", FOREIGN_PID, Some(clock.now()));
        let before = meta_dump(&path);

        let err = resume_err(&path, deps_at(clock.clone())).await;
        assert!(
            err.contains(&running_hint(FOREIGN_PID)),
            "{state:?} Fortsetzen: {err}"
        );

        let err = match recrawl_urls(
            fetcher(&path, &deps_at(clock.clone())),
            &path,
            &[UrlId(1)],
            deps_at(clock.clone()),
        )
        .await
        {
            Ok(()) => "erneutes Crawlen wurde nicht abgewiesen".to_string(),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains(&running_hint(FOREIGN_PID)),
            "{state:?} erneut crawlen: {err}"
        );

        if state != CrawlState::Completed {
            let err = stop_unowned(&path, &deps_at(clock.clone()))
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Stoppen wurde nicht abgewiesen".into());
            assert!(
                err.contains(&running_hint(FOREIGN_PID)),
                "{state:?} Stoppen: {err}"
            );
        }
        assert_eq!(meta_dump(&path), before, "{state:?}: Datei unverändert");
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test]
async fn resuming_a_completed_crawl_with_empty_queue_names_the_foreign_owner() {
    // Kriterium 2 wörtlich: „Crawl-Datei mit `Completed`, fremdem `owner_pid` und aktuellem (simuliertem) Heartbeat:
    // … Fortsetzen … mit „läuft noch“ abgewiesen“ – vor dem Hinweis „nichts fortzusetzen“.
    let clock = Arc::new(ManualClock::new(base_now()));
    let dir = TempDir::new("engine-completed");
    let (_, path) = new_crawl(dir.path(), 20, CrawlState::Completed);
    set_owner(&path, "cli", FOREIGN_PID, Some(clock.now()));
    let err = resume_err(&path, deps_at(clock.clone())).await;
    assert!(err.contains(&running_hint(FOREIGN_PID)), "{err}");
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test]
async fn stopping_without_an_engine_succeeds_eleven_seconds_after_the_last_foreign_heartbeat() {
    // Kriterium 2: „11 s später ohne neuen Heartbeat“ – Stoppen ohne Engine setzt in einem Commit `heartbeat_at` =
    // jetzt und leert `owner_pid` (CRAWL-125).
    let clock = Arc::new(ManualClock::new(base_now()));
    let dir = TempDir::new("engine-eleven");
    let (_, path) = new_crawl(dir.path(), 30, CrawlState::Paused);
    set_owner(&path, "cli", FOREIGN_PID, Some(clock.now()));
    clock.advance(Duration::from_millis(10_000));
    assert!(stop_unowned(&path, &deps_at(clock.clone())).is_err());
    clock.advance(Duration::from_millis(1_000));
    let summary = stop_unowned(&path, &deps_at(clock.clone())).unwrap();
    assert_eq!(summary.state, CrawlState::Completed);
    assert_eq!(get(&path, "owner_pid").as_deref(), Some(""));
    assert_eq!(get(&path, "end_reason").as_deref(), Some("stopped"));
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test]
async fn with_the_own_pid_as_active_owner_effect_b_does_not_apply() {
    // „(b) Ist `owner_pid` nicht der eigene Prozess …“ – mit eigener PID wird nicht mit „läuft noch“ abgewiesen.
    let clock = Arc::new(ManualClock::new(base_now()));
    let dir = TempDir::new("engine-own");
    let (_, completed) = new_crawl(dir.path(), 40, CrawlState::Completed);
    set_owner(&completed, "app", &own_pid(), Some(clock.now()));
    let err = resume_err(&completed, deps_at(clock.clone())).await;
    assert!(!err.contains("läuft noch"), "{err}");
    assert!(err.contains("Warteschlange leer"), "{err}");

    let (_, paused) = new_crawl(dir.path(), 41, CrawlState::Paused);
    set_owner(&paused, "app", &own_pid(), Some(clock.now()));
    let summary = stop_unowned(&paused, &deps_at(clock.clone())).unwrap();
    assert_eq!(summary.state, CrawlState::Completed);
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn removing_urls_is_rejected_while_a_foreign_process_is_the_active_owner() {
    // Entfernen schreibt in die Datei wie erneutes Crawlen; Wirkung (b) „wie bei einem fremd geführten laufenden
    // Crawl“ (1.0 weist dort ab). Systemuhr: Heartbeat 60 s in der Zukunft gilt als aktuell (CRAWL-125).
    let dir = TempDir::new("engine-remove");
    let (_, path) = new_crawl(dir.path(), 50, CrawlState::Completed);
    set_owner(
        &path,
        "cli",
        FOREIGN_PID,
        Some(ago(SystemClock.now(), -60_000)),
    );
    let before = meta_dump(&path);
    let err = remove_urls(&path, &[UrlId(1)]).unwrap_err().to_string();
    assert!(err.contains(&running_hint(FOREIGN_PID)), "{err}");
    assert_eq!(meta_dump(&path), before);
    set(
        &path,
        "heartbeat_at",
        &ago(SystemClock.now(), 11_000).to_db(),
    );
    remove_urls(&path, &[UrlId(1)]).unwrap();
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test]
async fn files_from_1_0_are_rejected_by_the_engine_exactly_when_running_by_crawl_125() {
    // Kriterium 6 für die Engine: „läuft noch“ genau dann, wenn CRAWL-125 allein „laufend“ ergibt (Stoppen ohne
    // Engine als Leser, der keine Fixture-Site braucht).
    let (now, cases) = v10_cases();
    assert_fixture_consistent(&cases);
    for (i, case) in cases.iter().enumerate() {
        let dir = TempDir::new("engine-v10");
        let (_, path) = case.build(dir.path(), 100 + i as u64, now);
        let clock = Arc::new(ManualClock::new(now));
        let result = stop_unowned(&path, &deps_at(clock)).map(|s| s.state);
        let rejected_running = result
            .as_ref()
            .err()
            .is_some_and(|e| e.to_string().contains("läuft noch"));
        assert_eq!(
            rejected_running,
            case.oracle_running(),
            "{}: {result:?}",
            case.name
        );
        if let (true, Pid::Value(pid)) = (case.oracle_running(), &case.owner_pid) {
            if !pid.is_empty() {
                let err = result.as_ref().unwrap_err().to_string();
                assert!(err.contains(&running_hint(pid)), "{}: {err}", case.name);
            }
        }
        let stoppable = matches!(
            case.oracle_state(),
            CrawlState::Paused | CrawlState::Incomplete
        );
        assert_eq!(result.is_ok(), stoppable, "{}: {result:?}", case.name);
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn the_engine_heartbeat_interval_is_unchanged() {
    // Konstanten: Intervall 2 s und Grenze 10 000 ms sind dieselben Werte wie in CRAWL-125.
    assert_eq!(os_engine::crawl::HEARTBEAT_INTERVAL, Duration::from_secs(2));
    assert_eq!(
        os_engine::crawl::HEARTBEAT_INTERVAL,
        os_store::owner::HEARTBEAT_INTERVAL
    );
    assert_eq!(os_store::resume::HEARTBEAT_TIMEOUT_MS, 10_000);
}
