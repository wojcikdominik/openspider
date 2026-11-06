//! Tab „Response Codes“ (M1-P21): Spalten, Sichten All/Internal/External, Statusklassen, Weiterleitungsarten,
//! Ketten und Schleifen gegen echte Crawls der Fixture `onpage-rc` (Testsite, Hostnamen `*.test`).
//!
//! Die Adresslisten je Filter liegen als Goldens in `tests/golden/onpage-rc/response_codes__<filter>.csv`
//! (aus der Spec abgeleitet, sortiert nach Address, `{BASE}` = `http://site.test`, `{EXT}` =
//! `http://other.test`). Die Tests prüfen sie und zusätzlich die Akzeptanzkriterien als Einzelaussagen.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use os_analyze::tabs;
use os_analyze::views::{self, ColumnSelection, ViewQuery};
use os_analyze::SortKey;
use os_engine::testing::crawl_fixture_from;
use os_model::{CrawlConfig, RobotsMode, RobotsUnreachablePolicy, Value};

const BASE: &str = "http://site.test";
const EXT: &str = "http://other.test";
const TAB: &str = "Response Codes";

/// Alles, was die Tests nach einem Crawl brauchen (die Crawl-Datei selbst lebt nur im Crawl-Thread).
struct Dump {
    /// Filter-Kennung → Adressen (sortiert nach Address, mit `{BASE}`/`{EXT}`).
    filters: BTreeMap<String, Vec<String>>,
    /// Spalten der Ansicht „All“ mit allen Spalten.
    columns: Vec<String>,
    /// Zeilen der Ansicht „All“ mit allen Spalten, sortiert nach Address.
    rows: Vec<Vec<Value>>,
    /// Spalten der Standardansicht.
    default_columns: Vec<String>,
    /// Anzahl Hyperlinks interner Quellen je Ziel-Adresse (Gegenprobe für `Inlinks`).
    hyperlink_inlinks: BTreeMap<String, i64>,
    /// Abrufe an die Testsite: (Host, Pfad mit Query).
    requests: Vec<(String, String)>,
}

fn normalize(address: &str) -> String {
    address.replace(BASE, "{BASE}").replace(EXT, "{EXT}")
}

fn config() -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    // /timeout antwortet nach 3 s; ein Host ohne erreichbare robots.txt (nohost.invalid, geschlossener Port) soll
    // seinen Fehlerstatus zeigen und nicht als „robots.txt nicht erreichbar“ gesperrt werden (CRAWL-100).
    cfg.advanced.response_timeout_s = 1;
    // K2-64: Der Tab prüft Statuswerte; Retry und Drosselung haben eigene Engine-Tests.
    cfg.http.retries_network = 0;
    cfg.http.throttle_on_429 = false;
    cfg.robots.unreachable_policy = RobotsUnreachablePolicy::AllowAll;
    cfg.robots.retries = 0;
    cfg.robots.retry_delay_s = 0;
    cfg
}

fn text(v: &Value) -> Option<&str> {
    match v {
        Value::Text(s) => Some(s),
        _ => None,
    }
}

fn by_address() -> Vec<SortKey> {
    vec![SortKey {
        column: "Address".to_string(),
        descending: false,
    }]
}

/// Crawlt `start` und liest alle Filter des Tabs in einen [`Dump`].
fn crawl(start: &str, cfg: CrawlConfig) -> Dump {
    let start = start.to_string();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Runtime")
            .block_on(async move {
                let fx = crawl_fixture_from("onpage-rc", &start, cfg.clone()).await;
                let tab = tabs::response_codes::tab();
                let mut filters = BTreeMap::new();
                for f in &tab.filters {
                    let mut q = ViewQuery::new(TAB, f.id);
                    q.limit = 500;
                    q.sort = by_address();
                    q.columns = ColumnSelection::Named(vec!["Address".to_string()]);
                    let page = views::query_view(&fx.file, &cfg, &q).expect("Ansicht");
                    assert_eq!(page.total as usize, page.rows.len(), "{}", f.id);
                    let addresses = page
                        .rows
                        .iter()
                        .map(|r| normalize(text(&r[0]).expect("Address ist Text")))
                        .collect();
                    filters.insert(f.id.to_string(), addresses);
                }
                let mut q = ViewQuery::new(TAB, "All");
                q.limit = 500;
                q.sort = by_address();
                q.columns = ColumnSelection::All;
                let all = views::query_view(&fx.file, &cfg, &q).expect("Ansicht All");
                let default_columns =
                    views::query_view(&fx.file, &cfg, &ViewQuery::new(TAB, "All"))
                        .expect("Standardspalten")
                        .columns;

                let mut stmt = fx
                    .file
                    .conn()
                    .prepare(
                        "SELECT t.address, COUNT(*) FROM links l JOIN urls s ON s.id = l.source_id \
                         JOIN urls t ON t.id = l.target_id \
                         WHERE s.is_internal = 1 AND l.link_type = 'Hyperlink' GROUP BY t.address",
                    )
                    .unwrap();
                let hyperlink_inlinks = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                    .unwrap()
                    .map(|r| {
                        let (a, n) = r.unwrap();
                        (normalize(&a), n)
                    })
                    .collect();
                let requests = fx
                    .site
                    .requests()
                    .into_iter()
                    .map(|r| (r.host, r.path_and_query))
                    .collect();
                Dump {
                    filters,
                    columns: all.columns,
                    rows: all.rows,
                    default_columns,
                    hyperlink_inlinks,
                    requests,
                }
            })
    })
    .join()
    .expect("Crawl-Thread")
}

/// Der Standardcrawl der Startseite (einmal je Testlauf).
fn dump() -> &'static Dump {
    static DUMP: OnceLock<Dump> = OnceLock::new();
    DUMP.get_or_init(|| crawl("http://site.test/", config()))
}

fn golden_path(filter: &str) -> PathBuf {
    let slug: String = filter
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden/onpage-rc")
        .join(format!("response_codes__{slug}.csv"))
}

fn golden(filter: &str) -> Vec<String> {
    let path = golden_path(filter);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("Address"), "{}", path.display());
    lines.map(str::to_string).collect()
}

fn list(d: &Dump, filter: &str) -> Vec<String> {
    d.filters
        .get(filter)
        .unwrap_or_else(|| panic!("Filter {filter} fehlt"))
        .clone()
}

fn contains(d: &Dump, filter: &str, address: &str) -> bool {
    list(d, filter).iter().any(|a| a == address)
}

/// Wert einer Zelle der Ansicht „All“ (alle Spalten).
fn cell<'a>(d: &'a Dump, address: &str, column: &str) -> &'a Value {
    let col = d
        .columns
        .iter()
        .position(|c| c == column)
        .unwrap_or_else(|| panic!("Spalte {column} fehlt in {:?}", d.columns));
    let row = d
        .rows
        .iter()
        .find(|r| text(&r[0]).map(normalize).as_deref() == Some(address))
        .unwrap_or_else(|| panic!("Zeile {address} fehlt"));
    &row[col]
}

fn asserts_golden(d: &Dump, filter: &str) {
    assert_eq!(list(d, filter), golden(filter), "Filter {filter}");
}

/// Basisnamen der Statusklassen (ohne Sicht-Präfix) in Registry-Reihenfolge.
const CLASSES: [&str; 10] = [
    "Blocked by Robots.txt",
    "No Response",
    "Success (2XX)",
    "Redirection (3XX)",
    "Redirection (Meta Refresh)",
    "Redirect Chain",
    "Redirect Loop",
    "Client Error (4XX)",
    "Server Error (5XX)",
    "Other",
];

fn pick(d: &Dump, filters: &[&str]) {
    for f in filters {
        asserts_golden(d, f);
    }
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-050

// spec: ONPAGE-050
#[test]
fn columns_follow_the_spec_order_and_extras_come_last() {
    let d = dump();
    assert_eq!(
        d.default_columns,
        [
            "Address",
            "Content",
            "Status Code",
            "Status",
            "Indexability",
            "Indexability Status",
            "Inlinks",
            "Response Time",
            "Redirect URL",
            "Redirect Type"
        ]
    );
    // „Alle Spalten“: Referenzspalten zuerst (inkl. Matched Robots.txt Line), OS-Erweiterung dahinter.
    assert_eq!(
        &d.columns[10..],
        ["Matched Robots.txt Line", "Number of Redirects"]
    );
    let tab = tabs::response_codes::tab();
    let ext: Vec<_> = tab
        .columns
        .iter()
        .filter(|c| c.extension)
        .map(|c| c.key)
        .collect();
    assert_eq!(ext, ["Number of Redirects"]);
    for c in &tab.columns {
        if c.extension {
            assert!(!c.default_visible, "{}", c.key);
        }
    }
    use os_analyze::ColumnKind::{Text, Url};
    let kind = |key: &str| tab.columns.iter().find(|c| c.key == key).unwrap().kind;
    assert_eq!(kind("Address"), Url);
    assert_eq!(kind("Redirect URL"), Url);
    assert_eq!(kind("Status"), Text);
    assert_eq!(kind("Content"), Text);
}

// spec: ONPAGE-050
#[test]
fn a_301_has_an_absolute_redirect_url_and_type_a_200_has_neither() {
    let d = dump();
    let r301 = "{BASE}/r301";
    assert_eq!(
        text(cell(d, r301, "Redirect URL")),
        Some("http://site.test/ok")
    );
    assert_eq!(text(cell(d, r301, "Redirect Type")), Some("HTTP Redirect"));
    assert_eq!(cell(d, r301, "Status Code"), &Value::Int(301));
    assert_eq!(text(cell(d, r301, "Status")), Some("Moved Permanently"));
    let ok = "{BASE}/ok";
    assert_eq!(cell(d, ok, "Redirect URL"), &Value::Null);
    assert_eq!(cell(d, ok, "Redirect Type"), &Value::Null);
    assert_eq!(text(cell(d, ok, "Status")), Some("OK"));
    assert_eq!(
        text(cell(d, ok, "Content")),
        Some("text/html; charset=utf-8")
    );
    // Meta Refresh: Status 200, Art „Meta Refresh Redirect“
    let meta = "{BASE}/meta-refresh.html";
    assert_eq!(
        text(cell(d, meta, "Redirect Type")),
        Some("Meta Refresh Redirect")
    );
    assert_eq!(
        text(cell(d, meta, "Redirect URL")),
        Some("http://site.test/ok")
    );
}

// spec: ONPAGE-050
#[test]
fn inlinks_count_internal_hyperlink_instances_like_the_internal_tab() {
    let d = dump();
    assert_eq!(cell(d, "{BASE}/ok", "Inlinks"), &Value::Int(1));
    // Weiterleitungsziele ohne Hyperlink haben keine Inlinks (nur Hyperlink-Instanzen zählen)
    assert_eq!(cell(d, "{BASE}/chain2", "Inlinks"), &Value::Int(0));
    // Gegenprobe gegen die Linktabelle für jede Zeile
    for row in &d.rows {
        let address = normalize(text(&row[0]).unwrap());
        let col = d.columns.iter().position(|c| c == "Inlinks").unwrap();
        let expected = d.hyperlink_inlinks.get(&address).copied().unwrap_or(0);
        assert_eq!(row[col], Value::Int(expected), "Inlinks von {address}");
    }
    // externe Ziele zählen Links interner Quellen
    assert_eq!(cell(d, "{EXT}/e404", "Inlinks"), &Value::Int(1));
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-051

// spec: ONPAGE-051
#[test]
fn every_class_is_the_disjoint_union_of_internal_and_external() {
    let d = dump();
    let tab = tabs::response_codes::tab();
    let ids: Vec<_> = tab.filters.iter().map(|f| f.id).collect();
    // M1-Klassen plus HTTP Refresh (M2-D12) und explizitem externem JavaScript-Filter (A4).
    assert_eq!(ids.len(), 37);
    for class in CLASSES {
        let all = list(d, class);
        let internal = list(d, &format!("Internal {class}"));
        let external = list(d, &format!("External {class}"));
        for a in &internal {
            assert!(!external.contains(a), "{class}: {a} in beiden Sichten");
        }
        let mut union: Vec<String> = internal.iter().chain(&external).cloned().collect();
        union.sort_by_key(|a| a.replace("{BASE}", BASE).replace("{EXT}", EXT));
        assert_eq!(all, union, "{class}: All = Internal ∪ External");
    }
    let all = list(d, "All");
    assert_eq!(
        all.len(),
        list(d, "Internal All").len() + list(d, "External All").len()
    );
}

// spec: ONPAGE-051
#[test]
fn an_external_404_appears_in_the_external_view_only() {
    let d = dump();
    let url = "{EXT}/e404";
    assert!(contains(d, "External Client Error (4XX)", url));
    assert!(contains(d, "Client Error (4XX)", url));
    assert!(!contains(d, "Internal Client Error (4XX)", url));
    assert!(contains(d, "Internal Client Error (4XX)", "{BASE}/e404"));
    assert!(!contains(d, "External Client Error (4XX)", "{BASE}/e404"));
}

// spec: ONPAGE-051
#[test]
fn all_views_match_the_golden_lists() {
    let d = dump();
    pick(d, &["All", "Internal All", "External All"]);
    for class in CLASSES {
        pick(
            d,
            &[
                class,
                &format!("Internal {class}"),
                &format!("External {class}"),
            ],
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-052

fn robots_cfg(mode: RobotsMode) -> CrawlConfig {
    let mut cfg = config();
    cfg.robots.mode = mode;
    cfg
}

// spec: ONPAGE-052, CRAWL-101
#[test]
fn respect_mode_lists_blocked_urls_with_status_0_and_never_requests_them() {
    let d = dump();
    assert_eq!(
        list(d, "Blocked by Robots.txt"),
        golden("Blocked by Robots.txt")
    );
    let a = "{BASE}/blocked/a";
    assert_eq!(cell(d, a, "Status Code"), &Value::Int(0));
    assert_eq!(text(cell(d, a, "Status")), Some("Blocked by Robots.txt"));
    assert!(
        !d.requests
            .iter()
            .any(|(host, path)| host == "site.test" && path == "/blocked/a"),
        "Request-Log: /blocked/a darf nie angefragt werden"
    );
    // Matched Robots.txt Line: Zeilennummer und Regeltext
    assert_eq!(
        text(cell(d, a, "Matched Robots.txt Line")),
        Some("Zeile 2: Disallow: /blocked/")
    );
    // CRAWL-101: blockierte externe URLs sind standardmäßig ausgeblendet.
    assert!(!contains(
        d,
        "External Blocked by Robots.txt",
        "{EXT}/blocked/x"
    ));
    // Status 0 ohne robots.txt-Markierung ist „No Response“, nicht „Blocked“
    assert!(!contains(d, "No Response", a));
    assert!(!contains(d, "Success (2XX)", a));
}

// spec: ONPAGE-052
#[test]
fn ignore_mode_requests_the_url_and_keeps_it_out_of_the_filter() {
    let d = crawl(
        "http://site.test/blocked-start.html",
        robots_cfg(RobotsMode::Ignore),
    );
    let a = "{BASE}/blocked/a";
    assert!(list(&d, "Blocked by Robots.txt").is_empty());
    assert!(contains(&d, "Success (2XX)", a));
    assert_eq!(cell(&d, a, "Status Code"), &Value::Int(200));
    assert_eq!(cell(&d, a, "Matched Robots.txt Line"), &Value::Null);
    assert!(d
        .requests
        .iter()
        .any(|(host, path)| host == "site.test" && path == "/blocked/a"));
}

// spec: ONPAGE-052
#[test]
fn ignore_but_report_mode_requests_the_url_and_still_lists_it() {
    let d = crawl(
        "http://site.test/blocked-start.html",
        robots_cfg(RobotsMode::IgnoreButReport),
    );
    let a = "{BASE}/blocked/a";
    // A-17: angefragt (echter Status) und zusätzlich im Filter
    assert!(d
        .requests
        .iter()
        .any(|(host, path)| host == "site.test" && path == "/blocked/a"));
    assert_eq!(cell(&d, a, "Status Code"), &Value::Int(200));
    assert!(contains(&d, "Blocked by Robots.txt", a));
    assert!(contains(&d, "Internal Blocked by Robots.txt", a));
    assert!(contains(&d, "Success (2XX)", a));
    assert!(!contains(&d, "No Response", a));
    assert_eq!(
        text(cell(&d, a, "Matched Robots.txt Line")),
        Some("Zeile 2: Disallow: /blocked/")
    );
    // die erlaubte Seite wird nicht markiert
    assert!(!contains(&d, "Blocked by Robots.txt", "{BASE}/ok"));
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-054

// spec: ONPAGE-054
#[test]
fn no_response_lists_timeouts_refused_connections_and_dns_failures() {
    // `nohost.invalid` liegt in der reservierten TLD `.invalid` (RFC 6761, K-51): Der HostResolver von os-fetch
    // antwortet sofort mit "nicht auflösbar", ohne den System-Resolver zu fragen. Der Test braucht deshalb kein
    // Netz und ist unabhängig vom Resolver des Rechners deterministisch ("DNS Lookup Failed", nie
    // "Connection Timeout").
    let d = dump();
    assert_eq!(list(d, "No Response"), golden("No Response"));
    for (url, status) in [
        ("{BASE}/timeout", "Connection Timeout"),
        ("http://nohost.invalid/", "DNS Lookup Failed"),
        ("{BASE}/drop", "Connection Error"),
    ] {
        assert!(contains(d, "No Response", url), "{url}");
        assert_eq!(cell(d, url, "Status Code"), &Value::Int(0), "{url}");
        assert_eq!(text(cell(d, url, "Status")), Some(status), "{url}");
    }
    assert_eq!(
        list(d, "Internal No Response"),
        golden("Internal No Response")
    );
    assert_eq!(
        list(d, "External No Response"),
        golden("External No Response")
    );
}

// spec: ONPAGE-054
#[test]
fn a_closed_loopback_port_is_connection_refused() {
    // Eigener Teilcrawl mit einem frisch freigegebenen Loopback-Port (statt fest Port 1): Das Betriebssystem
    // lehnt die Verbindung ab. Windows beantwortet ein SYN auf einen geschlossenen Port nicht sofort mit RST,
    // sondern wiederholt es (~2 s); das Antwort-Timeout von 5 s lässt diesen Verbindungsaufbau nicht als
    // Timeout enden, deshalb gilt die strenge Prüfung auf allen Plattformen.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("freier Loopback-Port");
        listener.local_addr().expect("Adresse").port()
        // Listener fällt hier: der Port ist wieder geschlossen
    };
    let dir = tempfile::tempdir().expect("temporäres Verzeichnis");
    let refused = format!("http://127.0.0.1:{port}/");
    std::fs::write(
        dir.path().join("index.html"),
        format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>Refused</title></head>\
             <body><h1>Refused</h1><a href=\"{refused}\">geschlossener Port</a></body></html>"
        ),
    )
    .expect("index.html");
    let mut cfg = config();
    cfg.advanced.response_timeout_s = 5;
    let (addresses, status, code, crawl_time) = std::thread::spawn({
        let refused = refused.clone();
        let cfg = cfg.clone();
        let path = dir.path().to_path_buf();
        move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Runtime")
                .block_on(async move {
                    // Aufbau (Testsite, Abrufer, Crawl-Datei) zählt nicht zur Zeitgrenze: sie belegt nur, dass
                    // der abgelehnte Verbindungsaufbau im Crawl nicht auf das Antwort-Timeout wartet.
                    let site = os_testsite::TestSite::start_dir(&path, 0)
                        .await
                        .expect("Testsite");
                    let deps = os_engine::testing::test_deps();
                    let fetcher = os_engine::testing::site_fetcher(&site, &cfg, &deps);
                    let mut crawl_cfg = cfg.clone();
                    crawl_cfg
                        .network
                        .resolve
                        .push(format!("*.test={}", site.addr()));
                    let work = tempfile::tempdir().expect("Arbeitsverzeichnis");
                    let file_path = work.path().join("crawl.ospider");
                    let request = os_engine::CrawlRequest {
                        config: crawl_cfg,
                        source: os_engine::CrawlSource::Spider {
                            start_url: "http://site.test/".to_string(),
                        },
                        file: file_path.clone(),
                        project: "test".to_string(),
                        name: "closed-port".to_string(),
                    };
                    // Messung: unmittelbar vor dem Start des Crawls bis zu dessen Ende.
                    let started = std::time::Instant::now();
                    let handle = os_engine::start_crawl(fetcher, request, deps)
                        .await
                        .expect("Crawl startet");
                    handle.wait().await.expect("Crawl endet");
                    let crawl_time = started.elapsed();
                    let file = os_store::CrawlFile::open(&file_path, os_store::OpenMode::ReadWrite)
                        .expect("Crawl-Datei öffnen");
                    let mut q = ViewQuery::new(TAB, "External No Response");
                    q.limit = 50;
                    q.sort = by_address();
                    q.columns = ColumnSelection::Named(
                        ["Address", "Status Code", "Status"]
                            .map(str::to_string)
                            .to_vec(),
                    );
                    let page = views::query_view(&file, &cfg, &q).expect("Ansicht");
                    let addresses: Vec<String> = page
                        .rows
                        .iter()
                        .map(|r| text(&r[0]).expect("Address").to_string())
                        .collect();
                    let row = page
                        .rows
                        .iter()
                        .find(|r| text(&r[0]) == Some(refused.as_str()))
                        .expect("geschlossener Port in External No Response");
                    (
                        addresses,
                        text(&row[2]).map(str::to_string),
                        row[1].clone(),
                        crawl_time,
                    )
                })
        }
    })
    .join()
    .expect("Crawl-Thread");
    assert_eq!(addresses, [refused]);
    assert_eq!(code, Value::Int(0));
    assert_eq!(status.as_deref(), Some("Connection Refused"));
    assert!(
        crawl_time < std::time::Duration::from_secs(10),
        "kein Warten über 10 s: {crawl_time:?}"
    );
}

// spec: ONPAGE-054, CRAWL-101
#[test]
fn robots_blocked_urls_are_not_in_no_response() {
    let d = dump();
    for url in ["{BASE}/blocked/a", "{EXT}/blocked/x"] {
        assert!(!contains(d, "No Response", url), "{url}");
        assert_eq!(
            contains(d, "Blocked by Robots.txt", url),
            url == "{BASE}/blocked/a",
            "{url} (externe Sperren standardmäßig ausgeblendet)"
        );
    }
}

// spec: ONPAGE-054 [M2]
#[test]
#[ignore = "Klärung K2-32: P2: CRAWL-054 – Malformed für ungültige Links braucht die Option Crawl Invalid Links"]
fn invalid_links_are_reported_as_malformed_with_crawl_invalid_links() {
    // Link `htps://x` mit „Crawl Invalid Links“ an → Eintrag mit Status „Malformed“ in No Response; aus →
    // kein Eintrag. Die Option und der Eintrag entstehen erst mit CRAWL-054.
    unreachable!("zurückgestellt (M2)");
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-055

// spec: ONPAGE-055
#[test]
fn success_lists_200_and_204_but_not_redirects() {
    let d = dump();
    assert_eq!(list(d, "Success (2XX)"), golden("Success (2XX)"));
    assert!(contains(d, "Success (2XX)", "{BASE}/ok"));
    assert!(contains(d, "Success (2XX)", "{BASE}/s204"));
    assert!(!contains(d, "Success (2XX)", "{BASE}/r301"));
    assert_eq!(cell(d, "{BASE}/s204", "Status Code"), &Value::Int(204));
    assert_eq!(
        list(d, "Internal Success (2XX)"),
        golden("Internal Success (2XX)")
    );
    assert_eq!(
        list(d, "External Success (2XX)"),
        golden("External Success (2XX)")
    );
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-056

// spec: ONPAGE-056
#[test]
fn redirection_lists_all_3xx_with_and_without_location() {
    let d = dump();
    assert_eq!(list(d, "Redirection (3XX)"), golden("Redirection (3XX)"));
    for p in ["r301", "r302", "r307", "r308"] {
        let url = format!("{{BASE}}/{p}");
        assert!(contains(d, "Redirection (3XX)", &url), "{p}");
        assert_eq!(
            text(cell(d, &url, "Redirect URL")),
            Some("http://site.test/ok"),
            "{p}"
        );
        assert_eq!(
            text(cell(d, &url, "Redirect Type")),
            Some("HTTP Redirect"),
            "{p}"
        );
    }
    // 304 und 301 ohne Location: im Filter, Redirect URL leer
    for p in ["r304", "r301-noloc"] {
        let url = format!("{{BASE}}/{p}");
        assert!(contains(d, "Redirection (3XX)", &url), "{p}");
        assert_eq!(cell(d, &url, "Redirect URL"), &Value::Null, "{p}");
        assert_eq!(cell(d, &url, "Redirect Type"), &Value::Null, "{p}");
    }
    assert_eq!(
        list(d, "Internal Redirection (3XX)"),
        golden("Internal Redirection (3XX)")
    );
    assert_eq!(
        list(d, "External Redirection (3XX)"),
        golden("External Redirection (3XX)")
    );
    // Meta-Refresh-Seiten (Status 200) gehören nicht dazu
    assert!(!contains(
        d,
        "Redirection (3XX)",
        "{BASE}/meta-refresh.html"
    ));
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-058

// spec: ONPAGE-058
#[test]
fn meta_refresh_lists_pages_that_refresh_to_another_url() {
    let d = dump();
    assert_eq!(
        list(d, "Redirection (Meta Refresh)"),
        golden("Redirection (Meta Refresh)")
    );
    // content="0; url=/ok" und content="5;URL='/ok'" (beliebige Verzögerung, Anführungszeichen)
    for p in ["meta-refresh.html", "meta-refresh-delay.html"] {
        let url = format!("{{BASE}}/{p}");
        assert!(contains(d, "Redirection (Meta Refresh)", &url), "{p}");
        assert_eq!(
            text(cell(d, &url, "Redirect URL")),
            Some("http://site.test/ok"),
            "{p}"
        );
        assert_eq!(
            text(cell(d, &url, "Redirect Type")),
            Some("Meta Refresh Redirect"),
            "{p}"
        );
        // die Seite selbst ist ein 200
        assert!(contains(d, "Success (2XX)", &url), "{p}");
    }
}

// spec: ONPAGE-058
#[test]
fn meta_refresh_of_an_external_page_fills_the_external_filter() {
    // ONPAGE-058: „Interne/externe HTML-Seiten“. `http://other.test/meta-refresh.html` (extern, verlinkt von
    // der Startseite) leitet per Meta Refresh auf `/ok` desselben Hosts.
    let d = dump();
    let url = "{EXT}/meta-refresh.html".to_string();
    for filter in [
        "Redirection (Meta Refresh)",
        "External Redirection (Meta Refresh)",
    ] {
        assert!(contains(d, filter, &url), "{filter}");
    }
    assert!(!contains(d, "Internal Redirection (Meta Refresh)", &url));
    assert_eq!(
        text(cell(d, &url, "Redirect URL")),
        Some("http://other.test/ok")
    );
    assert_eq!(
        text(cell(d, &url, "Redirect Type")),
        Some("Meta Refresh Redirect")
    );
    // die Seite selbst ist ein 200; eine einzelne Weiterleitung ist keine Kette (K-44: extern nie Chain/Loop)
    assert!(contains(d, "External Success (2XX)", &url));
    assert!(!contains(d, "External Redirect Chain", &url));
    assert!(!contains(d, "Redirect Chain", &url));
    // das Ziel steht in den Tabellen und wurde genau einmal abgerufen (auch als Link-Ziel der Startseite)
    assert!(contains(d, "External Success (2XX)", "{EXT}/ok"));
    assert_eq!(
        d.requests
            .iter()
            .filter(|(h, p)| h == "other.test" && p == "/ok")
            .count(),
        1
    );
}

// spec: ONPAGE-058
#[test]
fn meta_refresh_without_target_or_to_itself_is_a_redirect_to_itself() {
    // K-57 (Paritätsbefund N-07): ein Selbst-Refresh ist eine Weiterleitung auf sich selbst, mit Status 200
    // (Success), Redirect URL = eigene Adresse, Redirect Type Meta Refresh Redirect und Redirect Loop.
    let d = dump();
    for p in ["meta-refresh-noloc.html", "meta-refresh-self.html"] {
        let url = format!("{{BASE}}/{p}");
        assert!(contains(d, "Redirection (Meta Refresh)", &url), "{p}");
        assert!(
            contains(d, "Internal Redirection (Meta Refresh)", &url),
            "{p}"
        );
        assert!(contains(d, "Redirect Loop", &url), "{p}");
        assert!(contains(d, "Success (2XX)", &url), "{p}");
        assert!(!contains(d, "Redirection (3XX)", &url), "{p}");
        assert_eq!(
            text(cell(d, &url, "Redirect URL")),
            Some(format!("http://site.test/{p}").as_str()),
            "{p}"
        );
        assert_eq!(
            text(cell(d, &url, "Redirect Type")),
            Some("Meta Refresh Redirect"),
            "{p}"
        );
        // genau ein Abruf: das Ziel ist die eigene Adresse
        let hits = d
            .requests
            .iter()
            .filter(|(host, path)| host == "site.test" && path == &format!("/{p}"))
            .count();
        assert_eq!(hits, 1, "{p}");
    }
}

// spec: ONPAGE-058
#[test]
fn meta_refresh_outside_head_is_detected_too() {
    let d = dump();
    assert!(contains(
        d,
        "Redirection (Meta Refresh)",
        "{BASE}/meta-refresh-body.html"
    ));
    assert!(contains(
        d,
        "Internal Redirection (Meta Refresh)",
        "{BASE}/meta-refresh-body.html"
    ));
    assert_eq!(
        list(d, "Internal Redirection (Meta Refresh)"),
        golden("Internal Redirection (Meta Refresh)")
    );
    assert_eq!(
        list(d, "External Redirection (Meta Refresh)"),
        golden("External Redirection (Meta Refresh)")
    );
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-060

// spec: ONPAGE-060
#[test]
fn a_two_hop_chain_lists_its_start_url_only() {
    let d = dump();
    assert_eq!(list(d, "Redirect Chain"), golden("Redirect Chain"));
    // /chain1 → /chain2 → /ok: /chain1 mit 2 Hops; /chain2 hat nur 1 Hop (A-19)
    assert!(contains(d, "Redirect Chain", "{BASE}/chain1"));
    assert!(!contains(d, "Redirect Chain", "{BASE}/chain2"));
    assert_eq!(
        cell(d, "{BASE}/chain1", "Number of Redirects"),
        &Value::Int(2)
    );
    assert_eq!(
        cell(d, "{BASE}/chain2", "Number of Redirects"),
        &Value::Int(1)
    );
    // 1 Hop (/r301 → /ok): nicht im Filter
    assert!(!contains(d, "Redirect Chain", "{BASE}/r301"));
    assert_eq!(
        cell(d, "{BASE}/r301", "Number of Redirects"),
        &Value::Int(1)
    );
    assert_eq!(cell(d, "{BASE}/ok", "Number of Redirects"), &Value::Int(0));
}

// spec: ONPAGE-060
#[test]
fn a_meta_refresh_before_a_301_makes_a_mixed_chain() {
    let d = dump();
    // /mixed-chain.html (Meta Refresh) → /r301 → /ok
    assert!(contains(d, "Redirect Chain", "{BASE}/mixed-chain.html"));
    assert!(contains(
        d,
        "Internal Redirect Chain",
        "{BASE}/mixed-chain.html"
    ));
    assert_eq!(
        cell(d, "{BASE}/mixed-chain.html", "Number of Redirects"),
        &Value::Int(2)
    );
    // eine einzelne Meta-Refresh-Weiterleitung ist keine Kette
    assert!(!contains(d, "Redirect Chain", "{BASE}/meta-refresh.html"));
}

// spec: ONPAGE-060
#[test]
fn chains_longer_than_the_redirect_limit_still_list_their_start_url() {
    let mut cfg = config();
    cfg.limits.max_redirects = 3;
    let d = crawl("http://site.test/long1", cfg);
    // Limit 3: /long1 … /long4 werden abgerufen; /long4 leitet auf das nicht mehr verfolgte /long5
    assert!(contains(&d, "Redirect Chain", "{BASE}/long1"));
    assert_eq!(
        cell(&d, "{BASE}/long1", "Number of Redirects"),
        &Value::Int(4)
    );
    assert!(contains(&d, "Redirect Chain", "{BASE}/long3"));
    assert!(!contains(&d, "Redirect Chain", "{BASE}/long4"));
}

// spec: ONPAGE-060
#[test]
fn external_chains_do_not_appear_in_the_filter() {
    let d = dump();
    // {EXT}/chain1 → {EXT}/chain2 → {EXT}/ok ist eine Kette, aber eine externe
    assert!(contains(d, "External Redirection (3XX)", "{EXT}/chain1"));
    assert!(!contains(d, "Redirect Chain", "{EXT}/chain1"));
    assert!(list(d, "External Redirect Chain").is_empty());
    assert_eq!(
        list(d, "Internal Redirect Chain"),
        golden("Internal Redirect Chain")
    );
    // Ketten, die in einer Schleife enden, zählen als Redirect Chain mit (K-66)
    for p in ["tail", "qloop", "loop1"] {
        let url = format!("{{BASE}}/{p}");
        assert!(contains(d, "Redirect Chain", &url), "{p}");
    }
}

// spec: ONPAGE-060
#[test]
fn chains_ending_in_a_loop_count_but_self_redirects_do_not() {
    let d = dump();
    // /loop1 → /loop2 → /loop1: beide mit je 2 Hops in Redirect Chain und Internal Redirect Chain und
    // zugleich in Redirect Loop (A-19 revidiert, K-66)
    for p in ["loop1", "loop2"] {
        let url = format!("{{BASE}}/{p}");
        assert!(contains(d, "Redirect Chain", &url), "{p} in Redirect Chain");
        assert!(
            contains(d, "Internal Redirect Chain", &url),
            "{p} in Internal Redirect Chain"
        );
        assert!(contains(d, "Redirect Loop", &url), "{p} in Redirect Loop");
    }
    assert_eq!(
        cell(d, "{BASE}/loop1", "Number of Redirects"),
        &Value::Int(2)
    );
    // /tail → /loop1 → …: Kette in die Schleife (A-19b); /qloop → /qloop?x=1 → /qloop?x=1
    assert!(contains(d, "Redirect Chain", "{BASE}/tail"));
    assert!(contains(d, "Redirect Chain", "{BASE}/qloop"));
    // Selbst-Weiterleitung und Selbst-Refresh haben 1 Hop: nur in Redirect Loop
    for url in ["{BASE}/self", "{BASE}/qloop?x=1"] {
        assert!(!contains(d, "Redirect Chain", url), "{url}");
        assert!(!contains(d, "Internal Redirect Chain", url), "{url}");
        assert!(contains(d, "Redirect Loop", url), "{url}");
    }
    assert!(!contains(
        d,
        "Redirect Chain",
        "{BASE}/meta-refresh-self.html"
    ));
    // Die Hop-Zahl bleibt unverändert
    assert_eq!(
        cell(d, "{BASE}/self", "Number of Redirects"),
        &Value::Int(1)
    );
    // Externe Sicht bleibt leer
    assert!(list(d, "External Redirect Chain").is_empty());
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-061

// spec: ONPAGE-061
#[test]
fn a_loop_lists_all_members_and_the_crawl_terminates() {
    // Der Crawl endete (sonst gäbe es kein Ergebnis); jede Schleifen-URL wurde genau einmal angefragt.
    let d = dump();
    assert_eq!(list(d, "Redirect Loop"), golden("Redirect Loop"));
    for p in ["loop1", "loop2"] {
        assert!(
            contains(d, "Redirect Loop", &format!("{{BASE}}/{p}")),
            "{p}"
        );
        let count = d
            .requests
            .iter()
            .filter(|(host, path)| host == "site.test" && path == &format!("/{p}"))
            .count();
        assert_eq!(count, 1, "{p} genau einmal angefragt");
    }
    assert_eq!(
        list(d, "Internal Redirect Loop"),
        golden("Internal Redirect Loop")
    );
    assert!(list(d, "External Redirect Loop").is_empty());
}

// spec: ONPAGE-061
#[test]
fn a_self_redirect_is_a_loop() {
    let d = dump();
    assert!(contains(d, "Redirect Loop", "{BASE}/self"));
    assert_eq!(
        text(cell(d, "{BASE}/self", "Redirect URL")),
        Some("http://site.test/self")
    );
}

// spec: ONPAGE-061
#[test]
fn a_loop_through_a_changed_query_is_detected_and_urls_leading_into_it_are_listed() {
    let d = dump();
    // /qloop → /qloop?x=1 → /qloop?x=1 (die Testsite-Route ignoriert die Query)
    assert!(contains(d, "Redirect Loop", "{BASE}/qloop?x=1"));
    // A-20: auch URLs, deren Kette in die Schleife führt
    assert!(contains(d, "Redirect Loop", "{BASE}/qloop"));
    assert!(contains(d, "Redirect Loop", "{BASE}/tail"));
    // Ketten ohne Schleife bleiben draußen
    assert!(!contains(d, "Redirect Loop", "{BASE}/chain1"));
    assert!(!contains(d, "Redirect Loop", "{BASE}/long1"));
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-062

// spec: ONPAGE-062
#[test]
fn client_errors_list_4xx_with_their_status_texts() {
    let d = dump();
    assert_eq!(list(d, "Client Error (4XX)"), golden("Client Error (4XX)"));
    for (code, text_) in [
        (400, "Bad Request"),
        (403, "Forbidden"),
        (404, "Not Found"),
        (410, "Gone"),
        (429, "Too Many Requests"),
    ] {
        let url = format!("{{BASE}}/e{code}");
        assert!(contains(d, "Client Error (4XX)", &url), "{code}");
        assert_eq!(cell(d, &url, "Status Code"), &Value::Int(code), "{code}");
        assert_eq!(text(cell(d, &url, "Status")), Some(text_), "{code}");
    }
    assert_eq!(
        list(d, "Internal Client Error (4XX)"),
        golden("Internal Client Error (4XX)")
    );
    assert_eq!(
        list(d, "External Client Error (4XX)"),
        golden("External Client Error (4XX)")
    );
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-063

// spec: ONPAGE-063
#[test]
fn server_errors_list_5xx() {
    let d = dump();
    assert_eq!(list(d, "Server Error (5XX)"), golden("Server Error (5XX)"));
    for code in [500, 503] {
        let url = format!("{{BASE}}/e{code}");
        assert!(contains(d, "Server Error (5XX)", &url), "{code}");
        assert_eq!(cell(d, &url, "Status Code"), &Value::Int(code), "{code}");
    }
    assert_eq!(
        list(d, "Internal Server Error (5XX)"),
        golden("Internal Server Error (5XX)")
    );
    assert_eq!(
        list(d, "External Server Error (5XX)"),
        golden("External Server Error (5XX)")
    );
}

// spec: ONPAGE-063 [M2]
#[test]
fn a_retry_that_returns_200_removes_the_url_from_the_filter() {
    // /flaky liefert erst 503, dann 200. Retries ausdrücklich aktivieren.
    let mut cfg = config();
    cfg.http.retries_5xx = 1;
    let d = crawl("http://site.test/flaky-start.html", cfg);
    assert!(!contains(&d, "Server Error (5XX)", "{BASE}/flaky"));
    assert!(contains(&d, "Success (2XX)", "{BASE}/flaky"));
}

// ---------------------------------------------------------------------------------------------------
// CRAWL-012 (Filter „Other“, OS-Erweiterung; die Spec-ID gehört zu P15)

#[test]
fn other_lists_only_codes_outside_100_to_599() {
    let d = dump();
    assert_eq!(list(d, "Other"), golden("Other"));
    assert_eq!(list(d, "Internal Other"), golden("Internal Other"));
    assert_eq!(list(d, "External Other"), golden("External Other"));
    // 999 steht nur in Other (und All), in keiner anderen Klasse
    for class in CLASSES.iter().filter(|c| **c != "Other") {
        assert!(!contains(d, class, "{BASE}/s999"), "{class}");
        assert!(!contains(d, class, "{EXT}/s999"), "{class}");
    }
    assert!(contains(d, "All", "{BASE}/s999"));
    assert_eq!(text(cell(d, "{BASE}/s999", "Status")), Some(""));
    // Die Klassen sind überschneidungsfrei bis auf Chain/Loop/Meta Refresh (Teilmengen von Success bzw.
    // Redirection) und die Markierung „Blocked“: Summe der Statusklassen = Zeilenzahl von All.
    let sum: usize = [
        "No Response",
        "Blocked by Robots.txt",
        "Success (2XX)",
        "Redirection (3XX)",
        "Client Error (4XX)",
        "Server Error (5XX)",
        "Other",
    ]
    .iter()
    .map(|c| list(d, c).len())
    .sum();
    // „No Response“ und „Blocked by Robots.txt“ sind im Respektieren-Modus disjunkt (Status 0)
    assert_eq!(sum, list(d, "All").len());
}

// ---------------------------------------------------------------------------------------------------
// Rahmen

// spec: ONPAGE-051
#[test]
fn filter_names_resolve_in_all_three_views() {
    for name in [
        "Response Codes:Internal Client Error (4XX)",
        "response codes:external redirect loop",
        "Response Codes:Redirect Chain",
    ] {
        let (tab, filter) = views::resolve_name(name).unwrap();
        assert_eq!(tab, TAB);
        assert!(tabs::response_codes::tab()
            .filters
            .iter()
            .any(|f| f.id == filter));
    }
}
