//! `os-loadtest diag` – Sondenmessung für den Windows-Durchsatz der Engine (M1-P30).
//!
//! Jede Sonde misst eine Hypothese einzeln, damit ein CI-Lauf (`perf-diag.yml`) die Ursache eingrenzt, statt nur
//! „langsam“ zu melden. Ergebnis: ein JSON-Objekt (Datei `--out`, außerdem auf stdout).
//!
//! | Sonde | Hypothese |
//! |---|---|
//! | `timers` | Timer-Auflösung des Betriebssystems (~15,6 ms unter Windows) für `thread::sleep`, `tokio::time::sleep` und `interval` |
//! | `fsync` | Kosten von `FlushFileBuffers`/`fsync`, Anlegen kleiner Dateien (Virenscanner) im Arbeitsordner |
//! | `connect` | Verbindungsaufbau zu `127.0.0.1`, `::1` und `localhost` (Namensauflösung, IPv6 zuerst), Dauer bei geschlossenem Port (SYN-Wiederholungen) |
//! | `http` | Abruf mit `HttpFetcher` ohne Engine: einzeln, mit 8 Parallelen, Wiederverwendung der Verbindungen, `localhost` statt `127.0.0.1` |
//! | `crawl` | ein Crawl im selben Prozess mit den Messzählern von Engine und Store (`os_engine::diag`, `os_store::sink::diag`) |

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use os_engine::{start_crawl, CrawlRequest, CrawlSource, EngineDeps};
use os_fetch::{Fetch, FetchConfig, FetchOutcome, FetchPurpose, FetchRequest, HttpFetcher};
use os_fetch::{MemorySecretStore, SecretStore};
use os_model::{Clock, CrawlConfig, RandomIds, SystemClock};
use os_store::StorageProfile;
use os_testsite::{SiteOptions, SyntheticConfig, SyntheticSite, TestSite};
use serde_json::{json, Map, Value};
use tokio::net::TcpStream;

use crate::scenario::{self, Size};
use crate::sink::tuned_sink_factory;
use crate::timers;

/// Alle Sonden in Ausführungsreihenfolge.
pub const PROBES: &[&str] = &["timers", "fsync", "connect", "http", "crawl"];

/// Parameter von `os-loadtest diag`.
#[derive(Debug, Clone)]
pub struct DiagOptions {
    /// Seiten der Site für `http` und `crawl`.
    pub pages: u64,
    /// Parallele Abrufe des Crawls und der parallelen HTTP-Messung.
    pub threads: u32,
    /// Ordner für Dateien der Sonden (`None` = temporärer Ordner).
    pub work_dir: Option<PathBuf>,
    /// Nur diese Sonden (leer = alle).
    pub only: Vec<String>,
    /// Speicherprofil des Crawls.
    pub profile: StorageProfile,
    /// `PRAGMA wal_autocheckpoint` des Crawls (Seiten; `None` = SQLite-Standard).
    pub wal_autocheckpoint: Option<u32>,
}

impl Default for DiagOptions {
    fn default() -> Self {
        Self {
            pages: 3_000,
            threads: 8,
            work_dir: None,
            only: Vec::new(),
            profile: StorageProfile::Balanced,
            wal_autocheckpoint: None,
        }
    }
}

/// Verteilung von Messwerten in Millisekunden.
pub fn dist(mut ms: Vec<f64>) -> Value {
    if ms.is_empty() {
        return json!({ "n": 0 });
    }
    ms.sort_by(|a, b| a.total_cmp(b));
    let n = ms.len();
    let at = |q: f64| ms[((n as f64 * q) as usize).min(n - 1)];
    let round = |v: f64| (v * 1000.0).round() / 1000.0;
    json!({
        "n": n,
        "mean_ms": round(ms.iter().sum::<f64>() / n as f64),
        "p50_ms": round(at(0.50)),
        "p95_ms": round(at(0.95)),
        "p99_ms": round(at(0.99)),
        "max_ms": round(ms[n - 1]),
    })
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn pairs(values: Vec<(&'static str, u64)>) -> Value {
    Value::Object(
        values
            .into_iter()
            .map(|(k, v)| (k.to_string(), json!(v)))
            .collect(),
    )
}

/// Führt die gewählten Sonden aus.
pub async fn run(opts: &DiagOptions) -> Value {
    let wanted = |name: &str| opts.only.is_empty() || opts.only.iter().any(|o| o == name);
    let scratch = match &opts.work_dir {
        Some(dir) => {
            let _ = std::fs::create_dir_all(dir);
            tempfile::Builder::new().prefix("os-diag-").tempdir_in(dir)
        }
        None => tempfile::Builder::new().prefix("os-diag-").tempdir(),
    };
    let scratch = match scratch {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("Arbeitsordner: {e}") }),
    };

    let mut out = Map::new();
    out.insert(
        "env".into(),
        json!({
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
            "release": !cfg!(debug_assertions),
            "temp_dir": std::env::temp_dir().display().to_string(),
            "scratch": scratch.path().display().to_string(),
            "ram_bytes": os_store::profile::physical_ram_bytes(),
            "pages": opts.pages,
            "threads": opts.threads,
        }),
    );
    if wanted("timers") {
        out.insert("timers".into(), probe_timers().await);
    }
    if wanted("fsync") {
        out.insert("fsync".into(), probe_fsync(scratch.path()));
    }
    let needs_site = ["connect", "http"].iter().any(|p| wanted(p));
    if needs_site {
        let cfg = SyntheticConfig {
            pages: opts.pages.max(100),
            ..Default::default()
        };
        match TestSite::start_synthetic(cfg, SiteOptions::default()).await {
            Ok(site) => {
                if wanted("connect") {
                    out.insert("connect".into(), probe_connect(&site).await);
                }
                if wanted("http") {
                    out.insert("http".into(), probe_http(&site, opts).await);
                }
            }
            Err(e) => {
                out.insert("site_error".into(), json!(e.to_string()));
            }
        }
    }
    if wanted("crawl") {
        out.insert("crawl".into(), probe_crawl(opts, scratch.path()).await);
    }
    Value::Object(out)
}

// ---- timers --------------------------------------------------------------------------------------------------

async fn probe_timers() -> Value {
    let thread = tokio::task::spawn_blocking(|| timers::measure_thread_sleep(300))
        .await
        .map(|l| pairs(l.values()))
        .unwrap_or(Value::Null);
    let tokio_sleep = timers::measure_tokio_sleep(300).await;
    let (interval, total) = timers::measure_tokio_interval(300).await;
    json!({
        "tick_ms": 1,
        "thread_sleep_1ms_lateness": thread,
        "tokio_sleep_1ms_lateness": pairs(tokio_sleep.values()),
        "tokio_interval_1ms_lateness": pairs(interval.values()),
        "tokio_interval_300_ticks_ms": (ms(total) * 10.0).round() / 10.0,
    })
}

// ---- fsync ---------------------------------------------------------------------------------------------------

fn probe_fsync(dir: &Path) -> Value {
    let path = dir.join("fsync.bin");
    let mut result = Map::new();
    let appended = (|| -> std::io::Result<(Vec<f64>, Vec<f64>, Vec<f64>)> {
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        let block = vec![0xA5u8; 4096];
        let mut small = Vec::new();
        for _ in 0..40 {
            f.write_all(&block)?;
            let t = Instant::now();
            f.sync_all()?;
            small.push(ms(t.elapsed()));
        }
        let big_block = vec![0x5Au8; 8 * 1024 * 1024];
        let mut big = Vec::new();
        for _ in 0..3 {
            f.write_all(&big_block)?;
            let t = Instant::now();
            f.sync_all()?;
            big.push(ms(t.elapsed()));
        }
        // viele kleine Dateien: Anlegen, Schreiben, Schließen (Echtzeitscan des Virenscanners)
        let files = dir.join("small-files");
        std::fs::create_dir_all(&files)?;
        let mut created = Vec::new();
        for i in 0..200 {
            let t = Instant::now();
            let mut g = std::fs::File::create(files.join(format!("f{i}.bin")))?;
            g.write_all(&[7u8; 65_536])?;
            drop(g);
            created.push(ms(t.elapsed()));
        }
        Ok((small, big, created))
    })();
    match appended {
        Ok((small, big, created)) => {
            result.insert("fsync_after_4k_append".into(), dist(small));
            result.insert("fsync_after_8mib_append".into(), dist(big));
            result.insert("create_write_close_64k".into(), dist(created));
        }
        Err(e) => {
            result.insert("error".into(), json!(e.to_string()));
        }
    }
    Value::Object(result)
}

// ---- connect -------------------------------------------------------------------------------------------------

async fn time_connects<A: tokio::net::ToSocketAddrs + Copy>(addr: A, n: usize) -> Value {
    let mut v = Vec::new();
    let mut errors = 0;
    for _ in 0..n {
        let t = Instant::now();
        match TcpStream::connect(addr).await {
            Ok(s) => {
                v.push(ms(t.elapsed()));
                drop(s);
            }
            Err(_) => errors += 1,
        }
    }
    let mut d = dist(v);
    d["errors"] = json!(errors);
    d
}

async fn probe_connect(site: &TestSite) -> Value {
    let port = site.port();
    let mut out = Map::new();
    out.insert(
        "v4_127_0_0_1".into(),
        time_connects(("127.0.0.1", port), 200).await,
    );
    if let Some(a6) = site.addr6() {
        out.insert("v6_loopback".into(), time_connects(a6, 200).await);
    } else {
        out.insert("v6_loopback".into(), json!("kein IPv6-Loopback"));
    }
    // Namensauflösung von `localhost` und Verbindungsaufbau über den Namen
    let mut lookups = Vec::new();
    let mut order = Vec::new();
    for _ in 0..100 {
        let t = Instant::now();
        if let Ok(found) = tokio::net::lookup_host(("localhost", port)).await {
            lookups.push(ms(t.elapsed()));
            if order.is_empty() {
                order = found.map(|a| a.ip().to_string()).collect();
            }
        }
    }
    out.insert("localhost_lookup".into(), dist(lookups));
    out.insert("localhost_order".into(), json!(order));
    out.insert(
        "localhost_connect".into(),
        time_connects(("localhost", port), 200).await,
    );
    // geschlossener Port: unter Windows wiederholt connect nach dem RST den SYN (bis ~2 s)
    let free = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port());
    if let Ok(closed) = free {
        let mut v = Vec::new();
        for _ in 0..3 {
            let t = Instant::now();
            let r = TcpStream::connect(("127.0.0.1", closed)).await;
            debug_assert!(r.is_err());
            v.push(ms(t.elapsed()));
        }
        out.insert("closed_port_refused".into(), dist(v));
    }
    Value::Object(out)
}

// ---- http ----------------------------------------------------------------------------------------------------

fn fetcher() -> Result<Arc<HttpFetcher>, String> {
    let cfg = CrawlConfig::default();
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::new());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let fetch_cfg =
        FetchConfig::from_crawl_config(&cfg, &*secrets, clock).map_err(|e| e.to_string())?;
    HttpFetcher::new(fetch_cfg)
        .map(Arc::new)
        .map_err(|e| e.to_string())
}

async fn get(f: &HttpFetcher, base: &str, id: u64) -> (f64, bool) {
    let url = format!("{base}{}", SyntheticSite::page_path(id));
    // `parse` leitet den Typ aus `FetchRequest.url` ab (os-loadtest hängt nicht an der `url`-Crate).
    let Ok(url) = url.parse() else {
        return (0.0, false);
    };
    let t = Instant::now();
    let result = f
        .fetch(FetchRequest {
            url,
            purpose: FetchPurpose::Page,
            cookies: None,
            max_body_bytes: 10 * 1024 * 1024,
            attempt: 0,
            body_limit: Default::default(),
        })
        .await;
    (
        ms(t.elapsed()),
        matches!(result.outcome, FetchOutcome::Response(_)),
    )
}

/// Abrufe von `base` mit `workers` Parallelen; liefert Verteilung, Durchsatz und die Zahl neuer Verbindungen.
async fn http_run(
    site: &TestSite,
    f: &Arc<HttpFetcher>,
    base: &str,
    total: usize,
    workers: usize,
    pages: u64,
) -> Value {
    let connections_before = site.connections();
    let start = Instant::now();
    let mut tasks = Vec::new();
    for w in 0..workers {
        let (f, base) = (Arc::clone(f), base.to_string());
        tasks.push(tokio::spawn(async move {
            let mut lat = Vec::new();
            let mut failed = 0usize;
            for i in (w..total).step_by(workers) {
                let (t, ok) = get(&f, &base, (i as u64).wrapping_mul(49_999) % pages).await;
                lat.push(t);
                if !ok {
                    failed += 1;
                }
            }
            (lat, failed)
        }));
    }
    let mut all = Vec::new();
    let mut failed = 0;
    for t in tasks {
        if let Ok((l, f)) = t.await {
            all.extend(l);
            failed += f;
        }
    }
    let secs = start.elapsed().as_secs_f64();
    let new_connections = site.connections().saturating_sub(connections_before);
    let mut d = dist(all);
    d["requests_per_s"] = json!((total as f64 / secs.max(1e-9)).round());
    d["seconds"] = json!((secs * 1000.0).round() / 1000.0);
    d["failed"] = json!(failed);
    d["new_connections"] = json!(new_connections);
    d["requests_per_connection"] =
        json!((total as f64 / new_connections.max(1) as f64 * 10.0).round() / 10.0);
    d
}

async fn probe_http(site: &TestSite, opts: &DiagOptions) -> Value {
    let f = match fetcher() {
        Ok(f) => f,
        Err(e) => return json!({ "error": e }),
    };
    let pages = opts.pages.max(100);
    let workers = opts.threads.max(1) as usize;
    let mut out = Map::new();
    let v4 = site.base_url();
    // Aufwärmen (erste Verbindung, Zertifikatsspeicher des Clients)
    let _ = get(&f, &v4, 0).await;
    out.insert(
        "sequential_127_0_0_1".into(),
        http_run(site, &f, &v4, 300, 1, pages).await,
    );
    out.insert(
        "parallel_127_0_0_1".into(),
        http_run(site, &f, &v4, 4_000, workers, pages).await,
    );
    let localhost = format!("http://localhost:{}", site.port());
    out.insert(
        "sequential_localhost".into(),
        http_run(site, &f, &localhost, 300, 1, pages).await,
    );
    out.insert(
        "parallel_localhost".into(),
        http_run(site, &f, &localhost, 4_000, workers, pages).await,
    );
    // `localhost` gegen einen Server, der nur auf 127.0.0.1 lauscht (wie die meisten Entwicklungsserver): der
    // Client versucht ::1 zuerst; unter Windows kostet der Fehlschlag auf einem geschlossenen Port bis zu ~2 s
    // (SYN-Wiederholungen), Happy Eyeballs fängt es nach 300 ms ab. Jede Messung mit frischem Client (neue Verbindung).
    out.insert(
        "first_request_localhost_v4_only_server".into(),
        probe_v4_only_localhost().await,
    );
    if let Some(a6) = site.addr6() {
        let v6 = format!("http://[::1]:{}", a6.port());
        out.insert(
            "parallel_v6_loopback".into(),
            http_run(site, &f, &v6, 4_000, workers, pages).await,
        );
    }
    Value::Object(out)
}

/// Minimaler Server nur auf 127.0.0.1: antwortet auf jede Anfrage mit `200 OK` und schließt die Verbindung.
fn v4_only_server() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        use std::io::Read as _;
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
            );
        }
    });
    Ok(port)
}

async fn probe_v4_only_localhost() -> Value {
    let Ok(port) = v4_only_server() else {
        return json!({ "error": "Server startet nicht" });
    };
    let mut named = Vec::new();
    let mut literal = Vec::new();
    for _ in 0..5 {
        for (base, into) in [
            (format!("http://localhost:{port}"), &mut named),
            (format!("http://127.0.0.1:{port}"), &mut literal),
        ] {
            let Ok(f) = fetcher() else {
                return json!({ "error": "Client" });
            };
            let (t, ok) = get(&f, &base, 0).await;
            if ok {
                into.push(t);
            }
        }
    }
    json!({ "localhost": dist(named), "literal_127_0_0_1": dist(literal) })
}

// ---- crawl ---------------------------------------------------------------------------------------------------

async fn probe_crawl(opts: &DiagOptions, scratch: &Path) -> Value {
    let def = scenario::find("perf-001").expect("perf-001");
    let cfg_site = SyntheticConfig {
        pages: opts.pages,
        ..def.config(Size::Small)
    };
    let site = match TestSite::start_synthetic(cfg_site, SiteOptions::default()).await {
        Ok(s) => s,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let mut cfg = CrawlConfig::default();
    cfg.speed.threads = opts.threads;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let mut deps = EngineDeps {
        clock: clock.clone(),
        ids: Arc::new(RandomIds),
        profile: opts.profile,
        secrets: Arc::new(MemorySecretStore::new()),
        sink_factory: None,
        on_finished: None,
        owner_kind: "cli",
        renderer: None,
        ai_queue: None,
    };
    if opts.wal_autocheckpoint.is_some() {
        deps.sink_factory = Some(tuned_sink_factory(
            None,
            opts.wal_autocheckpoint,
            clock.clone(),
        ));
    }
    let fetch_cfg = match FetchConfig::from_crawl_config(&cfg, &*deps.secrets, clock) {
        Ok(c) => c,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let fetcher = match HttpFetcher::new(fetch_cfg) {
        Ok(f) => Arc::new(f),
        Err(e) => return json!({ "error": e.to_string() }),
    };
    os_engine::diag::reset();
    os_store::sink::diag::reset();
    let file = scratch.join("diag-crawl.ospider");
    let monitors = timers::Monitors::start();
    let started = Instant::now();
    let handle = match start_crawl(
        fetcher,
        CrawlRequest {
            config: cfg,
            source: CrawlSource::Spider {
                start_url: site.url("/"),
            },
            file: file.clone(),
            project: "diag".into(),
            name: "diag".into(),
        },
        deps,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let summary = handle.wait().await;
    let seconds = started.elapsed().as_secs_f64();
    let lateness: BTreeMap<_, _> = monitors
        .stop()
        .await
        .into_iter()
        .map(|(g, v)| (g.to_string(), pairs(v)))
        .collect();
    let crawled = summary.as_ref().map(|s| s.crawled).unwrap_or(0);
    json!({
        "pages": opts.pages,
        "profile": crate::child::profile_name(opts.profile),
        "wal_autocheckpoint": opts.wal_autocheckpoint,
        "ok": summary.is_ok(),
        "error": summary.err().map(|e| e.to_string()),
        "crawled": crawled,
        "seconds": (seconds * 1000.0).round() / 1000.0,
        "urls_per_s": (crawled as f64 / seconds.max(1e-9)).round(),
        "site_requests": site.total_requests(),
        "site_connections": site.connections(),
        "file_bytes": std::fs::metadata(&file).map(|m| m.len()).ok(),
        "engine": pairs(os_engine::diag::snapshot()),
        "store": pairs(os_store::sink::diag::snapshot()),
        "lateness": lateness,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Infrastruktur (M1-P30), kein Spec-Tag.
    #[test]
    fn dist_reports_percentiles_of_sorted_values() {
        let d = dist((1..=100).map(f64::from).collect());
        assert_eq!(d["n"], 100);
        assert_eq!(d["max_ms"], 100.0);
        assert_eq!(d["p50_ms"], 51.0);
        assert_eq!(dist(vec![]), json!({ "n": 0 }));
    }

    // Infrastruktur (M1-P30), kein Spec-Tag.
    #[tokio::test]
    async fn the_probes_run_against_a_small_site() {
        let dir = tempfile::tempdir().unwrap();
        let opts = DiagOptions {
            pages: 100,
            threads: 2,
            work_dir: Some(dir.path().to_path_buf()),
            ..DiagOptions::default()
        };
        let report = run(&opts).await;
        assert_eq!(report["crawl"]["ok"], true, "{report}");
        assert_eq!(report["crawl"]["crawled"], 100, "{report}");
        assert!(
            report["http"]["parallel_127_0_0_1"]["requests_per_s"]
                .as_f64()
                .unwrap()
                > 0.0
        );
        assert!(report["connect"]["v4_127_0_0_1"]["n"].as_u64().unwrap() > 0);
        assert!(
            report["fsync"]["fsync_after_4k_append"]["n"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(
            report["timers"]["tokio_interval_300_ticks_ms"]
                .as_f64()
                .unwrap()
                > 0.0
        );
        let engine = &report["crawl"]["engine"];
        assert!(engine["items"].as_u64().unwrap() >= 100, "{engine}");
        assert!(engine["fetch_ns"].as_u64().unwrap() > 0);
    }
}
