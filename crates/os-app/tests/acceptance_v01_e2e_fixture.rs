//! Fixture der unabhängigen Playwright-Abnahme M4-V01 (`apps/desktop/e2e/ai-suggestions/acceptance-v01.spec.ts`):
//! die Zustands-Fixture (`tests/fixtures/ai/v01-acceptance/states.rs`) plus zwei Seiten für die SERP-Vorschau (UI-037,
//! AI-076): `/serp/` mit einem Kandidaten von gut 570 px (Desktop-Titel, UI-038) und `/serp-real/`, deren echter Title
//! denselben Text trägt.
//!
//! Ohne Umgebungsvariable baut der Test die Datei in einem temporären Ordner und prüft, dass die App sie öffnet und den
//! Tab zeigt. Mit `V01_WRITE_E2E_FIXTURE=1` schreibt er sie nach
//! `tests/fixtures/ai/v01-acceptance/e2e/v01-states.ospider` (Erzeugung der eingecheckten Fixture).
// acceptance: independent
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

#[path = "../../../tests/fixtures/ai/v01-acceptance/states.rs"]
mod states;

use std::path::{Path, PathBuf};

use os_app::{App, AppConfig, CallCtx};
use os_store::ai::suggestions as store;
use serde_json::{json, Value};

pub const SERP_ID: i64 = 14;
pub const SERP_REAL_ID: i64 = 15;

/// Ein Text mit 570–575 px im Desktop-Titelprofil (UI-038).
fn text_570() -> String {
    let base = "Wanderschuhe für Damen und Herren im großen Test mit Kaufberatung und Ratgeber";
    let mut s = String::new();
    for c in base.chars() {
        s.push(c);
        if os_parse::pixel::width_px(&s, 20.0, false) >= 555 {
            break;
        }
    }
    while os_parse::pixel::width_px(&s, 20.0, false) < 570 {
        s.push('i');
    }
    s
}

fn build(path: &Path) {
    if path.exists() {
        std::fs::remove_file(path).unwrap();
    }
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    let tmp_name = path.file_name().unwrap().to_str().unwrap();
    let f = states::create(dir, tmp_name, os_model::CrawlConfig::default());
    let long = text_570();
    let px = os_parse::pixel::width_px(&long, 20.0, false);
    let short = "Wanderschuhe im Test";
    let short_px = os_parse::pixel::width_px(short, 20.0, false);
    let sql = format!(
        "INSERT INTO urls (id, url, address, origin, is_internal, state, discovered_via, folder_depth, url_length, \
         status_code, content_class, in_tables, indexability, title_1, title_1_px, h1_1) VALUES \
         ({SERP_ID}, '{h}/serp/', '{h}/serp/', '{h}', 1, 'done', 'link', 1, 26, 200, 'HTML', 1, 'Indexable', '{short}', {short_px}, 'H1'), \
         ({SERP_REAL_ID}, '{h}/serp-real/', '{h}/serp-real/', '{h}', 1, 'done', 'link', 1, 31, 200, 'HTML', 1, 'Indexable', '{long}', {px}, 'H1');",
        h = states::HOST,
    );
    f.conn().execute_batch(&sql).unwrap();
    let mut r = store::ElementRecord::pending(SERP_ID, store::ELEMENT_TITLE);
    r.status = "Valid".into();
    r.model = Some(states::MODEL.into());
    r.generated_at = Some(states::GENERATED_AT.into());
    r.candidates = vec![
        states::cand(1, "Wanderschuhe für Damen und Herren im Test", 380, &[], 1),
        states::cand(2, &long, px, &["pixel_width>561"], 1),
    ];
    r.selected = Some(1);
    r.order = vec![1];
    store::save(f.conn(), &r).unwrap();
    f.conn()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
}

fn target() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/ai/v01-acceptance/e2e/v01-states.ospider")
}

// spec: AI-076 [M4]
// spec: UI-037 [M4]
// acceptance: independent
// Die Fixture öffnet in der App; mit Schalter an steht der Tab „AI Suggestions“ im Katalog; der 570-px-Kandidat ist über
// 561 px (Kürzung in der SERP-Vorschau).
#[tokio::test]
async fn e2e_fixture_opens_and_shows_the_tab() {
    let tmp = tempfile::tempdir().unwrap();
    let path = if std::env::var_os("V01_WRITE_E2E_FIXTURE").is_some() {
        target()
    } else {
        tmp.path().join("v01-states.ospider")
    };
    build(&path);
    assert!(os_parse::pixel::width_px(&text_570(), 20.0, false) >= 570);
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    // Kopie öffnen, damit die erzeugte Datei unverändert bleibt.
    let copy = tmp.path().join("copy.ospider");
    std::fs::copy(&path, &copy).unwrap();
    let app = App::new(AppConfig::new(&home)).unwrap();
    let ctx = || CallCtx::for_window("main");
    app.dispatch(
        ctx(),
        "settings.set",
        json!({"patch": {"aiApp": {"previewEnabled": true}}}),
    )
    .await
    .unwrap();
    app.dispatch(ctx(), "crawl.open", json!({"kind": "path", "path": copy}))
        .await
        .unwrap();
    let tabs: Value = app
        .dispatch(ctx(), "catalog.tabs", Value::Null)
        .await
        .unwrap();
    assert!(tabs
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == "AI Suggestions"));
    let d: Value = app
        .dispatch(ctx(), "ai.suggestDetail", json!({"urlId": SERP_ID}))
        .await
        .unwrap();
    assert_eq!(
        d["elements"][0]["candidates"].as_array().unwrap().len(),
        2,
        "{d}"
    );
}
