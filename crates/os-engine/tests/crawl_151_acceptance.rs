//! Abnahmetests CRAWL-151 „Getrennte Verbindungen pro Host“, allein aus der Spec geschrieben.
//!
//! Die Crawls laufen echt gegen den Fixture-Server (`os-testsite`): HTTP/2 über TLS/ALPN (`https://`),
//! HTTP/1.1 im Klartext (`http://`). Der Server zählt angenommene TCP-Verbindungen (`connections`),
//! gleichzeitig bearbeitete Anfragen (`max_concurrency`) und schreibt je Anfrage die Verbindungsnummer
//! (`RequestRecord::conn_id`) ins Log. Was er nicht kennt, steht hier in eigenen Hilfen:
//!
//! * [`Gate`] – TCP-Weiterleitung vor dem Fixture-Server, die je Client-Adresse höchstens
//!   `max_connections_per_client` offene Verbindungen annimmt und jede weitere sofort schließt
//!   (Ablehnung im Sinne von „Verbindungsfehler/TLS-Fehler direkt nach dem Aufbau“).
//! * [`overlap_lower_bound`] – gleichzeitige Anfragen je Verbindung als **untere Schranke** aus dem
//!   Request-Log: Jede Seite antwortet frühestens nach `delay_ms`; zwei Anfragen derselben Verbindung,
//!   deren Eingang weniger als `delay_ms` auseinanderliegt, liefen also sicher gleichzeitig. Die Schranke
//!   kann eine Überschreitung übersehen, meldet aber nie eine, die es nicht gab (kein Wackeln auf
//!   langsamen Maschinen).
//!
//! Konfiguration ausschließlich über den Spec-Schlüssel `speed.connections_per_host` per
//! `CrawlConfig::set_value` (derselbe Weg wie `--set`, OUT-186); der Rust-Typ des Feldes bleibt offen.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_engine::lifecycle::apply_paused_config;
use os_engine::testing::test_deps;
use os_engine::{resume_crawl, start_crawl, CrawlHandle, CrawlRequest, CrawlSource, EngineDeps};
use os_fetch::{FetchConfig, HttpFetcher};
use os_model::{CrawlConfig, CrawlState, EndReason, HttpVersion, SystemClock};
use os_store::{CrawlFile, OpenMode};
use os_testsite::{RequestRecord, TestSite};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};

const KEY: &str = "speed.connections_per_host";
/// Obergrenze für jeden einzelnen Crawl (großzügig; die Crawls dauern regulär wenige Sekunden).
const CRAWL_TIMEOUT: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------------------------------------------
// Fixtures

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Proto {
    /// HTTP/2 über TLS (ALPN `h2`).
    H2,
    /// HTTP/1.1 im Klartext.
    H1,
}

impl Proto {
    fn scheme(self) -> &'static str {
        match self {
            Proto::H2 => "https",
            Proto::H1 => "http",
        }
    }
}

/// Flache Site: `/` verlinkt `/p/1` … `/p/<pages>`; jede Seite antwortet nach `delay_ms`.
/// Alle Seiten sind sofort nach der Startseite bekannt, die Threads sind also ausgelastet.
fn flat_site(pages: usize, delay_ms: u64) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut links = String::new();
    for n in 1..=pages {
        links.push_str(&format!("<a href='/p/{n}'>Seite {n}</a> "));
    }
    let mut toml = format!(
        "[[route]]\npath = \"/\"\ncontent_type = \"text/html; charset=utf-8\"\n\
         body = \"<!doctype html><html><head><title>Start</title></head><body>{links}</body></html>\"\n\n"
    );
    for n in 1..=pages {
        toml.push_str(&format!(
            "[[route]]\npath = \"/p/{n}\"\ncontent_type = \"text/html; charset=utf-8\"\ndelay_ms = {delay_ms}\n\
             body = \"<!doctype html><html><head><title>Seite {n}</title></head><body><h1>Seite {n}</h1>\
             <a href='/'>Start</a></body></html>\"\n\n"
        ));
    }
    std::fs::write(dir.path().join("scenario.toml"), toml).unwrap();
    dir
}

/// Kopie von `crawl-site-100` mit `delay_ms` auf jeder der 100 Seiten (wie in der Spec gefordert).
fn site_100(delay_ms: u64) -> tempfile::TempDir {
    let src = os_testsite::fixtures_root().join("crawl-site-100");
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&src, dir.path());
    let mut toml = String::new();
    for n in 0..100 {
        let path = if n == 0 {
            "/".to_string()
        } else {
            format!("/page-{n}.html")
        };
        toml.push_str(&format!(
            "[[route]]\npath = \"{path}\"\ndelay_ms = {delay_ms}\n\n"
        ));
    }
    std::fs::write(dir.path().join("scenario.toml"), toml).unwrap();
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

async fn start_site(dir: &Path) -> Arc<TestSite> {
    Arc::new(TestSite::start_dir(dir, 0).await.unwrap())
}

// ---------------------------------------------------------------------------------------------------
// Konfiguration und Crawl

/// Grundkonfiguration: Threads, Protokoll und `speed.connections_per_host`. Auto-Tempo und adaptive
/// Drosselung (CRAWL-146/147) sind aus, damit die Thread-Zahl während des Tests feststeht.
fn config(proto: Proto, threads: u32, connections_per_host: &str) -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    cfg.speed.threads = threads;
    cfg.speed.adaptive_throttle = false;
    cfg.speed.auto = false;
    cfg.http.version = match proto {
        Proto::H2 => HttpVersion::Http2Preferred,
        Proto::H1 => HttpVersion::Http1,
    };
    set_connections(&mut cfg, connections_per_host);
    cfg
}

fn set_connections(cfg: &mut CrawlConfig, value: &str) {
    cfg.set_value(KEY, value).unwrap_or_else(|e| {
        panic!("{KEY} = {value:?} muss ein gültiger Konfigurationswert sein (CRAWL-151): {e}")
    });
}

/// Echte Uhr: Rate-Limit (CRAWL-113) und Pause laufen in Echtzeit gegen den Fixture-Server.
fn deps() -> EngineDeps {
    let mut deps = test_deps();
    deps.clock = Arc::new(SystemClock);
    deps
}

/// HTTP-Abrufer, der `*.test` auf `target` auflöst und der Test-CA der Site vertraut.
fn fetcher(
    cfg: &CrawlConfig,
    target: SocketAddr,
    site: &TestSite,
    deps: &EngineDeps,
) -> Arc<HttpFetcher> {
    let mut cfg = cfg.clone();
    cfg.network.resolve = vec![format!("*.test={target}")];
    let mut fetch_cfg =
        FetchConfig::from_crawl_config(&cfg, &*deps.secrets, deps.clock.clone()).unwrap();
    fetch_cfg.extra_root_certs = vec![site.ca_cert_pem().into_bytes()];
    Arc::new(HttpFetcher::new(fetch_cfg).unwrap())
}

struct Crawl {
    file: CrawlFile,
    state: CrawlState,
    end_reason: Option<EndReason>,
    _dir: tempfile::TempDir,
}

async fn begin(
    site: &TestSite,
    target: SocketAddr,
    mut cfg: CrawlConfig,
    source: CrawlSource,
    path: &Path,
) -> CrawlHandle {
    cfg.network.resolve = vec![format!("*.test={target}")];
    let deps = deps();
    let f = fetcher(&cfg, target, site, &deps);
    start_crawl(
        f,
        CrawlRequest {
            config: cfg,
            source,
            file: path.to_path_buf(),
            project: "crawl-151".to_string(),
            name: "crawl-151".to_string(),
        },
        deps,
    )
    .await
    .unwrap_or_else(|e| panic!("Crawl startet nicht: {e}"))
}

async fn finish(handle: CrawlHandle) -> os_engine::CrawlSummary {
    tokio::time::timeout(CRAWL_TIMEOUT, handle.wait())
        .await
        .expect("Crawl endet innerhalb des Timeouts")
        .unwrap_or_else(|e| panic!("Crawl schlägt fehl: {e}"))
}

/// Crawlt vollständig; `target` ist der Fixture-Server selbst oder ein [`Gate`] davor.
async fn crawl_via(
    site: &TestSite,
    target: SocketAddr,
    cfg: CrawlConfig,
    source: CrawlSource,
) -> Crawl {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crawl.ospider");
    let handle = begin(site, target, cfg, source, &path).await;
    let summary = finish(handle).await;
    Crawl {
        file: CrawlFile::open(&path, OpenMode::ReadWrite).unwrap(),
        state: summary.state,
        end_reason: summary.end_reason,
        _dir: dir,
    }
}

fn spider(proto: Proto) -> CrawlSource {
    CrawlSource::Spider {
        start_url: format!("{}://site.test/", proto.scheme()),
    }
}

async fn crawl(site: &TestSite, cfg: CrawlConfig, proto: Proto) -> Crawl {
    crawl_via(site, site.addr(), cfg, spider(proto)).await
}

fn scalar(c: &Crawl, sql: &str) -> i64 {
    c.file.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn assert_complete(c: &Crawl, what: &str) {
    assert_eq!(c.state, CrawlState::Completed, "{what}: Zustand");
    assert_eq!(
        c.end_reason,
        Some(EndReason::QueueEmpty),
        "{what}: Endgrund"
    );
}

/// Prüft, dass der Crawl tatsächlich über HTTP/2 lief (sonst wäre der Test für h2 wertlos).
fn assert_http2(c: &Crawl) {
    let not_h2 = scalar(
        c,
        "SELECT COUNT(*) FROM urls WHERE status_code = 200 AND content_class = 'HTML' \
         AND (http_version IS NULL OR http_version NOT LIKE 'HTTP/2%')",
    );
    let h2 = scalar(
        c,
        "SELECT COUNT(*) FROM urls WHERE status_code = 200 AND http_version LIKE 'HTTP/2%'",
    );
    assert!(
        h2 > 0 && not_h2 == 0,
        "Crawl lief nicht über HTTP/2 ({h2} h2, {not_h2} andere)"
    );
}

fn html_200(c: &Crawl) -> i64 {
    scalar(
        c,
        "SELECT COUNT(*) FROM urls WHERE content_class = 'HTML' AND status_code = 200",
    )
}

// ---------------------------------------------------------------------------------------------------
// Auswertung des Request-Logs

fn is_page(r: &RequestRecord) -> bool {
    r.path().starts_with("/p/") || r.path() == "/" || r.path().starts_with("/page-")
}

fn distinct_conns<'a>(records: impl IntoIterator<Item = &'a RequestRecord>) -> BTreeSet<u64> {
    records.into_iter().map(|r| r.conn_id).collect()
}

/// `2026-01-01T00:00:00.000Z` → Millisekunden seit 1970 (UTC).
fn ts_ms(ts: &str) -> i64 {
    let (date, time) = ts.split_once('T').expect("Zeitstempel mit T");
    let d: Vec<i64> = date.split('-').map(|x| x.parse().unwrap()).collect();
    let time = time.trim_end_matches('Z');
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let h: Vec<i64> = hms.split(':').map(|x| x.parse().unwrap()).collect();
    let ms: i64 = format!("{frac:0<3}")[..3].parse().unwrap();
    days_from_civil(d[0], d[1], d[2]) * 86_400_000 + ((h[0] * 60 + h[1]) * 60 + h[2]) * 1000 + ms
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Untere Schranke der gleichzeitig laufenden Seitenabrufe: je Verbindung (`per_conn = true`) oder
/// über alle Verbindungen. Nur Pfade mit Verzögerung (`/p/…`) zählen; zwei Eingänge mit weniger als
/// `delay_ms - 2` ms Abstand (Millisekunden-Auflösung des Logs) liefen sicher gleichzeitig.
fn overlap_lower_bound(
    records: &[RequestRecord],
    delay_ms: i64,
    per_conn: bool,
) -> BTreeMap<u64, usize> {
    let pages: Vec<(u64, i64)> = records
        .iter()
        .filter(|r| r.path().starts_with("/p/"))
        .map(|r| (if per_conn { r.conn_id } else { 0 }, ts_ms(&r.ts)))
        .collect();
    let mut out: BTreeMap<u64, usize> = BTreeMap::new();
    for &(conn, t) in &pages {
        let n = pages
            .iter()
            .filter(|&&(c, u)| c == conn && u <= t && t - u <= delay_ms - 2)
            .count();
        let e = out.entry(conn).or_default();
        *e = (*e).max(n);
    }
    out
}

// ---------------------------------------------------------------------------------------------------
// Gate: Annahmegrenze je Client-Adresse (`max_connections_per_client`)

#[derive(Debug, Clone)]
struct Rejection {
    /// Anfragen, die der Fixture-Server bis zu dieser Ablehnung gesehen hatte.
    requests_seen: usize,
}

#[derive(Default)]
struct GateStats {
    open: Mutex<HashMap<IpAddr, usize>>,
    accepted: Mutex<usize>,
    max_open: Mutex<usize>,
    rejected: Mutex<Vec<Rejection>>,
}

/// TCP-Weiterleitung vor dem Fixture-Server mit Annahmegrenze je Client-Adresse. Abgelehnte
/// Verbindungen werden unmittelbar nach `accept` geschlossen (für den Client: Verbindungs- bzw.
/// TLS-Fehler direkt nach dem Aufbau).
struct Gate {
    addr: SocketAddr,
    stats: Arc<GateStats>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Gate {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Gate {
    async fn start(site: Arc<TestSite>, max_connections_per_client: usize) -> Gate {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let upstream = site.addr();
        let stats = Arc::new(GateStats::default());
        let s = Arc::clone(&stats);
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut client, peer)) = listener.accept().await else {
                    continue;
                };
                let ip = peer.ip();
                {
                    let mut open = s.open.lock().unwrap();
                    let n = open.entry(ip).or_default();
                    if *n >= max_connections_per_client {
                        s.rejected.lock().unwrap().push(Rejection {
                            requests_seen: site.requests().len(),
                        });
                        drop(client);
                        continue;
                    }
                    *n += 1;
                    let total: usize = open.values().sum();
                    let mut max = s.max_open.lock().unwrap();
                    *max = (*max).max(total);
                    *s.accepted.lock().unwrap() += 1;
                }
                let s = Arc::clone(&s);
                tokio::spawn(async move {
                    if let Ok(mut up) = TcpStream::connect(upstream).await {
                        let _ = tokio::io::copy_bidirectional(&mut client, &mut up).await;
                    }
                    drop(client);
                    if let Some(n) = s.open.lock().unwrap().get_mut(&ip) {
                        *n = n.saturating_sub(1);
                    }
                });
            }
        });
        Gate { addr, stats, task }
    }

    fn rejected(&self) -> Vec<Rejection> {
        self.stats.rejected.lock().unwrap().clone()
    }

    fn accepted(&self) -> usize {
        *self.stats.accepted.lock().unwrap()
    }

    fn max_open(&self) -> usize {
        *self.stats.max_open.lock().unwrap()
    }
}

// ===================================================================================================
// Konfiguration: Schlüssel, Standard, Wertebereich

fn serialized(cfg: &CrawlConfig) -> Value {
    serde_json::to_value(cfg).unwrap()["speed"]["connections_per_host"].clone()
}

// spec: CRAWL-151
#[test]
fn the_key_defaults_to_auto_and_accepts_auto_and_one_to_six() {
    assert_eq!(
        serialized(&CrawlConfig::default()),
        json!("auto"),
        "Standard von {KEY} ist `auto`"
    );
    let mut cfg = CrawlConfig::default();
    for n in 1..=6u32 {
        cfg.set_value(KEY, &n.to_string())
            .unwrap_or_else(|e| panic!("{KEY} = {n}: {e}"));
        assert_eq!(serialized(&cfg), json!(n), "{KEY} = {n}");
    }
    cfg.set_value(KEY, "auto").unwrap();
    assert_eq!(serialized(&cfg), json!("auto"));

    // Laden aus einer .osconfig: `auto` und eine Zahl sind gültig, ohne Fehler auf dem Schlüssel.
    for text in [
        "[speed]\nconnections_per_host = \"auto\"\n",
        "[speed]\nconnections_per_host = 3\n",
    ] {
        let loaded = CrawlConfig::load_str(text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        assert!(
            !loaded.validate().errors.iter().any(|e| e.path == KEY),
            "{text:?}: {:?}",
            loaded.validate().errors
        );
        assert!(
            loaded.warnings.iter().all(|w| w.path != KEY),
            "{text:?}: unbekannter Schlüssel?"
        );
    }
}

// spec: CRAWL-151
#[test]
fn values_outside_one_to_six_are_rejected_and_leave_the_config_unchanged() {
    let mut cfg = CrawlConfig::default();
    cfg.set_value(KEY, "4").expect("4 ist gültig");
    for bad in ["0", "7", "-1", "12", "100", "abc", "", "2.5"] {
        let err = cfg.set_value(KEY, bad);
        assert!(err.is_err(), "{KEY} = {bad:?} muss abgelehnt werden");
        assert_eq!(
            serialized(&cfg),
            json!(4),
            "{bad:?}: Wert bleibt unverändert"
        );
    }
    // Aus einer Datei geladen: Ladefehler oder Validierungsfehler auf genau diesem Schlüssel.
    for text in [
        "[speed]\nconnections_per_host = 0\n",
        "[speed]\nconnections_per_host = 7\n",
        "[speed]\nconnections_per_host = \"viele\"\n",
    ] {
        match CrawlConfig::load_str(text) {
            Err(_) => {}
            Ok(loaded) => assert!(
                loaded.validate().errors.iter().any(|e| e.path == KEY),
                "{text:?} muss abgelehnt werden, Fehler: {:?}",
                loaded.validate().errors
            ),
        }
    }
}

// ===================================================================================================
// Akzeptanzkriterien

// spec: CRAWL-151
// Kriterium: HTTP/2, Threads 5, `auto`, 100 Seiten mit `delay_ms=200`: der Fixture-Server sieht
// 5 TCP-Verbindungen und nie mehr als 5 gleichzeitige Anfragen.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h2_five_threads_auto_opens_five_connections_and_never_more_than_five_requests() {
    let dir = site_100(200);
    let site = start_site(dir.path()).await;
    let c = crawl(&site, config(Proto::H2, 5, "auto"), Proto::H2).await;
    assert_complete(&c, "h2/5/auto");
    assert_http2(&c);
    assert_eq!(html_200(&c), 100, "alle 100 Seiten");
    assert_eq!(site.connections(), 5, "TCP-Verbindungen am Fixture-Server");
    let max = site.max_concurrency();
    assert!(
        max <= 5,
        "höchstens 5 gleichzeitige Anfragen, gemessen {max}"
    );
}

// spec: CRAWL-151
// Kriterium: HTTP/2, Threads 20, `auto`: höchstens 6 TCP-Verbindungen; keine Verbindung trägt mehr als
// 4 gleichzeitige Anfragen.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h2_twenty_threads_auto_uses_at_most_six_connections_with_at_most_four_requests_each() {
    let delay = 300;
    let dir = flat_site(160, delay);
    let site = start_site(dir.path()).await;
    let c = crawl(&site, config(Proto::H2, 20, "auto"), Proto::H2).await;
    assert_complete(&c, "h2/20/auto");
    assert_http2(&c);
    assert_eq!(html_200(&c), 161);
    let conns = site.connections();
    assert!(conns <= 6, "höchstens 6 TCP-Verbindungen, gemessen {conns}");

    let records = site.requests();
    let total = overlap_lower_bound(&records, delay as i64, false);
    let total = total.values().copied().max().unwrap_or(0);
    assert!(
        total >= 8,
        "Test braucht echte Last: mindestens 8 gleichzeitige Seitenabrufe, gemessen {total}"
    );
    let per_conn = overlap_lower_bound(&records, delay as i64, true);
    for (conn, n) in &per_conn {
        assert!(
            *n <= 4,
            "Verbindung {conn} trug mindestens {n} gleichzeitige Anfragen (höchstens 4); alle: {per_conn:?}"
        );
    }
}

// spec: CRAWL-151
// Kriterium: `speed.connections_per_host = 1`, HTTP/2, Threads 5: genau 1 TCP-Verbindung
// (bisheriges Verhalten: die Threads teilen sich die Verbindung).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn value_one_over_h2_uses_exactly_one_connection() {
    let dir = flat_site(40, 200);
    let site = start_site(dir.path()).await;
    let c = crawl(&site, config(Proto::H2, 5, "1"), Proto::H2).await;
    assert_complete(&c, "h2/5/1");
    assert_http2(&c);
    assert_eq!(html_200(&c), 41);
    assert_eq!(site.connections(), 1, "genau 1 TCP-Verbindung");
    let max = site.max_concurrency();
    assert!(
        (2..=5).contains(&max),
        "die Threads laufen weiter parallel über die eine Verbindung: gemessen {max}"
    );
}

// spec: CRAWL-151
// Kriterium: HTTP/1.1, Threads 10, Wert 2: nie mehr als 2 TCP-Verbindungen und nie mehr als
// 2 gleichzeitige Anfragen an den Host; wartende Anfragen warten statt zu scheitern.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h1_ten_threads_value_two_never_exceeds_two_connections_or_two_requests() {
    let dir = site_100(50);
    let site = start_site(dir.path()).await;
    let c = crawl(&site, config(Proto::H1, 10, "2"), Proto::H1).await;
    assert_complete(&c, "h1/10/2");
    assert_eq!(html_200(&c), 100, "alle Seiten trotz Wartens");
    assert_eq!(
        scalar(&c, "SELECT COUNT(*) FROM urls WHERE status_code = 0"),
        0
    );
    let conns = site.connections();
    assert!(conns <= 2, "höchstens 2 TCP-Verbindungen, gemessen {conns}");
    let max = site.max_concurrency();
    assert!(
        max <= 2,
        "höchstens 2 gleichzeitige Anfragen, gemessen {max}"
    );
}

// spec: CRAWL-151
// spec: CRAWL-011
// Kriterium: Threads 1: höchstens 2 TCP-Verbindungen für 20 Seiten (Kriterium aus CRAWL-011).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_thread_needs_at_most_two_connections_for_twenty_pages() {
    for proto in [Proto::H2, Proto::H1] {
        let dir = flat_site(20, 0);
        let site = start_site(dir.path()).await;
        let c = crawl(&site, config(proto, 1, "auto"), proto).await;
        assert_complete(&c, &format!("{proto:?}/1/auto"));
        assert_eq!(html_200(&c), 21, "{proto:?}");
        let conns = site.connections();
        assert!(
            conns <= 2,
            "{proto:?}: höchstens 2 TCP-Verbindungen, gemessen {conns}"
        );
    }
}

// spec: CRAWL-151
// Kriterium: Zwei Hosts im selben Crawl: die Grenze gilt je Host getrennt.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_hosts_in_one_crawl_each_get_their_own_limit() {
    let delay = 300;
    let dir = flat_site(40, delay);
    let site = start_site(dir.path()).await;
    // Listenmodus mit abwechselnden Hosts: beide Hosts sind während des ganzen Crawls beschäftigt.
    // Beide Namen lösen auf dieselbe Adresse auf; getrennt wird nach (Schema, Host, Port).
    let mut items = Vec::new();
    for n in 1..=40 {
        items.push(format!("https://a.site.test/p/{n}"));
        items.push(format!("https://b.site.test/p/{n}"));
    }
    let cfg = os_engine::list_input::list_config(config(Proto::H2, 10, "2"));
    let c = crawl_via(
        &site,
        site.addr(),
        cfg,
        CrawlSource::List {
            items,
            label: "zwei-hosts".to_string(),
        },
    )
    .await;
    assert_complete(&c, "zwei Hosts");
    assert_http2(&c);
    assert_eq!(html_200(&c), 80);

    let records = site.requests();
    let a = distinct_conns(records.iter().filter(|r| r.host == "a.site.test"));
    let b = distinct_conns(records.iter().filter(|r| r.host == "b.site.test"));
    assert!(
        !a.is_empty() && !b.is_empty(),
        "beide Hosts wurden abgerufen"
    );
    assert!(
        a.len() <= 2,
        "a.site.test: höchstens 2 Verbindungen, gemessen {a:?}"
    );
    assert!(
        b.len() <= 2,
        "b.site.test: höchstens 2 Verbindungen, gemessen {b:?}"
    );
    assert!(
        a.is_disjoint(&b),
        "keine Verbindung trägt Anfragen beider Hosts: a {a:?}, b {b:?}"
    );
    assert!(
        a.len() + b.len() >= 3,
        "die Grenze 2 gilt je Host, nicht für den Crawl: a {a:?}, b {b:?}"
    );
    let conns = site.connections();
    assert!(conns <= 4, "höchstens 2 je Host, zusammen {conns}");
}

// spec: CRAWL-151
// Kriterium: Server, der nur eine Verbindung je Client-Adresse annimmt (`max_connections_per_client = 1`):
// der Crawl läuft vollständig durch, keine URL erhält dadurch Statuscode 0, und nach der ersten Ablehnung
// folgt kein weiterer Verbindungsversuch über die eine Verbindung hinaus.
// Randfall (Verhalten 3): keine Fehlerzeile, keine Wiederholung (CRAWL-096/097), keine Host-Pause (CRAWL-149).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_server_accepting_one_connection_per_client_is_crawled_completely_without_error_rows() {
    for proto in [Proto::H2, Proto::H1] {
        let threads = 5u32;
        let pages = 30;
        let dir = flat_site(pages, 200);
        let site = start_site(dir.path()).await;
        let gate = Gate::start(Arc::clone(&site), 1).await;
        let c = crawl_via(
            &site,
            gate.addr,
            config(proto, threads, "auto"),
            spider(proto),
        )
        .await;
        let what = format!("{proto:?}");

        // vollständig, ohne Statuscode 0 und ohne Fehlerzeile
        assert_complete(&c, &what);
        if proto == Proto::H2 {
            assert_http2(&c);
        }
        assert_eq!(
            html_200(&c),
            pages as i64 + 1,
            "{what}: alle Seiten mit 200"
        );
        assert_eq!(
            scalar(
                &c,
                "SELECT COUNT(*) FROM urls WHERE status_code = 0 OR no_response_reason IS NOT NULL"
            ),
            0,
            "{what}: keine URL ohne Antwort"
        );
        // keine Wiederholung: jede Seite genau einmal am Server, kein gespeicherter Versuch > 1
        let records = site.requests();
        let mut per_path: BTreeMap<&str, usize> = BTreeMap::new();
        for r in records.iter().filter(|r| is_page(r)) {
            *per_path.entry(r.path()).or_default() += 1;
        }
        let repeated: Vec<_> = per_path.iter().filter(|(_, n)| **n > 1).collect();
        assert!(
            repeated.is_empty(),
            "{what}: mehrfach abgerufen: {repeated:?}"
        );
        assert_eq!(
            scalar(
                &c,
                "SELECT COUNT(*) FROM url_ext WHERE key = 'http.attempts' AND v_int > 1"
            ),
            0,
            "{what}: die Ablehnung zählt nicht als Wiederholung"
        );
        // keine Host-Pause (Ereignisse nach CRAWL-149)
        assert_eq!(
            scalar(
                &c,
                "SELECT COUNT(*) FROM crawl_events WHERE kind LIKE 'host_%' OR code LIKE 'host_%'"
            ),
            0,
            "{what}: keine Host-Pause"
        );

        // Verbindungen am Gate: nie mehr als eine offen; der Crawler hat (Verhalten 1) eine zweite versucht,
        // nach der ersten Ablehnung aber nicht weiter: weitere Ablehnungen nur aus demselben Anlauf, also
        // höchstens Threads − 1 und ohne dass der Server dazwischen mehr als `threads` Anfragen gesehen hat.
        assert_eq!(gate.max_open(), 1, "{what}");
        assert!(gate.accepted() >= 1, "{what}");
        let rejected = gate.rejected();
        assert!(
            !rejected.is_empty(),
            "{what}: der Crawler versucht bei ausgelasteter Verbindung eine weitere (Verhalten 1)"
        );
        assert!(
            rejected.len() < threads as usize,
            "{what}: {} Ablehnungen – nach der ersten Ablehnung kein weiterer Versuch",
            rejected.len()
        );
        let first = rejected[0].requests_seen;
        let last = rejected.last().unwrap().requests_seen;
        assert!(
            last - first <= threads as usize,
            "{what}: Verbindungsversuch noch nach {} weiteren Anfragen seit der ersten Ablehnung \
             ({rejected:?}, {} Anfragen insgesamt)",
            last - first,
            records.len()
        );
    }
}

/// `(url, status_code, hash_md5, size_bytes, content_class)`
type SnapshotRow = (String, Option<i64>, Option<String>, Option<i64>, String);

// spec: CRAWL-151
// Kriterium: Ergebnisgleichheit – derselbe Crawl mit Wert 1 und mit `auto` liefert dieselben URLs mit
// denselben Statuscodes und Inhalten.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn value_one_and_auto_give_the_same_urls_status_codes_and_contents() {
    let dir = site_100(30);
    let site = start_site(dir.path()).await;
    let snapshot = |c: &Crawl| -> Vec<SnapshotRow> {
        let mut stmt = c
            .file
            .conn()
            .prepare(
                "SELECT url, status_code, hash_md5, size_bytes, content_class FROM urls ORDER BY url",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    let one = crawl(&site, config(Proto::H2, 5, "1"), Proto::H2).await;
    let auto = crawl(&site, config(Proto::H2, 5, "auto"), Proto::H2).await;
    assert_complete(&one, "Wert 1");
    assert_complete(&auto, "auto");
    assert_http2(&one);
    assert_http2(&auto);
    assert_eq!(html_200(&one), 100);
    let (a, b) = (snapshot(&one), snapshot(&auto));
    assert!(!a.is_empty());
    assert_eq!(a, b, "Wert 1 und `auto` liefern dasselbe Ergebnis");
}

// spec: CRAWL-151
// spec: CRAWL-113
// Kriterium: Bei 2 URLs/s und `auto` bleibt die Rate am Fixture-Server bei höchstens 2 Anfragen je
// Sekunde – auch wenn dafür mehrere Verbindungen offen sind.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_urls_per_second_with_auto_stay_at_two_requests_per_second_at_the_server() {
    // Lange Antworten (1,5 s) bei 0,5 s Abstand: rund drei Anfragen laufen gleichzeitig, `auto`
    // öffnet also mehrere Verbindungen, die sich dieselbe Rate teilen müssen.
    let dir = flat_site(10, 1500);
    let site = start_site(dir.path()).await;
    let mut cfg = config(Proto::H2, 5, "auto");
    cfg.speed.max_urls_per_sec = Some(2.0);
    let c = crawl(&site, cfg, Proto::H2).await;
    assert_complete(&c, "2 URLs/s");
    assert_http2(&c);
    assert_eq!(html_200(&c), 11);
    assert!(
        site.connections() >= 2,
        "überlappende Anfragen unter `auto` laufen über mehrere Verbindungen, gemessen {}",
        site.connections()
    );
    // Eingänge am Server (alle Anfragen, auch robots.txt). Der erste baut die Verbindung auf und bleibt
    // außen vor. Token-Bucket ohne Burst: drei aufeinanderfolgende Anfänge brauchen 1 s; 200 ms Spielraum
    // für Laufzeitschwankungen zwischen Client und Server.
    let times: Vec<i64> = site
        .requests()
        .iter()
        .skip(1)
        .map(|r| ts_ms(&r.ts))
        .collect();
    assert!(times.len() >= 10, "{} Anfragen", times.len());
    for w in times.windows(3) {
        assert!(
            w[2] - w[0] >= 800,
            "drei Anfragen innerhalb von {} ms (höchstens 2 je Sekunde): {times:?}",
            w[2] - w[0]
        );
    }
}

// ===================================================================================================
// Randfälle

// spec: CRAWL-151
// Randfall: Wert 6 mit mehr als 6 Threads – höchstens 6 Verbindungen; bei ausreichend Last (Verhalten 1)
// genau 6. Unter HTTP/1.1 zusätzlich höchstens 6 gleichzeitige Anfragen.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn value_six_with_ten_threads_opens_six_connections_and_not_more() {
    for proto in [Proto::H2, Proto::H1] {
        let dir = flat_site(80, 300);
        let site = start_site(dir.path()).await;
        let c = crawl(&site, config(proto, 10, "6"), proto).await;
        let what = format!("{proto:?}/10/6");
        assert_complete(&c, &what);
        if proto == Proto::H2 {
            assert_http2(&c);
        }
        assert_eq!(html_200(&c), 81, "{what}");
        assert_eq!(site.connections(), 6, "{what}: TCP-Verbindungen");
        if proto == Proto::H1 {
            let max = site.max_concurrency();
            assert!(
                max <= 6,
                "{what}: höchstens 6 gleichzeitige Anfragen, gemessen {max}"
            );
        }
    }
}

// spec: CRAWL-151
// spec: CRAWL-126
// Randfall: Änderung während der Pause wirkt auf danach gestartete Anfragen. Vor der Pause mit Wert 4
// laufen die Anfragen über 2 bis 4 Verbindungen; nach dem Fortsetzen mit Wert 1 über genau eine neue.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_change_during_the_pause_applies_to_the_requests_started_afterwards() {
    let dir = flat_site(80, 300);
    let site = start_site(dir.path()).await;
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("crawl.ospider");

    let cfg = config(Proto::H2, 5, "4");
    let handle = begin(&site, site.addr(), cfg.clone(), spider(Proto::H2), &path).await;
    let mut waited = Duration::ZERO;
    while site
        .requests()
        .iter()
        .filter(|r| r.path().starts_with("/p/"))
        .count()
        < 10
    {
        assert!(
            waited < Duration::from_secs(120),
            "der Crawl erreicht 10 Seiten nicht"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
        waited += Duration::from_millis(20);
    }
    handle.pause();
    let paused = finish(handle).await;
    assert_eq!(paused.state, CrawlState::Paused, "Crawl ist pausiert");

    let before = site.requests();
    assert!(
        before
            .iter()
            .filter(|r| r.path().starts_with("/p/"))
            .count()
            < 80,
        "Pause vor dem Ende"
    );
    let conns_before = site.connections();
    let used_before = distinct_conns(&before);
    assert!(
        (2..=4).contains(&used_before.len()) && conns_before <= 4,
        "vor der Pause gilt Wert 4: {used_before:?}, {conns_before} TCP-Verbindungen"
    );

    // Änderung in der Pause: Wert 1, gespeichert in der Crawl-Datei
    let deps = deps();
    let mut changed = cfg.clone();
    set_connections(&mut changed, "1");
    changed.network.resolve = vec![format!("*.test={}", site.addr())];
    apply_paused_config(&path, &changed, &deps).expect("in der Pause änderbar");
    {
        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        assert_eq!(
            serialized(&file.config().unwrap()),
            json!(1),
            "Wert in der Datei"
        );
    }
    let f = fetcher(&changed, site.addr(), &site, &deps);
    let handle = resume_crawl(f, path.clone(), None, deps)
        .await
        .unwrap_or_else(|e| panic!("Fortsetzen: {e}"));
    let summary = finish(handle).await;
    assert_eq!(summary.state, CrawlState::Completed);
    assert_eq!(summary.end_reason, Some(EndReason::QueueEmpty));

    let after: Vec<RequestRecord> = site.requests().into_iter().skip(before.len()).collect();
    let pages_after = after.iter().filter(|r| r.path().starts_with("/p/")).count();
    assert!(
        pages_after >= 20,
        "nach dem Fortsetzen genug Last: {pages_after} Seiten"
    );
    let used_after = distinct_conns(&after);
    assert_eq!(
        used_after.len(),
        1,
        "nach dem Fortsetzen gilt Wert 1: {used_after:?}"
    );
    let opened_after = site.connections() - conns_before;
    assert!(
        opened_after <= 1,
        "nach dem Fortsetzen höchstens eine weitere TCP-Verbindung, gemessen {opened_after}"
    );
    let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let ok: i64 = file
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE content_class = 'HTML' AND status_code = 200",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ok, 81, "alle Seiten nach dem Fortsetzen");
}
