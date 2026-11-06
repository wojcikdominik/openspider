//! Rechtschreibung und Grammatik im Tab „Content“ (M2-D19) gegen einen echten Crawl der Fixture `ui/fx-ui-spell`:
//! Spalten `Total Language Errors`, `Spelling Errors`, `Grammar Errors`, `Language` (auch im Internal-Tab), Filter
//! `Spelling Errors`/`Grammar Errors`, Detail-Tab „Spelling & Grammar Details“ und PDFs.

use std::path::PathBuf;
use std::sync::OnceLock;

use os_analyze::details::{detail_source, DetailKind, DetailReq};
use os_analyze::views::{query_view, state_of, ColumnSelection, ViewPage, ViewQuery};
use os_engine::testing::{crawl_fixture, FixtureCrawl};
use os_model::{CrawlConfig, FilterState, UrlId, Value};

const SCENARIO: &str = "ui/fx-ui-spell";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ui/fx-ui-spell")
}

/// Rechtschreibung und Grammatik an, Sprachwahl mit den Defaults der Spec; Mini-Wörterbücher der Fixture.
fn spec_cfg() -> CrawlConfig {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        std::env::set_var("OPENSPIDER_DICTIONARIES", fixture().join("dictionaries"));
    });
    let mut cfg = CrawlConfig::default();
    cfg.spelling.enabled = true;
    cfg.grammar.enabled = true;
    cfg.pdf.store = true;
    cfg.spelling.default_language = "en-GB".into();
    cfg.spelling.default_variants = [
        ("en", "en-GB"),
        ("de", "de-DE"),
        ("pt", "pt-PT"),
        ("fa", "fa-IR"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    cfg
}

async fn crawl(cfg: &CrawlConfig) -> FixtureCrawl {
    crawl_fixture(SCENARIO, cfg.clone()).await
}

fn view(fx: &FixtureCrawl, cfg: &CrawlConfig, tab: &str, filter: &str) -> ViewPage {
    let mut q = ViewQuery::new(tab, filter);
    q.columns = ColumnSelection::All;
    q.limit = 500;
    query_view(&fx.file, cfg, &q).unwrap_or_else(|e| panic!("{tab}:{filter}: {e}"))
}

fn short(fx: &FixtureCrawl, address: &str) -> String {
    address
        .strip_prefix(&fx.site.base_url())
        .unwrap_or(address)
        .to_string()
}

fn addresses(fx: &FixtureCrawl, cfg: &CrawlConfig, tab: &str, filter: &str) -> Vec<String> {
    let mut out: Vec<String> = view(fx, cfg, tab, filter)
        .rows
        .iter()
        .map(|r| match &r[0] {
            Value::Text(a) => short(fx, a),
            other => panic!("{other:?}"),
        })
        .collect();
    out.sort();
    out
}

fn cell(fx: &FixtureCrawl, page: &ViewPage, path: &str, column: &str) -> Value {
    let col = page
        .columns
        .iter()
        .position(|c| c == column)
        .unwrap_or_else(|| panic!("Spalte {column} fehlt"));
    let row = page
        .rows
        .iter()
        .find(|r| matches!(&r[0], Value::Text(a) if short(fx, a) == path))
        .unwrap_or_else(|| panic!("Zeile {path} fehlt"));
    row[col].clone()
}

fn url_id(fx: &FixtureCrawl, path: &str) -> UrlId {
    let url = format!("{}{path}", fx.site.base_url());
    UrlId(
        fx.file
            .conn()
            .query_row("SELECT id FROM urls WHERE url=?1", [&url], |r| r.get(0))
            .unwrap_or_else(|e| panic!("{url}: {e}")),
    )
}

// spec: ONPAGE-153
// spec: ONPAGE-166
// spec: ONPAGE-142 [P1] [M2]
#[tokio::test]
async fn language_and_spelling_columns_follow_the_page_language() {
    let cfg = spec_cfg();
    let fx = crawl(&cfg).await;
    let page = view(&fx, &cfg, "Content", "All");
    let int = |p: &str, c: &str| cell(&fx, &page, p, c);
    assert_eq!(
        int("/de-errors.html", "Language"),
        Value::Text("de-DE".into())
    );
    assert_eq!(int("/de-errors.html", "Spelling Errors"), Value::Int(3));
    assert_eq!(int("/de-repeat.html", "Spelling Errors"), Value::Int(3));
    assert_eq!(int("/de-at.html", "Language"), Value::Text("de-AT".into()));
    assert_eq!(int("/pt.html", "Language"), Value::Text("pt-PT".into()));
    assert_eq!(int("/pt-br.html", "Language"), Value::Text("pt-BR".into()));
    assert_eq!(
        int("/en-colour.html", "Language"),
        Value::Text("en-GB".into())
    );
    assert_eq!(int("/en-colour.html", "Spelling Errors"), Value::Int(1));
    assert_eq!(
        int("/no-lang.html", "Language"),
        Value::Text("en-GB".into())
    );
    assert_eq!(
        int("/xx.html", "Language"),
        Value::Text("xx (unsupported)".into())
    );
    assert_eq!(int("/xx.html", "Spelling Errors"), Value::Null);
    assert_eq!(int("/footer-only.html", "Spelling Errors"), Value::Int(0));
    assert_eq!(int("/tokens.html", "Spelling Errors"), Value::Int(1));
    // PDF mit gespeichertem Textlayer: Sprache aus dem Dokumentkatalog, ein Fehlerwort (ONPAGE-166).
    assert_eq!(int("/fehler.pdf", "Language"), Value::Text("de-DE".into()));
    assert_eq!(int("/fehler.pdf", "Spelling Errors"), Value::Int(1));
    assert_eq!(int("/fehler.pdf", "Hash"), Value::Null);
}

// spec: ONPAGE-155
// spec: ONPAGE-156
// spec: ONPAGE-166
#[tokio::test]
async fn filters_list_pages_with_at_least_one_finding() {
    let cfg = spec_cfg();
    let fx = crawl(&cfg).await;
    let spelling = addresses(&fx, &cfg, "Content", "Spelling Errors");
    for path in [
        "/de-errors.html",
        "/de-repeat.html",
        "/fehler.pdf",
        "/tesst.html",
        "/tokens.html",
    ] {
        assert!(spelling.contains(&path.to_string()), "{path}: {spelling:?}");
    }
    for path in [
        "/clean-de.html",
        "/xx.html",
        "/footer-only.html",
        "/no-lang.html",
        "/index.html",
    ] {
        assert!(!spelling.contains(&path.to_string()), "{path}");
    }
    let grammar = addresses(&fx, &cfg, "Content", "Grammar Errors");
    for path in [
        "/grammar-de.html",
        "/grammar-en.html",
        "/tesst.html",
        "/out-052.html",
    ] {
        assert!(grammar.contains(&path.to_string()), "{path}: {grammar:?}");
    }
    for path in ["/clean-de.html", "/grammar-fr.html", "/index.html"] {
        assert!(!grammar.contains(&path.to_string()), "{path}");
    }
    let page = view(&fx, &cfg, "Content", "All");
    assert_eq!(
        cell(&fx, &page, "/grammar-de.html", "Grammar Errors"),
        Value::Int(7)
    );
    assert_eq!(
        state_of(&fx.file, &cfg, "Content", "Spelling Errors").unwrap(),
        FilterState::Ready
    );
}

// spec: ONPAGE-142 [P1] [M2]
// spec: ONPAGE-153
#[tokio::test]
async fn total_language_errors_sums_the_active_checks_and_switched_off_columns_stay_empty() {
    let cfg = spec_cfg();
    let fx = crawl(&cfg).await;
    let both = view(&fx, &cfg, "Content", "All");
    assert_eq!(
        cell(&fx, &both, "/tesst.html", "Spelling Errors"),
        Value::Int(2)
    );
    assert_eq!(
        cell(&fx, &both, "/tesst.html", "Grammar Errors"),
        Value::Int(1)
    );
    assert_eq!(
        cell(&fx, &both, "/tesst.html", "Total Language Errors"),
        Value::Int(3)
    );
    assert_eq!(
        cell(&fx, &both, "/fehler.pdf", "Grammar Errors"),
        Value::Int(0)
    );

    let mut spelling_only = cfg.clone();
    spelling_only.grammar.enabled = false;
    let page = view(&fx, &spelling_only, "Content", "All");
    assert_eq!(
        cell(&fx, &page, "/tesst.html", "Total Language Errors"),
        Value::Int(2)
    );
    assert_eq!(
        cell(&fx, &page, "/tesst.html", "Grammar Errors"),
        Value::Null
    );

    let mut grammar_only = cfg.clone();
    grammar_only.spelling.enabled = false;
    let page = view(&fx, &grammar_only, "Content", "All");
    assert_eq!(
        cell(&fx, &page, "/tesst.html", "Total Language Errors"),
        Value::Int(1)
    );
    assert_eq!(
        cell(&fx, &page, "/tesst.html", "Spelling Errors"),
        Value::Null
    );
    assert_eq!(
        cell(&fx, &page, "/tesst.html", "Language"),
        Value::Text("en-GB".into())
    );

    let mut off = cfg.clone();
    off.spelling.enabled = false;
    off.grammar.enabled = false;
    let page = view(&fx, &off, "Content", "All");
    for column in [
        "Total Language Errors",
        "Spelling Errors",
        "Grammar Errors",
        "Language",
    ] {
        assert_eq!(
            cell(&fx, &page, "/tesst.html", column),
            Value::Null,
            "{column}"
        );
    }
    assert_eq!(
        state_of(&fx.file, &off, "Content", "Spelling Errors").unwrap(),
        FilterState::NeedsConfig("spelling.enabled".into())
    );
    assert_eq!(
        state_of(&fx.file, &off, "Content", "Grammar Errors").unwrap(),
        FilterState::NeedsConfig("grammar.enabled".into())
    );
    let tab = os_analyze::tabs::content::tab();
    for column in [
        "Total Language Errors",
        "Spelling Errors",
        "Grammar Errors",
        "Language",
    ] {
        let def = tab.columns.iter().find(|c| c.key == column).unwrap();
        assert!(def.default_visible, "{column}");
    }
}

// spec: ONPAGE-019
#[tokio::test]
async fn the_internal_tab_mirrors_the_content_values() {
    let cfg = spec_cfg();
    let fx = crawl(&cfg).await;
    for config in [cfg.clone(), {
        let mut off = cfg.clone();
        off.spelling.enabled = false;
        off.grammar.enabled = false;
        off
    }] {
        let content = view(&fx, &config, "Content", "All");
        let internal = view(&fx, &config, "Internal", "All");
        for path in [
            "/de-errors.html",
            "/tesst.html",
            "/xx.html",
            "/grammar-de.html",
            "/fehler.pdf",
        ] {
            for column in ["Spelling Errors", "Grammar Errors", "Language"] {
                let value = cell(&fx, &content, path, column);
                assert_eq!(cell(&fx, &internal, path, column), value, "{path} {column}");
                if !config.spelling.enabled {
                    assert_eq!(value, Value::Null, "{path} {column}: leer statt 0");
                }
            }
        }
    }
}

fn details(
    fx: &FixtureCrawl,
    cfg: &CrawlConfig,
    path: &str,
    params: serde_json::Value,
) -> Vec<Vec<Value>> {
    let ids = [url_id(fx, path)];
    let (owner, source) = detail_source(DetailKind::Spelling);
    assert_eq!(owner, "M2-D19");
    let rows = source(
        &fx.file,
        cfg,
        &DetailReq {
            url_ids: &ids,
            params: &params,
            sort: &[],
            offset: 0,
            limit: 100,
        },
    )
    .unwrap();
    let names: Vec<&str> = rows.columns.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "Error",
            "Type",
            "Detail",
            "Suggestion",
            "Page Section",
            "Rule ID"
        ]
    );
    assert_eq!(rows.total, rows.rows.len() as u64);
    rows.rows
}

// spec: ONPAGE-157
// spec: ONPAGE-153
#[tokio::test]
async fn the_detail_tab_lists_one_row_per_finding_with_section_and_rule() {
    let cfg = spec_cfg();
    let fx = crawl(&cfg).await;
    let rows = details(&fx, &cfg, "/de-errors.html", serde_json::json!({}));
    let json = serde_json::to_value(&rows).unwrap();
    let golden = fixture().join("expected/de-errors.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(
            &golden,
            format!("{}\n", serde_json::to_string_pretty(&json).unwrap()),
        )
        .unwrap();
    }
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&golden).unwrap()).unwrap();
    assert_eq!(json, expected);
    let sections: Vec<&Value> = rows.iter().map(|r| &r[4]).collect();
    assert_eq!(
        sections,
        [
            &Value::Text("Page Title".into()),
            &Value::Text("Meta Description".into()),
            &Value::Text("Body".into())
        ]
    );
    // Eine Zeile je Fehlerinstanz, Grammatik mit Regelkennung.
    let tesst = details(&fx, &cfg, "/tesst.html", serde_json::json!({}));
    let kinds: Vec<&Value> = tesst.iter().map(|r| &r[1]).collect();
    let (spelling, grammar) = (
        Value::Text("Spelling".into()),
        Value::Text("Grammar".into()),
    );
    // Reihenfolge nach Position: „Tesst“, „Tesst Tesst“ (ab derselben Stelle), „Tesst“.
    assert_eq!(kinds, [&spelling, &grammar, &spelling]);
    assert_eq!(tesst[1][0], Value::Text("Tesst Tesst".into()));
    assert_eq!(tesst[1][5], Value::Text("WORD_REPEAT".into()));
    assert_eq!(tesst[0][5], Value::Null);
    // Einschränkung nach Page Section und Type.
    let body = details(
        &fx,
        &cfg,
        "/de-errors.html",
        serde_json::json!({"section": "Body"}),
    );
    assert_eq!(body.len(), 1);
    assert_eq!(body[0][0], Value::Text("Rechtschreibfeler".into()));
    let grammar = details(
        &fx,
        &cfg,
        "/tesst.html",
        serde_json::json!({"type": "Grammar"}),
    );
    assert_eq!(grammar.len(), 1);
    // Ausgeschaltete Prüfung blendet ihre Befunde aus.
    let mut spelling_only = cfg.clone();
    spelling_only.grammar.enabled = false;
    assert_eq!(
        details(&fx, &spelling_only, "/tesst.html", serde_json::json!({})).len(),
        2
    );
    assert_eq!(
        os_analyze::tabs::content::SPELLING_DETAIL_EXTENSIONS,
        ["Page Section", "Rule ID"]
    );
}
