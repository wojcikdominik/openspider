//! Kindprozess für Absturz- und Lasttests (CRAWL-125, PERF-005): crawlt eine Site in eine Crawl-Datei und meldet
//! den Fortschritt auf stdout, damit der Elternprozess ihn gezielt hart beenden kann (SIGKILL bzw.
//! `TerminateProcess`). Geliefert von M1-P19, benutzt von `lifecycle_crash.rs` und M1-P28.
//!
//! ```text
//! crawl_child --file <crawl.ospider> --start <url>
//!             [--resolve <host=ip[:port]>]...   # network.resolve, z. B. '*.test=127.0.0.1:8080'
//!             [--threads <n>] [--max-urls <n>] [--max-depth <n>]
//!             [--config <datei.osconfig>]       # Basis-Konfiguration (TOML); die Schalter oben überschreiben sie
//!             [--resume]                        # vorhandene Datei fortsetzen statt neu anlegen
//!             [--ca-cert <datei.pem>]           # zusätzliche Test-CA für https
//!             [--pause-after <n>]               # nach n Abrufen pausieren; ein Leser hält dabei einen Stand
//! ```
//!
//! Ausgabe (eine Zeile je Ereignis, sofort geleert):
//!
//! * `READY pid=<pid>` – die Crawl-Datei existiert, der Crawl läuft.
//! * `PROGRESS crawled=<n> queued=<n> discovered=<n>` – bei jeder Änderung von `crawled` (höchstens alle 20 ms).
//! * `DONE state=<Completed|Paused|…> end_reason=<…|-> crawled=<n>` – regulärer Abschluss oder Pause.
//! * `HOLDING wal=<bytes>` – nur mit `--pause-after`: die Pause ist quittiert, der Leser hält das WAL fest, der
//!   Prozess wartet auf den harten Abbruch (CRAWL-126).
//!
//! Exit-Code: 0 bei Abschluss oder Pause, 130 nach Strg+C (der Crawl wird zuvor pausiert), 1 bei einem Fehler
//! (Meldung auf stderr). Ein harter Abbruch hinterlässt eine Datei im Zustand „Unterbrochen“ (Heartbeat).

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use os_engine::{
    resume_crawl, start_crawl, CrawlHandle, CrawlRequest, CrawlSource, CrawlSummary, EngineDeps,
};
use os_fetch::{FetchConfig, HttpFetcher, MemorySecretStore};
use os_model::{CrawlConfig, RandomIds, SystemClock};
use os_store::StorageProfile;

struct Args {
    file: PathBuf,
    start: String,
    resolve: Vec<String>,
    threads: Option<u32>,
    max_urls: Option<u64>,
    max_depth: Option<u32>,
    config: Option<PathBuf>,
    ca_cert: Option<PathBuf>,
    resume: bool,
    pause_after: Option<u64>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        file: PathBuf::new(),
        start: String::new(),
        resolve: Vec::new(),
        threads: None,
        max_urls: None,
        max_depth: None,
        config: None,
        ca_cert: None,
        resume: false,
        pause_after: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .ok_or_else(|| format!("{name} braucht einen Wert"))
        };
        match flag.as_str() {
            "--file" => args.file = PathBuf::from(value("--file")?),
            "--start" => args.start = value("--start")?,
            "--resolve" => args.resolve.push(value("--resolve")?),
            "--threads" => {
                args.threads = Some(
                    value("--threads")?
                        .parse()
                        .map_err(|e| format!("--threads: {e}"))?,
                )
            }
            "--max-urls" => {
                args.max_urls = Some(
                    value("--max-urls")?
                        .parse()
                        .map_err(|e| format!("--max-urls: {e}"))?,
                )
            }
            "--max-depth" => {
                args.max_depth = Some(
                    value("--max-depth")?
                        .parse()
                        .map_err(|e| format!("--max-depth: {e}"))?,
                )
            }
            "--config" => args.config = Some(PathBuf::from(value("--config")?)),
            "--ca-cert" => args.ca_cert = Some(PathBuf::from(value("--ca-cert")?)),
            "--resume" => args.resume = true,
            "--pause-after" => {
                args.pause_after = Some(
                    value("--pause-after")?
                        .parse()
                        .map_err(|e| format!("--pause-after: {e}"))?,
                )
            }
            other => return Err(format!("unbekannter Schalter {other}")),
        }
    }
    if args.file.as_os_str().is_empty() {
        return Err("--file fehlt".to_string());
    }
    if !args.resume && args.start.is_empty() {
        return Err("--start fehlt".to_string());
    }
    Ok(args)
}

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn deps() -> EngineDeps {
    EngineDeps {
        clock: Arc::new(SystemClock),
        ids: Arc::new(RandomIds),
        profile: StorageProfile::Balanced,
        secrets: Arc::new(MemorySecretStore::new()),
        sink_factory: None,
        on_finished: None,
        owner_kind: "cli",
        renderer: None,
        ai_queue: None,
    }
}

fn report(summary: &CrawlSummary) {
    say(&format!(
        "DONE state={} end_reason={} crawled={}",
        summary.state.as_str(),
        summary.end_reason.map_or("-", |r| r.as_str()),
        summary.crawled
    ));
}

async fn run(args: Args) -> Result<i32, String> {
    let mut cfg = match &args.config {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            CrawlConfig::from_toml_str(&text).map_err(|e| e.to_string())?
        }
        None if args.resume => {
            // Konfiguration der Datei weiterverwenden
            os_store::CrawlFile::open(&args.file, os_store::OpenMode::ReadOnly)
                .and_then(|f| f.config())
                .map_err(|e| e.to_string())?
        }
        None => CrawlConfig::default(),
    };
    cfg.network.resolve.extend(args.resolve.iter().cloned());
    if let Some(t) = args.threads {
        cfg.speed.threads = t;
    }
    if let Some(n) = args.max_urls {
        cfg.limits.max_urls = n;
    }
    if let Some(d) = args.max_depth {
        cfg.limits.max_depth = Some(d);
    }
    let deps = deps();
    let mut fetch_cfg = FetchConfig::from_crawl_config(&cfg, &*deps.secrets, deps.clock.clone())
        .map_err(|e| e.to_string())?;
    if let Some(p) = &args.ca_cert {
        fetch_cfg.extra_root_certs =
            vec![std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?];
    }
    let fetcher = Arc::new(HttpFetcher::new(fetch_cfg).map_err(|e| e.to_string())?);

    let handle: CrawlHandle = if args.resume {
        resume_crawl(fetcher, args.file.clone(), Some(cfg), deps)
            .await
            .map_err(|e| e.to_string())?
    } else {
        start_crawl(
            fetcher,
            CrawlRequest {
                config: cfg,
                source: CrawlSource::Spider {
                    start_url: args.start.clone(),
                },
                file: args.file.clone(),
                project: "crawl_child".to_string(),
                name: "crawl_child".to_string(),
            },
            deps,
        )
        .await
        .map_err(|e| e.to_string())?
    };
    say(&format!("READY pid={}", std::process::id()));

    let mut watch = handle.watch();
    let progress = async move {
        let mut last = u64::MAX;
        while watch.changed().await.is_ok() {
            let p = watch.borrow_and_update().clone();
            if p.crawled != last {
                last = p.crawled;
                say(&format!(
                    "PROGRESS crawled={} queued={} discovered={}",
                    p.crawled, p.queued, p.discovered
                ));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::spawn(progress);

    if let Some(n) = args.pause_after {
        let mut watch = handle.watch();
        while watch.borrow_and_update().crawled < n {
            if watch.changed().await.is_err() {
                break;
            }
        }
        // Ein Leser hält einen älteren Stand: der Abschluss-Checkpoint kann das WAL nicht zurückführen.
        let reader = os_store::CrawlFile::open(&args.file, os_store::OpenMode::ReadOnly)
            .map_err(|e| e.to_string())?;
        reader
            .conn()
            .execute_batch("BEGIN")
            .map_err(|e| e.to_string())?;
        reader
            .conn()
            .query_row("SELECT COUNT(*) FROM urls", [], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        handle.pause();
        let summary = handle.wait().await.map_err(|e| e.to_string())?;
        report(&summary);
        let mut wal = args.file.as_os_str().to_owned();
        wal.push("-wal");
        let bytes = std::fs::metadata(PathBuf::from(wal)).map_or(0, |m| m.len());
        say(&format!("HOLDING wal={bytes}"));
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            let _ = reader.conn();
        }
    }

    let interrupted = {
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        tokio::select! {
            _ = &mut ctrl_c => true,
            _ = wait_for(&handle) => false,
        }
    };
    if interrupted {
        handle.pause();
    }
    let summary = handle.wait().await.map_err(|e| e.to_string())?;
    report(&summary);
    Ok(if interrupted { 130 } else { 0 })
}

/// Wartet, bis der Fortschritt einen Endzustand meldet (das Handle wird erst danach verbraucht).
async fn wait_for(handle: &CrawlHandle) {
    let mut watch = handle.watch();
    loop {
        let state = watch.borrow().state;
        if matches!(
            state,
            os_model::CrawlState::Completed
                | os_model::CrawlState::Paused
                | os_model::CrawlState::Failed
        ) {
            return;
        }
        if watch.changed().await.is_err() {
            return;
        }
    }
}

#[tokio::main]
async fn main() {
    let code = match parse_args() {
        Ok(args) => match run(args).await {
            Ok(code) => code,
            Err(msg) => {
                eprintln!("{msg}");
                1
            }
        },
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("Aufruf: crawl_child --file <datei> --start <url> [--resolve h=ip:port]... [--threads n] [--resume]");
            1
        }
    };
    std::process::exit(code);
}
