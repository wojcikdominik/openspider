//! Kindprozess für PERF-001/003: crawlt eine Site in eine Crawl-Datei (`os-loadtest crawl-child ...`).
//!
//! Der Elternprozess ([`crate::engine::EngineCrawler`]) misst von außen den Spitzen-RSS dieses Prozesses. Das
//! Protokoll auf stdout ist dasselbe wie bei `examples/crawl_child.rs` von os-engine (P19), damit ein Parser
//! beide Kinder liest:
//!
//! * `READY pid=<pid>` – die Crawl-Datei existiert, der Crawl läuft.
//! * `PROGRESS crawled=<n> queued=<n> discovered=<n>` – bei jeder Änderung von `crawled`, höchstens alle 20 ms.
//! * `DONE state=<…> end_reason=<…|-> crawled=<n>` – Abschluss oder Pause.
//! * `DIAG <gruppe> <name>=<n> …` – Messzähler am Ende (nur dieses Kind, M1-P30): `engine` (os-engine),
//!   `store` (os-store), `loadtest` (bei `--slow-sink-ms`: Wartezeit und Aufrufe der verlangsamten Senke), bei `OS_LOADTEST_DIAG=1` zusätzlich `rt` (Verspätung des Tokio-Timers) und `os_sleep`
//!   (Verspätung von `std::thread::sleep`). Der Elternprozess legt sie unter `diag` in den Bericht.
//!
//! Gegenüber `crawl_child` kann dieses Kind die Senke tauschen (`--slow-sink-ms`, PERF-003) und das
//! Speicherprofil wählen (`--profile`, CRAWL-124).

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use os_engine::{start_crawl, CrawlHandle, CrawlRequest, CrawlSource, CrawlSummary, EngineDeps};
use os_fetch::{FetchConfig, HttpFetcher, MemorySecretStore};
use os_model::{CrawlConfig, CrawlState, RandomIds, SystemClock};
use os_store::StorageProfile;

use crate::sink::tuned_sink_factory;

/// Parameter des Kindes.
#[derive(Debug, Clone)]
pub struct ChildArgs {
    /// Zu erzeugende Crawl-Datei.
    pub file: PathBuf,
    /// Start-URL.
    pub start: String,
    /// `network.resolve`-Einträge (`host=ip:port`).
    pub resolve: Vec<String>,
    /// Parallele Abrufe.
    pub threads: u32,
    /// Speicherprofil.
    pub profile: StorageProfile,
    /// Verzögerung je Batch der Senke in Millisekunden (`None` = normale `SqliteSink`).
    pub slow_sink_ms: Option<u64>,
    /// `PRAGMA wal_autocheckpoint` in Seiten (`None` = Standard von SQLite, 1000).
    pub wal_autocheckpoint: Option<u32>,
}

/// Name eines Profils auf der Kommandozeile.
pub fn profile_name(p: StorageProfile) -> &'static str {
    match p {
        StorageProfile::Balanced => "balanced",
        StorageProfile::MaxSpeed => "max-speed",
        StorageProfile::MinMemory => "min-memory",
    }
}

/// Profil aus dem Namen auf der Kommandozeile.
pub fn parse_profile(s: &str) -> Result<StorageProfile, String> {
    match s {
        "balanced" => Ok(StorageProfile::Balanced),
        "max-speed" => Ok(StorageProfile::MaxSpeed),
        "min-memory" => Ok(StorageProfile::MinMemory),
        other => Err(format!(
            "unbekanntes Profil `{other}` (balanced, max-speed, min-memory)"
        )),
    }
}

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

// GNU allocator accounting separates live allocations from free arena memory retained
// by the process. Other platforms still report the portable engine counters and RSS.
fn allocator_profile() -> serde_json::Value {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: mallinfo2 takes no pointers and returns a process-wide snapshot.
        let m = unsafe { libc::mallinfo2() };
        serde_json::json!({
            "arena_bytes": m.arena, "in_use_bytes": m.uordblks,
            "free_arena_bytes": m.fordblks, "mmap_bytes": m.hblkhd,
        })
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    serde_json::Value::Null
}

// Opt-in profile; peak counters are cumulative, allocator/status values are current.
fn memory_sample(
    path: &std::ffi::OsStr,
    crawled: u64,
    queued: u64,
    started: std::time::Instant,
    completed: bool,
) {
    let counters: std::collections::BTreeMap<_, _> =
        os_engine::diag::snapshot().into_iter().collect();
    let sample = serde_json::json!({
        "pid": std::process::id(), "seconds": started.elapsed().as_secs_f64(),
        "crawled": crawled, "queued": queued, "completed": completed,
        "peak_rss_bytes": crate::rss::peak_rss_self(), "allocator": allocator_profile(),
        "proc_status": std::fs::read_to_string("/proc/self/status").ok(),
        "engine_peak_counters": counters,
        "sink_peak_counters": crate::sink::diag_snapshot().into_iter().collect::<std::collections::BTreeMap<_, _>>(),
    });
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{sample}");
    }
}

/// Eine `DIAG`-Zeile: `DIAG <gruppe> <name>=<n> …`.
pub fn diag_line(group: &str, values: &[(&'static str, u64)]) -> String {
    let mut line = format!("DIAG {group}");
    for (name, value) in values {
        line.push_str(&format!(" {name}={value}"));
    }
    line
}

fn report(summary: &CrawlSummary) {
    say(&format!(
        "DONE state={} end_reason={} crawled={}",
        summary.state.as_str(),
        summary.end_reason.map_or("-", |r| r.as_str()),
        summary.crawled
    ));
}

/// Beendet das Warten, sobald der Crawl einen Endzustand meldet (das Handle wird erst danach verbraucht).
async fn wait_for_end(handle: &CrawlHandle) {
    let mut watch = handle.watch();
    loop {
        let state = watch.borrow().state;
        if matches!(
            state,
            CrawlState::Completed | CrawlState::Paused | CrawlState::Failed
        ) {
            return;
        }
        if watch.changed().await.is_err() {
            return;
        }
    }
}

/// Führt den Crawl aus; Rückgabe ist der Exit-Code (0 bei Abschluss).
pub async fn run(args: ChildArgs) -> Result<i32, String> {
    let mut cfg = CrawlConfig::default();
    cfg.network.resolve.extend(args.resolve.iter().cloned());
    cfg.speed.threads = args.threads;
    let clock = Arc::new(SystemClock);
    let mut deps = EngineDeps {
        clock: clock.clone(),
        ids: Arc::new(RandomIds),
        profile: args.profile,
        secrets: Arc::new(MemorySecretStore::new()),
        sink_factory: None,
        on_finished: None,
        owner_kind: "cli",
        renderer: None,
        ai_queue: None,
    };
    if args.slow_sink_ms.is_some() || args.wal_autocheckpoint.is_some() {
        deps.sink_factory = Some(tuned_sink_factory(
            args.slow_sink_ms.map(Duration::from_millis),
            args.wal_autocheckpoint,
            clock,
        ));
    }
    let fetch_cfg = FetchConfig::from_crawl_config(&cfg, &*deps.secrets, deps.clock.clone())
        .map_err(|e| e.to_string())?;
    let fetcher = Arc::new(HttpFetcher::new(fetch_cfg).map_err(|e| e.to_string())?);
    let peak_monitor = crate::rss::PeakMonitor::start();
    let handle = start_crawl(
        fetcher,
        CrawlRequest {
            config: cfg,
            source: CrawlSource::Spider {
                start_url: args.start.clone(),
            },
            file: args.file.clone(),
            project: "loadtest".to_string(),
            name: "loadtest".to_string(),
        },
        deps,
    )
    .await
    .map_err(|e| e.to_string())?;
    say(&format!("READY pid={}", std::process::id()));

    let started = std::time::Instant::now();
    let mut watch = handle.watch();
    tokio::spawn(async move {
        let mut last = u64::MAX;
        let mut next_profile = 0;
        let profile_path = std::env::var_os("OS_LOADTEST_MEMORY_LOG");
        while watch.changed().await.is_ok() {
            let p = watch.borrow_and_update().clone();
            if p.crawled >= next_profile {
                next_profile = p.crawled + 100_000;
                if let Some(path) = &profile_path {
                    memory_sample(path, p.crawled, p.queued, started, false);
                }
            }
            if p.crawled != last {
                last = p.crawled;
                say(&format!(
                    "PROGRESS crawled={} queued={} discovered={}",
                    p.crawled, p.queued, p.discovered
                ));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    let monitors = (std::env::var("OS_LOADTEST_DIAG").as_deref() == Ok("1"))
        .then(crate::timers::Monitors::start);
    wait_for_end(&handle).await;
    let summary = handle.wait().await.map_err(|e| e.to_string())?;
    if let Some(path) = std::env::var_os("OS_LOADTEST_MEMORY_LOG") {
        memory_sample(&path, summary.crawled, 0, started, true);
    }
    if let Some(m) = monitors {
        for (group, values) in m.stop().await {
            say(&diag_line(group, &values));
        }
    }
    say(&diag_line("memory", &peak_monitor.finish()));
    say(&diag_line("engine", &os_engine::diag::snapshot()));
    say(&diag_line("store", &os_store::sink::diag::snapshot()));
    if args.slow_sink_ms.is_some() {
        say(&diag_line(
            crate::sink::DIAG_GROUP,
            &crate::sink::diag_snapshot(),
        ));
    }
    report(&summary);
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diag_lines_have_the_protocol_shape() {
        let line = diag_line("engine", &[("items", 3), ("fetch_ns", 40)]);
        assert_eq!(line, "DIAG engine items=3 fetch_ns=40");
    }

    #[test]
    fn profile_names_round_trip() {
        for p in [
            StorageProfile::Balanced,
            StorageProfile::MaxSpeed,
            StorageProfile::MinMemory,
        ] {
            assert_eq!(parse_profile(profile_name(p)).unwrap(), p);
        }
        assert!(parse_profile("memory").is_err());
    }
}
