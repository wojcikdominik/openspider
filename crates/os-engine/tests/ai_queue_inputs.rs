//! M4-D05: Quelle der Eingabefelder für einen KI-Lauf nach dem Crawl (08 AI-032, Nachtrag „Quelle der Eingabefelder“, K4-24,
//! berichtigt 27.11.2026 mit K4-26): (1) gespeicherte KI-Eingaben, (2) aus `page_html` abgeleitet, (3) sonst nicht
//! vorhanden; Titel, Meta Description, H1, Status, Wortzahl aus `urls`, `hreflang_self` aus `hreflang`. Kein Seitenabruf.
use os_engine::ai_input::load_pages;
use os_engine::testing::{crawl_fixture_with, test_deps};
use os_model::ai::hook::{InputSource, PageAiFields};
use os_model::CrawlConfig;
use os_store::ai::inputs::{self, StoredInputs};

fn find<'a>(pages: &'a [PageAiFields], base: &str, path: &str) -> &'a PageAiFields {
    pages
        .iter()
        .find(|p| p.url == format!("{base}{path}"))
        .unwrap_or_else(|| panic!("{path} fehlt"))
}

// spec: AI-032 [M4]
// Crawl ohne gespeicherte Eingaben, mit gespeichertem HTML (CRAWL-063) → die Felder werden aus `page_html` abgeleitet
// (gleiche Aufbereitung wie während des Crawls); ohne gespeichertes HTML → nicht vorhanden, Titel bleibt verfügbar.
#[tokio::test]
async fn fields_are_derived_from_page_html_or_unavailable() {
    let mut cfg = CrawlConfig::default();
    cfg.extraction.store_html = true;
    let fx = crawl_fixture_with("ai/run/site", "/", cfg.clone(), test_deps()).await;
    let base = fx.site.base_url();
    let site_requests = fx.site.requests().len();
    let pages = load_pages(&fx.dir.path().join("crawl.ospider"), &cfg).unwrap();
    assert_eq!(pages.len(), 6, "Grundmenge G");
    let f = find(&pages, &base, "/fields");
    assert_eq!(f.source, InputSource::Derived);
    assert!(f.page_text.as_deref().unwrap().contains("MARK-TEXT"));
    assert_eq!(f.h2, ["Pflege MARK-H2", "Größen MARK-H2"]);
    assert_eq!(f.html_lang.as_deref(), Some("MARK-LANG"));
    assert_eq!(
        f.hreflang_self,
        ["MARK-HREFLANG"],
        "aus der Tabelle hreflang"
    );
    assert_eq!(f.title.as_deref(), Some("Wanderschuhe MARK-TITLE"));
    assert_eq!(
        f.images[0].context.as_deref(),
        Some("Bildunterschrift MARK-CONTEXT")
    );
    assert_eq!(find(&pages, &base, "/gone").status_code, Some(404));
    assert_eq!(fx.site.requests().len(), site_requests, "kein Seitenabruf");
    // Nichts wurde nachgespeichert.
    assert_eq!(inputs::size(fx.file.conn()).unwrap().entries, 0);

    let fx = crawl_fixture_with("ai/run/site", "/", CrawlConfig::default(), test_deps()).await;
    let base = fx.site.base_url();
    let pages = load_pages(
        &fx.dir.path().join("crawl.ospider"),
        &CrawlConfig::default(),
    )
    .unwrap();
    let f = find(&pages, &base, "/fields");
    assert_eq!(f.source, InputSource::Unavailable);
    assert_eq!(f.page_text, None);
    assert_eq!(
        f.title.as_deref(),
        Some("Wanderschuhe MARK-TITLE"),
        "Spalten von urls"
    );
    assert_eq!(
        f.meta_description.as_deref(),
        Some("Leichte Schuhe MARK-META")
    );
    assert_eq!(f.h1, ["Wanderschuhe MARK-H1"]);
    assert!(f.word_count.unwrap() > 0);
}

// spec: AI-032 [M4]
// Gespeicherte KI-Eingaben haben Vorrang vor dem gespeicherten HTML; `structured_data_types` NULL bleibt „nicht
// vorhanden“, eine leere Liste bleibt leer (K4-26).
#[tokio::test]
async fn stored_inputs_take_precedence() {
    let mut cfg = CrawlConfig::default();
    cfg.extraction.store_html = true;
    let fx = crawl_fixture_with("ai/run/site", "/", cfg.clone(), test_deps()).await;
    let base = fx.site.base_url();
    let id: i64 = fx
        .file
        .conn()
        .query_row(
            "SELECT id FROM urls WHERE address = ?1",
            [format!("{base}/fields")],
            |r| r.get(0),
        )
        .unwrap();
    let p1: i64 = fx
        .file
        .conn()
        .query_row(
            "SELECT id FROM urls WHERE address = ?1",
            [format!("{base}/p1")],
            |r| r.get(0),
        )
        .unwrap();
    inputs::write(
        fx.file.conn(),
        id,
        &StoredInputs {
            page_text: Some("gespeicherter Text".into()),
            h2: vec!["gespeichert".into()],
            html_lang: Some("de".into()),
            ..StoredInputs::default()
        },
    )
    .unwrap();
    inputs::write(
        fx.file.conn(),
        p1,
        &StoredInputs {
            structured_data_types: Some(Vec::new()),
            ..StoredInputs::default()
        },
    )
    .unwrap();
    let pages = load_pages(&fx.dir.path().join("crawl.ospider"), &cfg).unwrap();
    let f = find(&pages, &base, "/fields");
    assert_eq!(f.source, InputSource::Stored);
    assert_eq!(f.page_text.as_deref(), Some("gespeicherter Text"));
    assert_eq!(f.h2, ["gespeichert"]);
    assert_eq!(f.structured_data_types, None);
    assert_eq!(
        find(&pages, &base, "/p1").structured_data_types.as_deref(),
        Some(&[][..])
    );
}
