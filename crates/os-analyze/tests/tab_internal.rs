//! Tab „Internal“ (M1-P20): Spalten und Klassenfilter gegen echte Crawls der Fixtures `onpage-internal` und
//! `onpage-ctype`. Erwartungen sind aus der Spec (ONPAGE-010 … 024) abgeleitet; die Adresslisten stehen
//! unter `tests/golden/onpage-{internal,ctype}/`.

use std::path::PathBuf;

use os_analyze::tabs;
use os_analyze::views::{self, ColumnSelection, ViewQuery};
use os_analyze::SortKey;
use std::sync::Arc;

use os_engine::testing::{
    crawl_fixture, crawl_fixture_from, crawl_fixture_with, test_deps, FixtureCrawl,
};
use os_model::{CrawlConfig, StoreCrawl, SystemClock, Value};
use os_testsite::golden;

// ---------------------------------------------------------------------------------------------------
// Hilfen

/// Ergebnis einer Ansicht mit allen Spalten.
struct Table {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
    base: String,
}

fn query(
    fx: &FixtureCrawl,
    cfg: &CrawlConfig,
    tab: &str,
    filter: &str,
    cols: ColumnSelection,
) -> Table {
    let mut q = ViewQuery::new(tab, filter);
    q.columns = cols;
    q.limit = 500;
    let page = views::query_view(&fx.file, cfg, &q).expect("Ansicht");
    Table {
        columns: page.columns,
        rows: page.rows,
        base: fx.site.base_url(),
    }
}

fn all_columns(fx: &FixtureCrawl, filter: &str) -> Table {
    query(
        fx,
        &CrawlConfig::default(),
        "Internal",
        filter,
        ColumnSelection::All,
    )
}

impl Table {
    fn col(&self, name: &str) -> usize {
        self.columns
            .iter()
            .position(|c| c == name)
            .unwrap_or_else(|| panic!("Spalte {name:?} fehlt in {:?}", self.columns))
    }

    /// Adressen in Zeilenreihenfolge.
    fn addresses(&self) -> Vec<String> {
        let a = self.col("Address");
        self.rows
            .iter()
            .map(|r| match &r[a] {
                Value::Text(s) => s.clone(),
                other => panic!("Address ist kein Text: {other:?}"),
            })
            .collect()
    }

    /// Wert der Zelle in der Zeile mit der Adresse `base + path`.
    fn get(&self, path: &str, col: &str) -> Value {
        let want = format!("{}{path}", self.base);
        let a = self.col("Address");
        let c = self.col(col);
        let row = self
            .rows
            .iter()
            .find(|r| r[a] == Value::Text(want.clone()))
            .unwrap_or_else(|| panic!("keine Zeile für {want}; vorhanden: {:?}", self.addresses()));
        row[c].clone()
    }

    fn text(&self, path: &str, col: &str) -> String {
        match self.get(path, col) {
            Value::Text(s) => s,
            other => panic!("{path} / {col}: kein Text: {other:?}"),
        }
    }

    fn int(&self, path: &str, col: &str) -> i64 {
        match self.get(path, col) {
            Value::Int(i) => i,
            other => panic!("{path} / {col}: keine Ganzzahl: {other:?}"),
        }
    }

    fn float(&self, path: &str, col: &str) -> f64 {
        match self.get(path, col) {
            Value::Float(f) => f,
            other => panic!("{path} / {col}: keine Dezimalzahl: {other:?}"),
        }
    }

    /// Leere Zelle: `Null`, bei Mehrfachspalten ein leerer Text (nie `null`, TECH-004).
    fn is_empty_cell(&self, path: &str, col: &str) -> bool {
        match self.get(path, col) {
            Value::Null => true,
            Value::Text(s) => s.is_empty(),
            _ => false,
        }
    }

    fn contains(&self, path: &str) -> bool {
        self.addresses().contains(&format!("{}{path}", self.base))
    }
}

fn golden_path(scenario: &str, file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden")
        .join(scenario)
        .join(file)
}

/// Adressliste einer Ansicht (nach Address sortiert, Basis-URL als `{BASE}`) gegen die Golden-Datei.
fn assert_address_golden(fx: &FixtureCrawl, scenario: &str, tab: &str, filter: &str, file: &str) {
    let mut q = ViewQuery::new(tab, filter);
    q.columns = ColumnSelection::Named(vec!["Address".to_string()]);
    q.sort = vec![SortKey {
        column: "Address".to_string(),
        descending: false,
    }];
    q.limit = 500;
    let page = views::query_view(&fx.file, &CrawlConfig::default(), &q).expect("Ansicht");
    let mut text = String::from("Address\n");
    for row in &page.rows {
        match &row[0] {
            Value::Text(s) => {
                text.push_str(s);
                text.push('\n');
            }
            other => panic!("Address ist kein Text: {other:?}"),
        }
    }
    let bases = fx.site.golden_bases();
    let bases: Vec<&str> = bases.iter().map(String::as_str).collect();
    golden::assert_golden(
        golden_path(scenario, file),
        &golden::normalize(&text, &bases),
    );
}

async fn internal_site() -> FixtureCrawl {
    let mut cfg = CrawlConfig::default();
    cfg.extraction.http_headers = true;
    crawl_fixture("onpage-internal", cfg).await
}

// K2-64: Fehlerstatus prüfen, ohne produktive Timeouts und Wiederholungen abzuwarten.
async fn response_codes_site() -> FixtureCrawl {
    let mut cfg = CrawlConfig::default();
    cfg.advanced.response_timeout_s = 1;
    cfg.http.retries_network = 0;
    cfg.http.throttle_on_429 = false;
    cfg.robots.retries = 0;
    cfg.robots.retry_delay_s = 0;
    cfg.robots.unreachable_policy = os_model::RobotsUnreachablePolicy::AllowAll;
    crawl_fixture("onpage-rc", cfg).await
}

/// Crawl der Inhaltstypen-Fixture mit kurzem Antwort-Timeout (die Route `/timeout` antwortet erst nach 3 s).
async fn ctype_site() -> FixtureCrawl {
    let mut cfg = CrawlConfig::default();
    cfg.advanced.response_timeout_s = 1;
    // K2-64: Dieser Test prüft den Timeout-Status, nicht die separat getesteten Wiederholungen.
    cfg.http.retries_network = 0;
    crawl_fixture("onpage-ctype", cfg).await
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-010 Basisspalten

// spec: ONPAGE-010
#[test]
fn internal_tab_has_the_base_columns_first_in_spec_order() {
    let tab = tabs::internal::tab();
    let keys: Vec<&str> = tab.columns.iter().map(|c| c.key).collect();
    assert_eq!(
        &keys[..6],
        [
            "Address",
            "Content",
            "Status Code",
            "Status",
            "Indexability",
            "Indexability Status"
        ]
    );
    assert_eq!(tab.id, "Internal");
    assert!(
        tab.default_sort.is_empty(),
        "ohne Sortierung gilt die Entdeckungsreihenfolge"
    );
}

// spec: ONPAGE-010
#[tokio::test]
async fn status_code_status_and_content_follow_the_response() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");

    // 200-Seite: Status Code 200, Status „OK“, Content wie gesendet
    assert_eq!(t.int("/elements.html", "Status Code"), 200);
    assert_eq!(t.text("/elements.html", "Status"), "OK");
    assert_eq!(
        t.text("/elements.html", "Content"),
        "text/html; charset=utf-8"
    );

    // Reason-Phrase des Servers wird übernommen
    assert_eq!(t.int("/custom-reason", "Status Code"), 200);
    assert_eq!(t.text("/custom-reason", "Status"), "Alles gut");

    // Fehlt die Reason-Phrase (HTTP/2 ohne Phrase, hier: leere Phrase), gilt der IANA-Text zum Code
    assert_eq!(t.int("/empty-reason", "Status Code"), 404);
    assert_eq!(t.text("/empty-reason", "Status"), "Not Found");

    // robots.txt-blockierte URL: Status Code 0, Status „Blocked by Robots.txt“, Non-Indexable
    assert_eq!(t.int("/blocked/x.html", "Status Code"), 0);
    assert_eq!(t.text("/blocked/x.html", "Status"), "Blocked by Robots.txt");
    assert_eq!(t.text("/blocked/x.html", "Indexability"), "Non-Indexable");
    assert_eq!(
        t.text("/blocked/x.html", "Indexability Status"),
        "Blocked by Robots.txt"
    );

    // Antwort ohne Content-Type-Header: Spalte Content leer
    assert_eq!(t.int("/no-ct", "Status Code"), 200);
    assert!(t.is_empty_cell("/no-ct", "Content"));
}

// spec: ONPAGE-010
#[tokio::test]
async fn address_is_shown_decoded_and_the_requested_form_is_in_url_encoded_address() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    assert!(
        t.contains("/café"),
        "Anzeigeform dekodiert: {:?}",
        t.addresses()
    );
    assert!(!t.contains("/caf%C3%A9"));
    assert_eq!(
        t.text("/café", "URL Encoded Address"),
        format!("{}/caf%C3%A9", fx.site.base_url())
    );
}

// spec: ONPAGE-010 [M2]
#[tokio::test]
async fn http2_404_has_the_standard_status_text() {
    // Diese Fixture enthält keine HTTP/1.1-only-Routen (chunked/custom reason).
    let fx = crawl_fixture_from(
        "crawl-m2-http",
        "https://site.test/missing",
        CrawlConfig::default(),
    )
    .await;
    let t = query(
        &fx,
        &CrawlConfig::default(),
        "Internal",
        "All",
        ColumnSelection::All,
    );
    let row = &t.rows[0];
    assert_eq!(
        row[t.col("Address")],
        Value::Text("https://site.test/missing".into())
    );
    assert_eq!(row[t.col("HTTP Version")], Value::Text("2".into()));
    assert_eq!(row[t.col("Status Code")], Value::Int(404));
    assert_eq!(row[t.col("Status")], Value::Text("Not Found".into()));
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-011 Element-Spalten

// spec: ONPAGE-011
#[test]
fn second_occurrences_stand_at_the_end_of_the_column_list() {
    let tab = tabs::internal::tab();
    let keys: Vec<&str> = tab.columns.iter().map(|c| c.key).collect();
    let pos = |k: &str| {
        keys.iter()
            .position(|c| *c == k)
            .unwrap_or_else(|| panic!("{k}"))
    };
    // erste Vorkommen stehen vor Size/Hash …, zweite Vorkommen hinter Crawl Timestamp
    assert!(pos("Title 1") < pos("Size (bytes)"));
    assert!(pos("Crawl Timestamp") < pos("Title 2"));
    for second in [
        "Title 2",
        "Title 2 Length",
        "Title 2 Pixel Width",
        "Meta Description 2",
        "Meta Description 2 Length",
        "Meta Description 2 Pixel Width",
        "H1-2",
        "H1-2 Length",
        "H2-2",
        "H2-2 Length",
    ] {
        assert!(
            pos("Crawl Timestamp") < pos(second),
            "{second} gehört ans Ende"
        );
    }
    let first: Vec<&str> = keys[6..21].to_vec();
    assert_eq!(
        first,
        [
            "Title 1",
            "Title 1 Length",
            "Title 1 Pixel Width",
            "Meta Description 1",
            "Meta Description 1 Length",
            "Meta Description 1 Pixel Width",
            "Meta Keywords 1",
            "Meta Keywords 1 Length",
            "H1-1",
            "H1-1 Length",
            "H2-1",
            "H2-1 Length",
            "Meta Robots",
            "X-Robots-Tag",
            "Meta Refresh 1"
        ]
    );
}

// spec: ONPAGE-011
#[tokio::test]
async fn element_columns_carry_the_values_of_the_page() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    let p = "/elements.html";
    assert_eq!(t.text(p, "Title 1"), "Elements Title");
    assert_eq!(t.int(p, "Title 1 Length"), 14);
    assert!(t.int(p, "Title 1 Pixel Width") > 0);
    assert_eq!(t.text(p, "Meta Description 1"), "Elements description text");
    assert_eq!(t.int(p, "Meta Description 1 Length"), 25);
    assert_eq!(t.text(p, "Meta Keywords 1"), "alpha, beta");
    assert_eq!(t.int(p, "Meta Keywords 1 Length"), 11);
    assert_eq!(t.text(p, "H1-1"), "Elements H1");
    assert_eq!(t.int(p, "H1-1 Length"), 11);
    assert_eq!(t.text(p, "H2-1"), "Elements H2");
    assert_eq!(t.int(p, "H2-1 Length"), 11);
    // Meta Robots 1: content des ersten <meta name="robots">
    assert_eq!(t.text(p, "Meta Robots 1"), "noindex, follow");
    // Meta Refresh 1: content des ersten <meta http-equiv="refresh">
    assert_eq!(t.text(p, "Meta Refresh 1"), "5; url=/ziel");
    // Canonical Link Element 1: absolut aufgelöst
    assert_eq!(
        t.text(p, "Canonical Link Element 1"),
        format!("{}/elements.html", fx.site.base_url())
    );
    assert_eq!(
        t.text(p, "rel=\"next\" 1"),
        format!("{}/p2", fx.site.base_url())
    );
    assert_eq!(
        t.text(p, "rel=\"prev\" 1"),
        format!("{}/p0", fx.site.base_url())
    );
    // keine Header-Angaben auf dieser Seite
    assert!(t.is_empty_cell(p, "X-Robots-Tag 1"));
    assert!(t.is_empty_cell(p, "HTTP rel=\"next\" 1"));
    // keine zweiten Vorkommen
    assert!(t.is_empty_cell(p, "Title 2"));
    assert!(t.is_empty_cell(p, "H1-2"));
}

// spec: ONPAGE-011
#[tokio::test]
async fn header_values_fill_x_robots_tag_and_http_rel_columns() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    let p = "/elements-header";
    assert_eq!(t.text(p, "X-Robots-Tag 1"), "noindex");
    assert_eq!(
        t.text(p, "HTTP rel=\"next\" 1"),
        format!("{}/p2", fx.site.base_url())
    );
    assert_eq!(
        t.text(p, "HTTP rel=\"prev\" 1"),
        format!("{}/p0", fx.site.base_url())
    );
    // aus dem Header, nicht aus dem HTML
    assert!(t.is_empty_cell(p, "rel=\"next\" 1"));
    assert!(t.is_empty_cell(p, "Meta Robots 1"));
}

// spec: ONPAGE-011
#[tokio::test]
async fn second_occurrences_and_numbered_columns_follow_the_page() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    let p = "/two-of-each.html";
    assert_eq!(t.text(p, "Title 1"), "First Title");
    assert_eq!(t.text(p, "Title 2"), "Second Title");
    assert_eq!(t.int(p, "Title 2 Length"), 12);
    assert_eq!(t.text(p, "Meta Description 1"), "First description");
    assert_eq!(t.text(p, "Meta Description 2"), "Second description");
    assert_eq!(t.text(p, "H1-1"), "First H1");
    assert_eq!(t.text(p, "H1-2"), "Second H1");
    assert_eq!(t.int(p, "H1-2 Length"), 9);
    assert_eq!(t.text(p, "H2-1"), "First H2");
    assert_eq!(t.text(p, "H2-2"), "Second H2");
    // Mehrfachspalten: so viele nummerierte Spalten wie das Maximum der Ansicht (TECH-004)
    assert_eq!(t.text(p, "Meta Robots 1"), "noindex");
    assert_eq!(t.text(p, "Meta Robots 2"), "nofollow");
    assert!(t.columns.contains(&"Canonical Link Element 2".to_string()));
    assert_eq!(
        t.text(p, "Canonical Link Element 2"),
        format!("{}/elements.html", fx.site.base_url())
    );
    // eine Seite mit nur einem Vorkommen hat die zweite Spalte leer
    assert!(t.is_empty_cell("/elements.html", "Meta Robots 2"));
}

// spec: ONPAGE-011
#[tokio::test]
async fn non_html_urls_have_empty_element_columns_and_pdfs_use_document_properties() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    for col in [
        "Title 1",
        "Meta Description 1",
        "H1-1",
        "H2-1",
        "Meta Robots 1",
        "Canonical Link Element 1",
    ] {
        assert!(t.is_empty_cell("/chunked.pdf", col), "chunked.pdf / {col}");
    }
    // PDF: Title 1 und Meta Keywords 1 aus den Dokumenteigenschaften, alles Übrige leer
    assert_eq!(t.text("/doc.pdf", "Title 1"), "PDF Titel");
    assert_eq!(t.text("/doc.pdf", "Meta Keywords 1"), "pdf, schluessel");
    for col in [
        "Meta Description 1",
        "H1-1",
        "H2-1",
        "Meta Robots 1",
        "Meta Refresh 1",
        "Canonical Link Element 1",
    ] {
        assert!(t.is_empty_cell("/doc.pdf", col), "doc.pdf / {col}");
    }
}

// spec: ONPAGE-011, ONPAGE-009, ONPAGE-013
#[tokio::test]
async fn error_and_redirect_pages_have_empty_element_columns_even_with_a_body() {
    // K-60/N-01: /e400, /e404, /e500 liefern HTML mit <title> und <h1>, /r301 leitet um; nur interne
    // HTML-Seiten mit 2xx tragen Element-, Längen-, Pixel-, Word-Count- und Text-Ratio-Werte.
    let fx = response_codes_site().await;
    let t = all_columns(&fx, "All");
    for p in ["/e400", "/e404", "/e500", "/r301"] {
        if !t.contains(p) {
            continue;
        }
        for col in [
            "Title 1",
            "Title 1 Length",
            "Title 1 Pixel Width",
            "Meta Description 1",
            "Meta Description 1 Length",
            "Meta Description 1 Pixel Width",
            "Meta Keywords 1",
            "Meta Keywords 1 Length",
            "H1-1",
            "H1-1 Length",
            "H2-1",
            "H2-1 Length",
            "Word Count",
            "Text Ratio",
        ] {
            assert!(t.is_empty_cell(p, col), "{p} / {col}");
        }
    }
    assert!(t.contains("/e400") && t.contains("/e404") && t.contains("/e500"));
    // Gegenprobe: die 2xx-Seite /ok ist gefüllt
    assert_eq!(t.text("/ok", "Title 1"), "ok");
    assert_eq!(t.text("/ok", "H1-1"), "ok");
    assert_eq!(t.int("/ok", "Title 1 Length"), 2);
    assert!(t.int("/ok", "Title 1 Pixel Width") > 0);
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-012 Größen

// spec: ONPAGE-012
#[tokio::test]
async fn size_is_the_uncompressed_html_and_transferred_is_smaller_when_compressed() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");

    // 10 000-Byte-HTML-Seite
    assert_eq!(t.int("/size-10k.html", "Size (bytes)"), 10_000);
    assert_eq!(
        t.int("/size-10k.html", "Transferred (bytes)"),
        10_000,
        "unkomprimiert werden genau die Body-Bytes übertragen, der Kopf zählt nicht (K-02)"
    );

    // dieselbe Größe, gzip-komprimiert: Size bleibt, Transferred ist deutlich kleiner
    assert_eq!(t.int("/size-10k-gz", "Size (bytes)"), 10_000);
    let transferred = t.int("/size-10k-gz", "Transferred (bytes)");
    assert!(
        transferred > 0 && transferred < 10_000,
        "gzip: Transferred {transferred} < Size"
    );
}

// spec: ONPAGE-012
#[tokio::test]
async fn transferred_of_an_uncompressed_page_equals_size() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    assert_eq!(t.int("/size-10k.html", "Transferred (bytes)"), 10_000);
}

// spec: ONPAGE-012
#[tokio::test]
async fn size_of_a_chunked_pdf_without_content_length_is_zero() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    // A-6: andere Ressourcen = Content-Length, ohne Header 0 (der gespeicherte Body ist 36 Byte lang)
    assert_eq!(t.int("/chunked.pdf", "Size (bytes)"), 0);
    // die übrigen Spalten bleiben gefüllt
    assert!(t.int("/chunked.pdf", "Transferred (bytes)") > 0);
}

// spec: ONPAGE-012
#[tokio::test]
async fn size_of_a_pdf_with_content_length_is_the_header_value() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    let on_disk = std::fs::metadata(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/onpage-internal/doc.pdf"),
    )
    .unwrap()
    .len() as i64;
    assert_eq!(t.int("/doc.pdf", "Size (bytes)"), on_disk);
}

// spec: ONPAGE-012
#[tokio::test]
async fn total_transferred_equals_transferred_without_rendering() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    for path in [
        "/elements.html",
        "/size-10k.html",
        "/size-10k-gz",
        "/doc.pdf",
        "/r",
    ] {
        assert_eq!(
            t.get(path, "Total Transferred (bytes)"),
            t.get(path, "Transferred (bytes)"),
            "{path}: Textmodus (K-02)"
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-011 Gleichheit mit den Element-Tabs

/// Zeile der Tabelle mit der Adresse `address`, als Spaltenname → Wert.
fn row_by_address<'a>(t: &'a Table, address: &str) -> Option<&'a Vec<Value>> {
    let a = t.col("Address");
    t.rows
        .iter()
        .find(|r| r[a] == Value::Text(address.to_string()))
}

/// Länge und Pixelbreite zeigen die Element-Tabs bei fehlendem Element als 0, der Internal-Tab als
/// leere Zelle; für den Vergleich gilt beides als gleich.
fn element_value(column: &str, v: &Value) -> Value {
    let numeric = column.ends_with("Length") || column.ends_with("Pixel Width");
    match v {
        Value::Null if numeric => Value::Int(0),
        other => other.clone(),
    }
}

// spec: ONPAGE-011
#[tokio::test]
async fn internal_element_columns_equal_the_element_tabs_per_address() {
    let fx = internal_site().await;
    let cfg = CrawlConfig::default();
    let internal = all_columns(&fx, "All");

    // Element-Tab → Spalten, die der Internal-Tab unter demselben Namen führt
    let tabs: [(&str, &[&str]); 4] = [
        (
            "Page Titles",
            &[
                "Title 1",
                "Title 1 Length",
                "Title 1 Pixel Width",
                "Title 2",
                "Title 2 Length",
            ],
        ),
        (
            "Meta Description",
            &[
                "Meta Description 1",
                "Meta Description 1 Length",
                "Meta Description 1 Pixel Width",
                "Meta Description 2",
                "Meta Description 2 Length",
            ],
        ),
        ("H1", &["H1-1", "H1-1 Length", "H1-2", "H1-2 Length"]),
        ("H2", &["H2-1", "H2-1 Length", "H2-2", "H2-2 Length"]),
    ];

    for (tab, columns) in tabs {
        let element = query(&fx, &cfg, tab, "All", ColumnSelection::All);
        assert!(
            element.rows.len() >= 2,
            "{tab}: die Fixture liefert mindestens elements.html und two-of-each.html"
        );
        for row in &element.rows {
            let address = match &row[element.col("Address")] {
                Value::Text(s) => s.clone(),
                other => panic!("{tab}: Address ist kein Text: {other:?}"),
            };
            let internal_row = row_by_address(&internal, &address)
                .unwrap_or_else(|| panic!("{tab}: {address} fehlt im Internal-Tab"));
            for column in columns {
                assert_eq!(
                    element_value(column, &internal_row[internal.col(column)]),
                    element_value(column, &row[element.col(column)]),
                    "{tab} / {address} / {column}: Internal ≠ Element-Tab"
                );
            }
        }
    }

    // Die Gleichheit ist nicht leer: die Werte der beiden Fixture-Seiten stehen in beiden Tabs
    let titles = query(&fx, &cfg, "Page Titles", "All", ColumnSelection::All);
    let metas = query(&fx, &cfg, "Meta Description", "All", ColumnSelection::All);
    let h1s = query(&fx, &cfg, "H1", "All", ColumnSelection::All);
    let h2s = query(&fx, &cfg, "H2", "All", ColumnSelection::All);
    for tab in [&internal, &titles] {
        assert_eq!(tab.text("/elements.html", "Title 1"), "Elements Title");
        assert_eq!(tab.int("/elements.html", "Title 1 Length"), 14);
        assert_eq!(tab.text("/two-of-each.html", "Title 2"), "Second Title");
    }
    for tab in [&internal, &metas] {
        assert_eq!(
            tab.text("/elements.html", "Meta Description 1"),
            "Elements description text"
        );
        assert_eq!(tab.int("/elements.html", "Meta Description 1 Length"), 25);
    }
    for tab in [&internal, &h1s] {
        assert_eq!(tab.text("/elements.html", "H1-1"), "Elements H1");
        assert_eq!(tab.int("/elements.html", "H1-1 Length"), 11);
        assert_eq!(tab.text("/two-of-each.html", "H1-2"), "Second H1");
    }
    for tab in [&internal, &h2s] {
        assert_eq!(tab.text("/elements.html", "H2-1"), "Elements H2");
        assert_eq!(tab.int("/elements.html", "H2-1 Length"), 11);
        assert_eq!(tab.text("/two-of-each.html", "H2-2"), "Second H2");
    }
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-013 Word Count, Text Ratio

// spec: ONPAGE-013
#[tokio::test]
async fn word_count_counts_only_the_content_area() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    assert_eq!(t.int("/content/wordcount-3.html", "Word Count"), 3);
    // 10 Wörter im main, 50 im nav, 20 im footer → 10
    assert_eq!(t.int("/content/nav-footer.html", "Word Count"), 10);
    // Skript- und Style-Inhalt zählt nicht
    assert_eq!(t.int("/content/script-style.html", "Word Count"), 1);
}

// spec: ONPAGE-013
#[tokio::test]
async fn text_ratio_is_visible_text_over_source_in_percent_with_three_decimals() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    // K2-63 / SQ-08, ONPAGE-013: 8 / 271 × 100 = 2,952 (drei Nachkommastellen in der Ansicht).
    assert_eq!(t.int("/content/script-style.html", "Size (bytes)"), 271);
    assert!((t.float("/content/script-style.html", "Text Ratio") - 2.952).abs() < 1e-9);
    // Seite ohne Body-Text → 0
    assert!((t.float("/content/no-text.html", "Text Ratio")).abs() < 1e-9);
    assert_eq!(t.int("/content/no-text.html", "Word Count"), 0);
    // immer im Bereich [0, 100]
    let c = t.col("Text Ratio");
    for row in &t.rows {
        if let Value::Float(f) = row[c] {
            assert!((0.0..=100.0).contains(&f), "{f}");
        }
    }
}

// spec: ONPAGE-013
#[tokio::test]
async fn non_html_urls_have_no_word_count_and_no_text_ratio() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    for path in ["/chunked.pdf", "/doc.pdf", "/blocked/x.html", "/r"] {
        assert!(t.is_empty_cell(path, "Word Count"), "{path}");
        assert!(t.is_empty_cell(path, "Text Ratio"), "{path}");
    }
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-014 Crawl Depth, Folder Depth

// spec: ONPAGE-014
#[tokio::test]
async fn crawl_depth_counts_clicks_and_a_redirect_is_one_more_level() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    // Start → a → b
    assert_eq!(t.int("/", "Crawl Depth"), 0);
    assert_eq!(t.int("/depth/a.html", "Crawl Depth"), 1);
    assert_eq!(t.int("/depth/b.html", "Crawl Depth"), 2);
    // Start → /r (301) → /ziel
    assert_eq!(t.int("/r", "Crawl Depth"), 1);
    assert_eq!(t.int("/ziel", "Crawl Depth"), 2);
}

// spec: ONPAGE-014
#[tokio::test]
async fn folder_depth_counts_segments_followed_by_a_slash_and_ignores_the_query() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    assert_eq!(t.int("/", "Folder Depth"), 0);
    assert_eq!(t.int("/folders/", "Folder Depth"), 1); // /a/
    assert_eq!(t.int("/folders/b.html", "Folder Depth"), 1); // /a/b.html
    assert_eq!(t.int("/folders/b/", "Folder Depth"), 2); // /a/b/
    assert_eq!(t.int("/folders/b", "Folder Depth"), 1); // /a/b
    assert_eq!(t.int("/folders/?x=1", "Folder Depth"), 1); // /a/?x=1
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-015 Link-Zählspalten

async fn links_site() -> FixtureCrawl {
    crawl_fixture_from("onpage-internal", "/links/a.html", CrawlConfig::default()).await
}

// spec: ONPAGE-015
#[tokio::test]
async fn inlinks_count_every_instance_and_unique_inlinks_the_source_pages() {
    let fx = links_site().await;
    let t = all_columns(&fx, "All");
    // Z: A verlinkt dreimal, B einmal → 4 / 2
    assert_eq!(t.int("/links/z.html", "Inlinks"), 4);
    assert_eq!(t.int("/links/z.html", "Unique Inlinks"), 2);
    // Y: nur A
    assert_eq!(t.int("/links/y.html", "Inlinks"), 1);
    assert_eq!(t.int("/links/y.html", "Unique Inlinks"), 1);
}

// spec: ONPAGE-015
#[tokio::test]
async fn outlinks_count_internal_targets_and_external_outlinks_the_rest() {
    let fx = links_site().await;
    let t = all_columns(&fx, "All");
    // A: Z dreimal und Y einmal → 4 / 2; zwei Links auf dasselbe externe Ziel → 2 / 1
    assert_eq!(t.int("/links/a.html", "Outlinks"), 4);
    assert_eq!(t.int("/links/a.html", "Unique Outlinks"), 2);
    assert_eq!(t.int("/links/a.html", "External Outlinks"), 2);
    assert_eq!(t.int("/links/a.html", "Unique External Outlinks"), 1);
    // Ressourcen (img, link stylesheet) sind keine Hyperlinks
    assert_eq!(t.int("/links/pic.png", "Inlinks"), 0);
    assert_eq!(t.int("/links/style.css", "Inlinks"), 0);
}

// spec: ONPAGE-015
#[tokio::test]
async fn self_links_and_nofollow_links_are_counted() {
    let fx = links_site().await;
    let t = all_columns(&fx, "All");
    // B verlinkt sich selbst (A-9) und Y verlinkt B: Inlinks 2, Unique Inlinks 2 (Y und B)
    assert_eq!(t.int("/links/b.html", "Inlinks"), 2);
    assert_eq!(t.int("/links/b.html", "Unique Inlinks"), 2);
    // B: Z, B selbst und ein nofollow-Link auf N → 3 Outlinks, 3 verschiedene Ziele
    assert_eq!(t.int("/links/b.html", "Outlinks"), 3);
    assert_eq!(t.int("/links/b.html", "Unique Outlinks"), 3);
    // CRAWL-056: N bleibt als Beziehung/Zähler gespeichert, hat aber keine Tabellenzeile.
    assert!(!t.contains("/links/n.html"));
    let inlinks: i64 = fx
        .file
        .conn()
        .query_row(
            "SELECT c.inlinks FROM url_counters c JOIN urls u ON u.id=c.url_id WHERE u.url=?1",
            [format!("{}/links/n.html", fx.site.base_url())],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(inlinks, 1);
}

// spec: ONPAGE-015
#[tokio::test]
async fn pct_of_total_is_unique_html_200_inlinks_over_all_internal_html_200_pages() {
    let fx = links_site().await;
    let t = all_columns(&fx, "All");
    // interne 200-HTML-Seiten: A, Z, Y, B = 4 (N wird wegen nofollow nicht abgerufen)
    // Z: Quellen A und B → 2 von 4 = 50,00
    assert!((t.float("/links/z.html", "% of Total") - 50.0).abs() < 1e-9);
    // Y: nur A → 25,00; B: Y und B selbst → 50,00; A: keine → 0,00
    assert!((t.float("/links/y.html", "% of Total") - 25.0).abs() < 1e-9);
    assert!((t.float("/links/b.html", "% of Total") - 50.0).abs() < 1e-9);
    assert!(t.float("/links/a.html", "% of Total").abs() < 1e-9);
    // Ressourcen bekommen keinen Anteil
    assert!(t.float("/links/pic.png", "% of Total").abs() < 1e-9);
}

// spec: ONPAGE-015
#[test]
fn pct_of_total_counts_every_2xx_html_page_in_numerator_and_denominator() {
    // K-56 (Befund B-04): eine 201-Seite zählt als Quelle und im Nenner; 404 und 301 nicht.
    use os_model::time::{IdSource, SeededIds};
    use os_model::{CrawlMode, Timestamp};
    use os_store::{CrawlFile, CrawlInit, StorageProfile};
    let dir = tempfile::tempdir().unwrap();
    let created = Timestamp::from_db("2026-10-14T07:35:00.000Z").unwrap();
    let init = CrawlInit {
        crawl_id: SeededIds::new(3).new_crawl_id(created),
        mode: CrawlMode::Spider,
        start_url: Some("https://site.test/".into()),
        list_source: None,
        project: "Projekt".into(),
        name: "pct".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    let file = CrawlFile::create(
        &dir.path().join("p.ospider"),
        &init,
        StorageProfile::Balanced,
    )
    .unwrap();
    for (id, path, status) in [
        (1, "a", 200),
        (2, "b", 201),
        (3, "gone", 404),
        (4, "moved", 301),
    ] {
        file.conn()
            .execute(
                "INSERT INTO urls (id, url, address, origin, is_internal, in_tables, state, discovered_via, \
                 folder_depth, url_length, status_code, content_class) \
                 VALUES (?1, ?2, ?2, 'https://site.test', 1, 1, 'done', 'link', 0, 20, ?3, 'HTML')",
                rusqlite::params![id, format!("https://site.test/{path}"), status],
            )
            .unwrap();
    }
    // a und b verlinken beide auf a; die 404-Seite verlinkt ebenfalls (zählt nicht als Quelle)
    for (n, source) in [1, 2, 3].into_iter().enumerate() {
        file.conn()
            .execute(
                "INSERT INTO links (source_id, target_id, link_type, position, target_raw, follow) \
                 VALUES (?1, 1, 'Hyperlink', ?2, '/a', 1)",
                rusqlite::params![source, n as i64],
            )
            .unwrap();
    }
    // Die Zeilen wurden direkt eingefügt: die Zähler (Schema v2, `url_counters`) aus den Links berechnen.
    let ids: Vec<i64> = (1..=4).collect();
    for (id, c) in os_store::counters::recompute_many(file.conn(), &ids).unwrap() {
        file.conn()
            .execute(
                "UPDATE url_counters SET html_2xx_sources = ?1, inlinks = ?2 WHERE url_id = ?3",
                rusqlite::params![c.html_2xx_sources, c.inlinks, id],
            )
            .unwrap();
    }
    let mut q = ViewQuery::new("Internal", "All");
    q.columns = ColumnSelection::Named(vec!["Address".into(), "% of Total".into()]);
    q.limit = 500;
    let page = views::query_view(&file, &CrawlConfig::default(), &q).unwrap();
    let pct = page
        .rows
        .iter()
        .find(|r| matches!(&r[0], Value::Text(a) if a == "https://site.test/a"))
        .map(|r| match &r[1] {
            Value::Float(f) => *f,
            other => panic!("{other:?}"),
        })
        .unwrap();
    // Quellen 1 und 2 von 2 HTML-2xx-Seiten = 100 %
    assert!((pct - 100.0).abs() < 1e-9, "{pct}");
}

// spec: ONPAGE-015
#[tokio::test]
async fn pct_of_total_is_rounded_to_two_decimals() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    let tab = tabs::internal::tab();
    let col = tab.columns.iter().find(|c| c.key == "% of Total").unwrap();
    assert!(matches!(
        col.kind,
        os_analyze::ColumnKind::Float { decimals: 2 }
    ));
    // Anzahl der internen HTML-Seiten mit Status 200 aus der Ansicht selbst
    let class = |ct: &Value| matches!(ct, Value::Text(s) if s.starts_with("text/html"));
    let (c_ct, c_code) = (t.col("Content"), t.col("Status Code"));
    let pages = t
        .rows
        .iter()
        .filter(|r| class(&r[c_ct]) && r[c_code] == Value::Int(200))
        .count() as f64;
    // /custom-reason wird nur von der Startseite verlinkt: 1 von N, auf zwei Stellen gerundet
    let expected = (100.0 / pages * 100.0).round() / 100.0;
    assert!(
        (t.float("/custom-reason", "% of Total") - expected).abs() < 1e-9,
        "{} von {pages}",
        t.float("/custom-reason", "% of Total")
    );
    // nicht ganzzahlig, aber mit höchstens zwei Nachkommastellen
    let v = t.float("/custom-reason", "% of Total");
    assert!((v * 100.0 - (v * 100.0).round()).abs() < 1e-6, "{v}");
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-018 Technische Spalten

// spec: ONPAGE-018
#[tokio::test]
async fn hash_is_md5_of_the_decoded_body() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    let h1 = t.text("/tech/same1.html", "Hash");
    assert_eq!(h1.len(), 32);
    assert!(h1
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    // byte-identische Seiten: gleicher Hash; ein zusätzliches Leerzeichen ändert ihn
    assert_eq!(h1, t.text("/tech/same2.html", "Hash"));
    assert_ne!(h1, t.text("/tech/same3.html", "Hash"));
    // gzip-ausgeliefert = Hash der unkomprimierten Seite
    assert_eq!(h1, t.text("/tech/gz-same1.html", "Hash"));
}

// spec: ONPAGE-018
#[tokio::test]
async fn hash_is_empty_for_non_html_responses_and_redirects() {
    // K-60/N-06: PDF, Weiterleitung (leerer Körper) und gesperrte URL haben keinen Hash; HTML schon.
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    for path in ["/doc.pdf", "/chunked.pdf", "/r", "/blocked/x.html"] {
        assert!(t.is_empty_cell(path, "Hash"), "{path}");
    }
    // Fehlerseiten mit HTML-Körper: kein Hash (die Referenz schreibt ihn nur für 2xx)
    let rc = response_codes_site().await;
    let rt = all_columns(&rc, "All");
    for path in ["/e400", "/e404", "/e500", "/s999"] {
        assert!(rt.is_empty_cell(path, "Hash"), "{path}");
    }
    assert_eq!(rt.text("/ok", "Hash").len(), 32);
    // 204 ohne Content-Type: Hash des leeren Körpers
    assert_eq!(rt.text("/s204", "Hash"), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(t.text("/elements.html", "Hash").len(), 32);
}

// spec: ONPAGE-018
#[tokio::test]
async fn response_time_is_in_seconds_with_three_decimals() {
    // echte Uhr statt FixedClock: die Route /slow wartet 500 ms
    let mut deps = test_deps();
    deps.clock = Arc::new(SystemClock);
    let fx = crawl_fixture_with("onpage-internal", "/slow", CrawlConfig::default(), deps).await;
    let t = all_columns(&fx, "All");
    let slow = t.float("/slow", "Response Time");
    assert!(slow >= 0.5, "Response Time {slow}");
    assert!(slow < 5.0, "Response Time {slow}");
    assert!(
        (slow * 1000.0 - (slow * 1000.0).round()).abs() < 1e-6,
        "drei Nachkommastellen: {slow}"
    );
}

// spec: ONPAGE-018
#[tokio::test]
async fn last_modified_is_the_raw_header_and_empty_without_it() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    assert_eq!(
        t.text("/lm", "Last-Modified"),
        "Wed, 21 Oct 2026 07:28:00 GMT"
    );
    // fehlt der Header, ist die Zelle leer (nicht „0“ oder 1970)
    assert!(t.is_empty_cell("/no-lm", "Last-Modified"));
}

// spec: ONPAGE-018
#[tokio::test]
async fn redirect_url_is_absolute_and_redirect_type_has_the_tab_label() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    let ziel = format!("{}/ziel", fx.site.base_url());
    // relative Location → absolut
    assert_eq!(t.text("/rel-redirect", "Redirect URL"), ziel);
    assert_eq!(t.text("/rel-redirect", "Redirect Type"), "HTTP Redirect");
    assert_eq!(t.int("/rel-redirect", "Status Code"), 302);
    assert_eq!(t.text("/r", "Redirect URL"), ziel);
    // Meta-Refresh-Ziel
    assert_eq!(t.text("/elements.html", "Redirect URL"), ziel);
    assert_eq!(
        t.text("/elements.html", "Redirect Type"),
        "Meta Refresh Redirect"
    );
    // Refresh-Header: eigene Ausprägung (A-10)
    assert_eq!(
        t.text("/http-refresh", "Redirect Type"),
        "HTTP Refresh Redirect"
    );
    // Seiten ohne Weiterleitung
    assert!(t.is_empty_cell("/lm", "Redirect URL"));
    assert!(t.is_empty_cell("/lm", "Redirect Type"));
}

// spec: ONPAGE-018
#[tokio::test]
async fn http_version_url_encoded_address_and_crawl_timestamp() {
    let fx = internal_site().await;
    let t = all_columns(&fx, "All");
    assert_eq!(t.text("/lm", "HTTP Version"), "1.1");
    // Pfad /straße → URL Encoded Address endet auf /stra%C3%9Fe
    let encoded = t.text("/straße", "URL Encoded Address");
    assert!(encoded.ends_with("/stra%C3%9Fe"), "{encoded}");
    // Zeitpunkt der Anfrage als RFC-3339-Text (Umrechnung in die lokale Zone beim Export, K-32)
    let ts = t.text("/lm", "Crawl Timestamp");
    assert!(ts.starts_with("2026-01-01T00:00:00"), "{ts}");
    assert!(ts.ends_with('Z'), "{ts}");
}

// ---------------------------------------------------------------------------------------------------
// Population und Goldens des Szenarios onpage-internal

// spec: ONPAGE-010, CRAWL-098
#[tokio::test]
async fn internal_all_lists_the_internal_urls_in_tables_and_no_external_ones() {
    let fx = internal_site().await;
    assert_address_golden(
        &fx,
        "onpage-internal",
        "Internal",
        "All",
        "internal__all.csv",
    );
    let t = all_columns(&fx, "All");
    // die robots.txt des internen Hosts hat eine URL-Zeile, erscheint aber in keinem Tab (K-54, CRAWL-098)
    assert!(!t.contains("/robots.txt"));
    let robots: i64 = fx
        .file
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE discovered_via = 'robots_txt' AND in_tables = 0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(robots, 1);
}

// spec: ONPAGE-021, ONPAGE-022, ONPAGE-024
#[tokio::test]
async fn class_filters_of_the_internal_scenario_match_the_golden_lists() {
    let fx = internal_site().await;
    assert_address_golden(
        &fx,
        "onpage-internal",
        "Internal",
        "HTML",
        "internal__html.csv",
    );
    assert_address_golden(
        &fx,
        "onpage-internal",
        "Internal",
        "PDF",
        "internal__pdf.csv",
    );
    assert_address_golden(
        &fx,
        "onpage-internal",
        "Internal",
        "Unknown",
        "internal__unknown.csv",
    );
    assert_address_golden(
        &fx,
        "onpage-internal",
        "Internal",
        "JavaScript",
        "internal__javascript.csv",
    );
    assert_address_golden(
        &fx,
        "onpage-internal",
        "Internal",
        "CSS",
        "internal__css.csv",
    );
    assert_address_golden(
        &fx,
        "onpage-internal",
        "Internal",
        "Images",
        "internal__images.csv",
    );
}

// ---------------------------------------------------------------------------------------------------
// ONPAGE-021 … ONPAGE-024 Klassenfilter (Fixture onpage-ctype)

fn filter_addresses(fx: &FixtureCrawl, filter: &str) -> Vec<String> {
    let t = query(
        fx,
        &CrawlConfig::default(),
        "Internal",
        filter,
        ColumnSelection::Named(vec!["Address".into()]),
    );
    let base = fx.site.base_url();
    t.addresses()
        .into_iter()
        .map(|a| a.strip_prefix(&base).unwrap_or(&a).to_string())
        .collect()
}

// spec: ONPAGE-021
#[tokio::test]
async fn html_filter_has_every_html_class_url_regardless_of_status() {
    let fx = ctype_site().await;
    let html = filter_addresses(&fx, "HTML");
    for want in ["/t-html", "/t-xhtml", "/HTML-upper"] {
        assert!(html.contains(&want.to_string()), "{want} fehlt: {html:?}");
    }
    assert!(!html.contains(&"/t-xml".to_string()));
    // interne 404-Seite mit text/html: Statuscode egal
    assert!(html.contains(&"/gone".to_string()));
    let all = query(
        &fx,
        &CrawlConfig::default(),
        "Internal",
        "All",
        ColumnSelection::All,
    );
    assert_eq!(all.int("/gone", "Status Code"), 404);
    // 301-Antwort mit text/html-Body
    assert!(html.contains(&"/moved".to_string()));
    assert_eq!(all.int("/moved", "Status Code"), 301);
    // Content-Type mit Parametern und Großschreibung ändert die Klasse nicht
    assert!(html.contains(&"/HTML-upper".to_string()));
    // text/plain mit HTML-Body ist keine HTML-Seite
    assert!(!html.contains(&"/falscher-ct".to_string()));
    assert_address_golden(
        &fx,
        "onpage-ctype",
        "Internal",
        "HTML",
        "internal__html.csv",
    );
}

// spec: ONPAGE-022
#[tokio::test]
async fn javascript_css_images_and_pdf_filters_follow_the_content_type() {
    let fx = ctype_site().await;
    let js = filter_addresses(&fx, "JavaScript");
    assert!(js.contains(&"/t-js".to_string()));
    assert!(js.contains(&"/t-js-text".to_string()), "text/javascript");
    assert_eq!(js.len(), 7, "alle sieben JavaScript-MIME-Typen: {js:?}");
    assert_eq!(filter_addresses(&fx, "CSS"), ["/t-css"]);
    let images = filter_addresses(&fx, "Images");
    for want in ["/t-png", "/t-svg", "/t-webp", "/t-avif"] {
        assert!(images.contains(&want.to_string()), "{want}: {images:?}");
    }
    assert_eq!(
        images.len(),
        4,
        "image/svg+xml ist ein Bild, nicht XML: {images:?}"
    );
    assert_eq!(filter_addresses(&fx, "PDF"), ["/t-pdf"]);
    for (filter, file) in [
        ("JavaScript", "internal__javascript.csv"),
        ("CSS", "internal__css.csv"),
        ("Images", "internal__images.csv"),
        ("PDF", "internal__pdf.csv"),
    ] {
        assert_address_golden(&fx, "onpage-ctype", "Internal", filter, file);
    }
}

// spec: ONPAGE-022
#[tokio::test]
async fn images_filter_is_empty_when_images_are_not_stored() {
    // Seite mit zwei <img>: mit Standard-Konfiguration stehen beide Bilder im Filter …
    let fx = crawl_fixture_from("onpage-ctype", "/images-only.html", CrawlConfig::default()).await;
    assert_eq!(filter_addresses(&fx, "Images"), ["/pic.png", "/pic.svg"]);
    // … ist „Bilder speichern“ aus, ist der Filter leer, obwohl <img> vorkommen
    let mut cfg = CrawlConfig::default();
    cfg.spider.images = StoreCrawl::OFF;
    let fx = crawl_fixture_from("onpage-ctype", "/images-only.html", cfg).await;
    assert!(filter_addresses(&fx, "Images").is_empty());
}

// spec: ONPAGE-024
#[tokio::test]
async fn unknown_filter_has_missing_invalid_blocked_and_unanswered_urls() {
    let fx = ctype_site().await;
    let unknown = filter_addresses(&fx, "Unknown");
    for want in ["/kein-ct", "/t-invalid", "/blocked/x", "/timeout"] {
        assert!(
            unknown.contains(&want.to_string()),
            "{want} fehlt: {unknown:?}"
        );
    }
    // keine URL mit gültigem Content-Type
    let all = query(
        &fx,
        &CrawlConfig::default(),
        "Internal",
        "All",
        ColumnSelection::All,
    );
    for path in &unknown {
        match all.get(path, "Content") {
            Value::Null => {}
            Value::Text(ct) => assert_eq!(ct, "foo", "{path}: {ct}"),
            other => panic!("{other:?}"),
        }
    }
    // die Timeout-URL hat keine Antwort
    assert_eq!(all.int("/timeout", "Status Code"), 0);
    assert_address_golden(
        &fx,
        "onpage-ctype",
        "Internal",
        "Unknown",
        "internal__unknown.csv",
    );
}

// spec: ONPAGE-021, ONPAGE-022, ONPAGE-024
#[tokio::test]
async fn every_url_is_in_at_most_one_class_filter_and_filters_are_subsets_of_all() {
    let fx = ctype_site().await;
    let all = filter_addresses(&fx, "All");
    let mut seen: Vec<String> = Vec::new();
    for filter in ["HTML", "JavaScript", "CSS", "Images", "PDF", "Unknown"] {
        for a in filter_addresses(&fx, filter) {
            assert!(all.contains(&a), "{filter}: {a} nicht in All");
            assert!(
                !seen.contains(&a),
                "{a} steht in mehr als einem Klassenfilter"
            );
            seen.push(a);
        }
    }
    // Flash, Medien, Schriften, XML und Sonstiges gehören in M1 zu keinem Internal-Klassenfilter
    assert!(!seen.contains(&"/t-swf".to_string()));
    assert!(all.contains(&"/t-swf".to_string()));
    assert_address_golden(&fx, "onpage-ctype", "Internal", "All", "internal__all.csv");
}

// ---------------------------------------------------------------------------------------------------
// Tragfähigkeit der Definitionen (alle Spalten sortierbar, Zähler aller Filter berechenbar)

#[tokio::test]
async fn every_column_of_the_three_tabs_can_be_selected_sorted_and_counted() {
    let fx = internal_site().await;
    let cfg = CrawlConfig::default();
    for tab in [
        tabs::internal::tab(),
        tabs::external::tab(),
        tabs::security::tab(),
    ] {
        for col in &tab.columns {
            for descending in [false, true] {
                let mut q = ViewQuery::new(tab.id, "All");
                q.columns = ColumnSelection::Named(vec!["Address".into(), col.key.into()]);
                q.sort = vec![SortKey {
                    column: col.key.to_string(),
                    descending,
                }];
                q.limit = 500;
                let page = views::query_view(&fx.file, &cfg, &q)
                    .unwrap_or_else(|e| panic!("{} / {}: {e}", tab.id, col.key));
                assert_eq!(page.rows.len() as u64, page.total.min(500));
            }
        }
    }
    let counts = views::filter_counts(&fx.file, &cfg).expect("Zähler");
    let internal_all = counts
        .iter()
        .find(|c| c.tab == "Internal" && c.filter == "All")
        .expect("Internal:All");
    let html = counts
        .iter()
        .find(|c| c.tab == "Internal" && c.filter == "HTML")
        .expect("Internal:HTML");
    assert_eq!(internal_all.count, Some(36));
    assert_eq!(html.count, Some(30));
    // SQ-12: p0/p2 sind nur per Paginierung bekannt; 30 von 36 = 83,33… → 83,3.
    assert_eq!(html.pct.map(|p| (p * 10.0).round() / 10.0), Some(83.3));
    let security_http = counts
        .iter()
        .find(|c| c.tab == "Security" && c.filter == "HTTP URLs")
        .expect("Security:HTTP URLs");
    assert_eq!(
        security_http.count,
        Some(36),
        "alle Internal-URLs sind http"
    );
}

// spec: ONPAGE-143 [M2]
#[tokio::test]
async fn enabling_hash_recomputes_internal_values_and_duplicates_without_http_requests() {
    let mut old = CrawlConfig::default();
    old.extraction.hash = false;
    old.extraction.store_html = true;
    let mut fx = crawl_fixture("onpage-content-dupes", old.clone()).await;
    let requests = fx.site.total_requests();
    let mut new = old.clone();
    new.extraction.hash = true;
    let duplicates = |fx: &FixtureCrawl, cfg: &CrawlConfig| {
        query(fx, cfg, "Content", "Exact Duplicates", ColumnSelection::All)
    };
    assert!(duplicates(&fx, &old).rows.is_empty());
    let cancel = os_model::CancelToken::new();
    let changed = os_engine::recompute::e01::run(&mut fx.file, &old, &new, &cancel).unwrap();
    assert!(!changed.is_empty());
    let internal = query(&fx, &new, "Internal", "All", ColumnSelection::All);
    let hash = internal.text("/a.html", "Hash");
    assert_eq!(hash.len(), 32);
    assert_eq!(internal.text("/b.html", "Hash"), hash);
    assert_ne!(internal.text("/c.html", "Hash"), hash);
    let page = duplicates(&fx, &new);
    assert!(page.contains("/a.html"));
    assert!(page.contains("/b.html"));
    assert!(!page.contains("/c.html"));
    assert!(
        os_engine::recompute::e01::run(&mut fx.file, &old, &new, &cancel)
            .unwrap()
            .is_empty()
    );
    assert_eq!(fx.site.total_requests(), requests);
}
