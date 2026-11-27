//! Unabhängige Abnahmetests M4-V01 (1.1-Etappe) über die Command-Schicht der App (`App::dispatch`): Schalter
//! „KI (Vorschau)“, Sichtbarkeit von Tab und Exporten (UI-004 [M4]), Geltungsbereich (AI-070, AI-072), Live-Prüfung mit
//! Pixelbreite nach UI-038 (AI-071, AI-072, AI-076), Prüfen/Übernehmen/Massenaktion und Persistenz (AI-070, AI-076),
//! Neuprüfung nach Grenzwertänderung (AI-075), Fokus-Begriffe (AI-070), Kostenbestätigung (AI-020).
//!
//! Erwartungen aus `docs/spec/08-ai.md`, `docs/spec/05-ui.md` (UI-004, UI-037, UI-038) und den Festlegungen des
//! Orchestrators im Auftrag (Exportnamen, „Kandidat 1“ = gewählter Kandidat, Lauf aus der App endet bis M4-D05/N01 mit
//! `Pending (confirmation)`). Commands und DTO-Namen aus `crates/os-app/src/commands/ai_suggest.rs`.
// acceptance: independent
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

#[path = "../../../tests/fixtures/ai/v01-acceptance/states.rs"]
mod states;

use std::collections::BTreeSet;
use std::path::Path;

use os_app::{App, AppConfig, CallCtx, ErrorCode};
use serde_json::{json, Value};
use states::*;

fn ctx() -> CallCtx {
    CallCtx::for_window("main")
}

fn app(home: &Path) -> App {
    App::new(AppConfig::new(home)).expect("App startet")
}

async fn call(app: &App, name: &str, args: Value) -> Value {
    app.dispatch(ctx(), name, args)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

async fn preview(app: &App, on: bool) {
    call(
        app,
        "settings.set",
        json!({ "patch": { "aiApp": { "previewEnabled": on } } }),
    )
    .await;
}

async fn open(app: &App, path: &Path) {
    call(app, "crawl.open", json!({"kind": "path", "path": path})).await;
}

/// App mit geöffneter Zustands-Fixture; Schalter nach `on`.
async fn with_states(dir: &Path, on: bool) -> (App, std::path::PathBuf) {
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let path = {
        let f = create(dir, "states.ospider", os_model::CrawlConfig::default());
        f.path().to_path_buf()
    };
    let app = app(&home);
    preview(&app, on).await;
    open(&app, &path).await;
    (app, path)
}

fn element<'a>(detail: &'a Value, id: &str) -> &'a Value {
    detail["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["element"] == id)
        .unwrap_or_else(|| panic!("Element {id} fehlt: {detail}"))
}

async fn title_of(app: &App, path: &str) -> Value {
    let d = call(app, "ai.suggestDetail", json!({"urlId": id(path)})).await;
    element(&d, "title").clone()
}

const COMMANDS: [&str; 7] = [
    "ai.suggestRun",
    "ai.suggestPlan",
    "ai.suggestDetail",
    "ai.suggestReview",
    "ai.suggestCheck",
    "ai.suggestRecheck",
    "ai.suggestFocusImport",
];

fn args_for(name: &str) -> Value {
    match name {
        "ai.suggestDetail" => json!({"urlId": 1}),
        "ai.suggestReview" => json!({"urlIds": [1], "element": "title", "action": "reject"}),
        "ai.suggestCheck" => json!({"urlId": 1, "element": "title", "value": "Ein Title"}),
        "ai.suggestFocusImport" => {
            json!({"csv": "Address,Keyword\nhttp://ai.test:8080/valid-open/,Schuhe"})
        }
        "ai.suggestRecheck" => Value::Null,
        _ => json!({}),
    }
}

// =================================================================================================================
// Schalter „KI (Vorschau)“ und Sichtbarkeit.

// spec: AI-070 [M4]
// spec: UI-004 [M4]
// acceptance: independent
// Schalter aus → jeder Vorschlags-Command antwortet mit dem Vorschau-Fehler und ändert nichts an der Datei.
#[tokio::test]
async fn suggestion_commands_answer_with_the_preview_error_when_off() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = with_states(dir.path(), false).await;
    for name in COMMANDS {
        let err = match app.dispatch(ctx(), name, args_for(name)).await {
            Ok(v) => panic!("{name} darf ohne Schalter nicht gelingen: {v}"),
            Err(e) => e,
        };
        assert_eq!(err.code, ErrorCode::Unsupported, "{name}: {err:?}");
        assert!(err.message_key.contains("preview"), "{name}: {err:?}");
    }
    drop(app);
    let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
    let r = os_store::ai::suggestions::load(f.conn(), 1, "title")
        .unwrap()
        .unwrap();
    assert_eq!(
        r.review,
        os_store::ai::suggestions::ReviewStatus::Open,
        "nichts geändert"
    );
    assert!(os_store::ai::suggestions::focus_map(f.conn())
        .unwrap()
        .is_empty());
}

fn is_suggestions(name: &str) -> bool {
    name == "AI Suggestions"
        || name.starts_with("AI Suggestions:")
        || name.starts_with("AI Suggestions /")
}

/// Namen, die der Kern dem Frontend als Tabs, Filter, Zähler, Übersicht und Exporte liefert.
async fn listed(app: &App) -> Vec<String> {
    let mut out = Vec::new();
    let tabs = call(app, "catalog.tabs", Value::Null).await;
    for t in tabs.as_array().unwrap() {
        out.push(t["id"].as_str().unwrap().to_string());
    }
    let counts = call(app, "catalog.filterCounts", json!({})).await;
    for f in counts["filters"].as_array().unwrap() {
        out.push(f["tab"].as_str().unwrap().to_string());
    }
    let overview = call(
        app,
        "sidebar.overview",
        json!({"segment": null, "compare": null}),
    )
    .await;
    for t in overview["tabs"].as_array().into_iter().flatten() {
        out.push(t["tab"].as_str().unwrap_or("").to_string());
    }
    let exports = call(app, "export.catalog", Value::Null).await;
    for group in ["bulk", "reports"] {
        for g in exports[group].as_array().into_iter().flatten() {
            for i in g["items"].as_array().into_iter().flatten() {
                out.push(i["name"].as_str().unwrap_or("").to_string());
            }
        }
    }
    out
}

// spec: UI-004 [M4]
// spec: AI-070 [M4]
// spec: AI-076 [M4]
// acceptance: independent
// Schalter aus: weder Tab noch Filter, Zähler, Übersicht oder Exporte „AI Suggestions“ (Exportdialog). Schalter an: Tab
// mit den Filtern aus AI-070 und beide Exporte.
#[tokio::test]
async fn tab_filters_counts_and_exports_follow_the_switch() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = with_states(dir.path(), false).await;
    let off = listed(&app).await;
    let shown: Vec<&String> = off.iter().filter(|n| is_suggestions(n)).collect();
    assert!(shown.is_empty(), "Schalter aus: {shown:?}");
    preview(&app, true).await;
    let tabs = call(&app, "catalog.tabs", Value::Null).await;
    let tab = tabs
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "AI Suggestions")
        .expect("Tab AI Suggestions mit Schalter an");
    let filters: Vec<String> = tab["filters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap_or("").to_string())
        .collect();
    for f in [
        "Titles",
        "Meta Descriptions",
        "H1",
        "Alt Text",
        "No Valid Suggestion",
        "Accepted",
        "Open",
        "Rejected",
    ] {
        assert!(
            filters.iter().any(|x| x == f),
            "Filter {f} fehlt: {filters:?}"
        );
    }
    let on = listed(&app).await;
    for name in [
        "AI Suggestions:Accepted Suggestions",
        "AI Suggestions:All Suggestions",
    ] {
        assert!(on.iter().any(|n| n == name), "Export {name} fehlt");
    }
}

// spec: UI-004 [M4]
// acceptance: independent
// CLI (`--export-tabs`, `--bulk-export`, Listen) und MCP-Katalog (`list_catalog`) nennen bei ausgeschaltetem Schalter
// keine „AI Suggestions“-Einträge.
#[test]
fn cli_lists_hide_ai_suggestions_when_off() {
    use os_ai::PreviewSwitch;
    use os_cli::registry::{names_with, Kind};
    for kind in [Kind::Tab, Kind::Bulk, Kind::Report] {
        let off = names_with(kind, PreviewSwitch::OFF);
        let hits: Vec<&String> = off.iter().filter(|n| is_suggestions(n)).collect();
        assert!(hits.is_empty(), "CLI {kind:?}: {hits:?}");
    }
    let on = names_with(Kind::Bulk, PreviewSwitch::ON);
    assert!(
        on.iter()
            .any(|n| n == "AI Suggestions:Accepted Suggestions"),
        "{on:?}"
    );
    let tabs_on = names_with(Kind::Tab, PreviewSwitch::ON);
    assert!(tabs_on.iter().any(|n| n.starts_with("AI Suggestions:")));
}

// spec: UI-004 [M4]
// acceptance: independent
// MCP-Werkzeug `list_catalog` (Tabs, Bulk-Exporte) bei ausgeschaltetem Schalter (Einstellungen im Datenordner des
// Servers): keine „AI Suggestions“-Einträge.
#[test]
fn mcp_catalog_hides_ai_suggestions_when_off() {
    use os_mcp::Tool;
    let home = tempfile::tempdir().unwrap();
    let mut settings = os_model::settings::AppSettings::default();
    settings.ai_app.preview_enabled = false;
    settings.save(home.path()).unwrap();
    let crawls = tempfile::tempdir().unwrap();
    let mut tool_ctx = os_mcp::ToolContext::new(crawls.path().to_path_buf());
    tool_ctx.settings_dir = home.path().to_path_buf();
    let tool = os_mcp::tools::list_catalog::ListCatalog;
    for kind in ["tabs", "bulk_exports", "reports"] {
        let out = tool
            .call(&tool_ctx, json!({"kind": kind, "limit": 500}))
            .unwrap_or_else(|e| panic!("list_catalog {kind}: {e:?}"));
        let items = out.untrusted.unwrap()["items"].clone();
        let names: Vec<String> = items
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                i.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| i["name"].as_str().unwrap_or("").to_string())
            })
            .collect();
        let hits: Vec<&String> = names.iter().filter(|n| is_suggestions(n)).collect();
        assert!(hits.is_empty(), "MCP {kind}: {hits:?}");
    }
}

// =================================================================================================================
// Live-Prüfung, Pixelbreite UI-038.

// spec: AI-071 [M4]
// spec: AI-072 [M4]
// spec: AI-076 [M4]
// spec: UI-038 [M4]
// spec: UI-037 [M4]
// acceptance: independent
// „`Title Suggestion k Pixel Width` ist für jeden Kandidaten identisch mit dem Wert, den UI-038 für denselben Text
// liefert“; Meta Description „nach UI-038 (14 px, Desktop-Profil)“. Testvektoren aus UI-038 und dieselben Werte in der
// SERP-Messwerttabelle (UI-037, `detail.serpMeasure`). Länge in Codepoints.
#[tokio::test]
async fn live_check_pixel_width_matches_ui038_and_serp_measure() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = with_states(dir.path(), true).await;
    let w = |c: &str, n: usize| c.repeat(n);
    let title_vectors = [
        ("W".to_string(), Some(18)),
        (w("W", 5), Some(90)),
        (w("i", 5), Some(25)),
        (w("W", 29), Some(522)),
        (w("W", 30), Some(540)),
        (w("i", 70), Some(350)),
        (w("W", 60), Some(1080)),
        ("漢".to_string(), Some(20)),
        ("Grüße aus München – Öl, Äpfel & Übermut".to_string(), None),
        (
            "Familie 👨\u{200d}👩\u{200d}👧 und 漢字 im Title".to_string(),
            None,
        ),
    ];
    for (text, expected) in &title_vectors {
        let c = call(
            &app,
            "ai.suggestCheck",
            json!({"urlId": 1, "element": "title", "value": text}),
        )
        .await;
        let px = c["pixelWidth"].as_u64().unwrap() as u32;
        assert_eq!(
            px,
            os_parse::pixel::width_px(text, 20.0, false),
            "UI-038 für {text:?}"
        );
        if let Some(e) = expected {
            assert_eq!(px, *e, "Testvektor {text:?}");
        }
        assert_eq!(
            c["length"].as_u64().unwrap() as usize,
            text.chars().count(),
            "Codepoints {text:?}"
        );
        let m = call(
            &app,
            "detail.serpMeasure",
            json!({"title": text, "description": "", "device": "desktop", "boldTerms": [], "prefix": ""}),
        )
        .await;
        assert_eq!(
            m["rows"][0]["px"].as_f64().unwrap() as u32,
            px,
            "SERP-Messwert {text:?}"
        );
    }
    for (text, expected) in [
        (w("a", 100), 800),
        (w("a", 126), 1008),
        (w("a", 127), 1016),
        (w("W", 150), 1800),
        (w("i", 160), 480),
    ] {
        let c = call(
            &app,
            "ai.suggestCheck",
            json!({"urlId": 1, "element": "meta_description", "value": text}),
        )
        .await;
        assert_eq!(
            c["pixelWidth"].as_u64().unwrap(),
            expected,
            "UI-038 14 px für {} × {}",
            text.len(),
            &text[..1]
        );
        let m = call(
            &app,
            "detail.serpMeasure",
            json!({"title": "", "description": text, "device": "desktop", "boldTerms": [], "prefix": ""}),
        )
        .await;
        assert_eq!(m["rows"][1]["px"].as_f64().unwrap() as u64, expected);
    }
}

// spec: AI-076 [M4]
// spec: AI-075 [M4]
// spec: AI-071 [M4]
// acceptance: independent
// Live-Prüfung: Verstöße gegen Länge/Pixel und Stufe 1 der Dublettenprüfung (Title einer anderen indexierbaren URL,
// case-insensitiv) werden gemeldet; ein gültiger Text ist gültig.
#[tokio::test]
async fn live_check_reports_length_pixel_and_duplicate_violations() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = with_states(dir.path(), true).await;
    let ok = call(&app, "ai.suggestCheck", json!({"urlId": 1, "element": "title", "value": "Wanderausrüstung für jede Tour und jedes Wetter"})).await;
    assert_eq!(ok["valid"], true, "{ok}");
    let long = "W".repeat(40);
    let bad = call(
        &app,
        "ai.suggestCheck",
        json!({"urlId": 1, "element": "title", "value": long}),
    )
    .await;
    assert_eq!(bad["valid"], false);
    assert!(
        bad["violations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "pixel_width>561"),
        "{bad}"
    );
    let dup = call(
        &app,
        "ai.suggestCheck",
        json!({"urlId": 1, "element": "title", "value": "ohne VORSCHLAG"}),
    )
    .await;
    assert!(
        dup["violations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap_or("").starts_with("duplicate_of:")),
        "{dup}"
    );
}

// =================================================================================================================
// Prüfen, Übernehmen, Massenaktion, Persistenz.

// spec: AI-076 [M4]
// spec: AI-070 [M4]
// acceptance: independent
// Übernehmen, Bearbeiten (mit Verstößen gespeichert, `Edited` mit Verstoßliste), Ablehnen, Zurücksetzen über
// `ai.suggestReview`; alles bleibt nach Schließen und erneutem Öffnen der Crawl-Datei erhalten.
#[tokio::test]
async fn review_actions_persist_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = with_states(dir.path(), true).await;
    let review = |ids: Vec<i64>, action: &str, extra: Value| {
        let mut a = json!({"urlIds": ids, "element": "title", "action": action});
        if let Some(o) = extra.as_object() {
            for (k, v) in o {
                a[k] = v.clone();
            }
        }
        a
    };
    call(
        &app,
        "ai.suggestReview",
        review(vec![id("/valid-open/")], "accept", json!({"candidate": 2})),
    )
    .await;
    let t = title_of(&app, "/valid-open/").await;
    assert_eq!(t["review"], "Accepted");
    assert_eq!(t["accepted"]["value"], V_OPEN_2);
    assert_eq!(t["accepted"]["origin"], "ai");

    let too_long = "Ein viel zu langer bearbeiteter Title, der die Grenze von sechzig Zeichen klar überschreitet";
    call(
        &app,
        "ai.suggestReview",
        review(vec![id("/novalid/")], "edit", json!({"value": too_long})),
    )
    .await;
    let t = title_of(&app, "/novalid/").await;
    assert_eq!(t["review"], "Edited");
    assert_eq!(t["accepted"]["value"], too_long, "keine stille Korrektur");
    assert_eq!(t["accepted"]["origin"], "edited");
    assert!(
        !t["accepted"]["violations"].as_array().unwrap().is_empty(),
        "Verstoßliste: {t}"
    );

    call(
        &app,
        "ai.suggestReview",
        review(vec![id("/accepted/")], "reject", json!({})),
    )
    .await;
    assert_eq!(title_of(&app, "/accepted/").await["review"], "Rejected");
    call(
        &app,
        "ai.suggestReview",
        review(vec![id("/edited/")], "reset", json!({})),
    )
    .await;
    let t = title_of(&app, "/edited/").await;
    assert_eq!(t["review"], "Open");
    assert!(t["accepted"].is_null(), "{t}");

    drop(app);
    use os_store::ai::suggestions::{self as store, ReviewStatus};
    let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
    let load = |p: &str| store::load(f.conn(), id(p), "title").unwrap().unwrap();
    assert_eq!(load("/valid-open/").accepted.unwrap().value, V_OPEN_2);
    let ed = load("/novalid/");
    assert_eq!(ed.review, ReviewStatus::Edited);
    assert_eq!(ed.accepted.as_ref().unwrap().value, too_long);
    assert!(!ed.accepted.unwrap().violations.is_empty());
    assert_eq!(load("/accepted/").review, ReviewStatus::Rejected);
    assert_eq!(load("/edited/").review, ReviewStatus::Open);
    assert!(load("/edited/").accepted.is_none());
}

// spec: AI-076 [M4]
// acceptance: independent
// „Massenaktion „Kandidat 1 übernehmen“ auf 10 URLs übernimmt nur gültige Kandidaten; URLs mit `No valid suggestion`
// bleiben `Open` und werden gezählt gemeldet.“ Übernommen wird der gewählte Kandidat 1.
#[tokio::test]
async fn bulk_accept_first_on_ten_urls() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = with_states(dir.path(), true).await;
    let paths = [
        "/valid-open/",
        "/accepted/",
        "/rejected/",
        "/formula/",
        "/novalid/",
        "/pending/",
        "/error/",
        "/budget/",
        "/refusal/",
        "/skipped/",
    ];
    let ids: Vec<i64> = paths.iter().map(|p| id(p)).collect();
    let result = call(
        &app,
        "ai.suggestReview",
        json!({"urlIds": ids, "element": "title", "action": "acceptFirst"}),
    )
    .await;
    assert_eq!(result["accepted"], 4, "{result}");
    assert_eq!(result["skipped"], 6, "{result}");
    for (p, value) in [
        ("/valid-open/", V_OPEN_1),
        ("/accepted/", V_ACC),
        ("/rejected/", V_REJ),
        ("/formula/", V_FORMULA),
    ] {
        let t = title_of(&app, p).await;
        assert_eq!(t["review"], "Accepted", "{p}");
        assert_eq!(t["accepted"]["value"], value, "{p}: Kandidat 1");
    }
    let t = title_of(&app, "/novalid/").await;
    assert_eq!(t["review"], "Open");
    assert!(t["accepted"].is_null());
}

// =================================================================================================================
// AI-075 Neuprüfung nach Grenzwertänderung.

async fn set_title_max(app: &App, max: u32) {
    let mut dto = call(app, "config.get", Value::Null).await;
    dto["config"]["title"]["max_chars"] = json!(max);
    call(app, "config.set", dto).await;
}

// spec: AI-075 [M4]
// acceptance: independent
// „Grenzwertänderung in den Preferences (Title max. 50 Zeichen) → bestehende Kandidaten werden ohne Request neu geprüft;
// Status ändert sich entsprechend.“ Hier: max. 40 Zeichen; beide Kandidaten von `/valid-open/` haben 43 Zeichen. Erwartet
// wird die Neuprüfung als Folge der Grenzwertänderung selbst (ohne eigenen Aufruf).
#[tokio::test]
async fn changing_title_limit_rechecks_existing_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = with_states(dir.path(), true).await;
    assert_eq!(title_of(&app, "/valid-open/").await["status"], "Valid");
    set_title_max(&app, 40).await;
    let t = title_of(&app, "/valid-open/").await;
    assert_eq!(t["status"], "No valid suggestion", "{t}");
}

// spec: AI-075 [M4]
// acceptance: independent
// Gegenprobe über `ai.suggestRecheck`: Neuprüfung ohne Request mit den aktuellen Grenzwerten; mit dem alten Grenzwert
// wieder gültig.
#[tokio::test]
async fn explicit_recheck_applies_current_limits() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = with_states(dir.path(), true).await;
    set_title_max(&app, 40).await;
    call(&app, "ai.suggestRecheck", Value::Null).await;
    let t = title_of(&app, "/valid-open/").await;
    assert_eq!(t["status"], "No valid suggestion", "{t}");
    assert!(t["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["valid"] == false));
    set_title_max(&app, 60).await;
    call(&app, "ai.suggestRecheck", Value::Null).await;
    assert_eq!(title_of(&app, "/valid-open/").await["status"], "Valid");
}

// =================================================================================================================
// AI-070 Geltungsbereich, Fokus-Begriffe, Bestätigung.

struct ScopePage {
    path: &'static str,
    title: Option<&'static str>,
    h1: &'static str,
    meta: bool,
    status: i64,
    indexable: bool,
}

const LONG_61: &str = "Ein Title mit genau einundsechzig Zeichen für den Filtertest!";
const NORMAL: &str = "Ein ganz normaler Title für die Testseite";
const META_OK: &str =
    "Eine ganz normale Meta Description für die Testseite mit ausreichend Text für den Filter.";

fn scope_pages() -> Vec<ScopePage> {
    vec![
        ScopePage {
            path: "/missing/",
            title: None,
            h1: "H",
            meta: true,
            status: 200,
            indexable: true,
        },
        ScopePage {
            path: "/long/",
            title: Some(LONG_61),
            h1: "H",
            meta: true,
            status: 200,
            indexable: true,
        },
        ScopePage {
            path: "/short/",
            title: Some("Kurz"),
            h1: "H",
            meta: true,
            status: 200,
            indexable: true,
        },
        ScopePage {
            path: "/wide/",
            title: Some("WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW"),
            h1: "H",
            meta: true,
            status: 200,
            indexable: true,
        },
        ScopePage {
            path: "/same-h1/",
            title: Some("Gleicher Text in Title und Überschrift"),
            h1: "Gleicher Text in Title und Überschrift",
            meta: true,
            status: 200,
            indexable: true,
        },
        ScopePage {
            path: "/mismatch/",
            title: Some("Ein unpassender Title für die Testseite"),
            h1: "H",
            meta: true,
            status: 200,
            indexable: true,
        },
        ScopePage {
            path: "/fine/",
            title: Some(NORMAL),
            h1: "H",
            meta: true,
            status: 200,
            indexable: true,
        },
        ScopePage {
            path: "/fine-no-meta/",
            title: Some("Noch ein normaler Title für die Seite"),
            h1: "H",
            meta: false,
            status: 200,
            indexable: true,
        },
        ScopePage {
            path: "/noindex-long/",
            title: Some(LONG_61),
            h1: "H",
            meta: false,
            status: 200,
            indexable: false,
        },
        ScopePage {
            path: "/gone/",
            title: Some(LONG_61),
            h1: "H",
            meta: false,
            status: 404,
            indexable: true,
        },
    ]
}

fn q(v: Option<&str>) -> String {
    v.map_or("NULL".into(), |s| format!("'{}'", s.replace('\'', "''")))
}

fn scope_file(dir: &Path) -> std::path::PathBuf {
    let created = os_model::Timestamp::from_db("2026-11-27T09:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: os_model::IdSource::new_crawl_id(&os_model::SeededIds::new(7272), created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some(format!("{HOST}/")),
        list_source: None,
        project: "Abnahme V01".into(),
        name: "V01 Scope".into(),
        config: os_model::CrawlConfig::default(),
        created_at: created,
    };
    let f = os_store::CrawlFile::create(
        &dir.join("scope.ospider"),
        &init,
        os_store::StorageProfile::Balanced,
    )
    .unwrap();
    let mut sql = String::new();
    for (i, p) in scope_pages().iter().enumerate() {
        let a = format!("{HOST}{}", p.path);
        let len = p.title.map_or(0, |t| t.chars().count());
        let px = p
            .title
            .map_or(0, |t| os_parse::pixel::width_px(t, 20.0, false));
        let meta = p.meta.then(|| format!("{META_OK} Seite {}", p.path));
        let mlen = meta.as_deref().map_or(0, |t| t.chars().count());
        let mpx = meta
            .as_deref()
            .map_or(0, |t| os_parse::pixel::width_px(t, 14.0, false));
        sql.push_str(&format!(
            "INSERT INTO urls (id, url, address, origin, is_internal, state, discovered_via, folder_depth, url_length, \
             status_code, content_class, in_tables, indexability, title_1, title_1_len, title_1_px, title_count, \
             meta_desc_1, meta_desc_1_len, meta_desc_1_px, h1_1) VALUES ({id}, '{a}', '{a}', '{HOST}', 1, 'done', 'link', 1, \
             {alen}, {st}, 'HTML', 1, {idx}, {t}, {len}, {px}, {cnt}, {m}, {mlen}, {mpx}, {h});\n",
            id = i + 1,
            alen = a.chars().count(),
            st = p.status,
            idx = if p.indexable && p.status == 200 { "'Indexable'" } else { "'Non-Indexable'" },
            t = q(p.title),
            cnt = if p.title.is_some() { 1 } else { 0 },
            m = q(meta.as_deref()),
            h = q(Some(p.h1)),
        ));
    }
    let mismatch = scope_pages()
        .iter()
        .position(|p| p.path == "/mismatch/")
        .unwrap()
        + 1;
    sql.push_str(&format!(
        "INSERT INTO ai_questions(id, version, definition_json) VALUES ('title-matches-content', '1', '{{}}');\n\
         INSERT INTO ai_answers(url_id, question_id, question_version, params_hash, params_json, answer_json, status) \
         VALUES ({mismatch}, 'title-matches-content', '1', '', '{{}}', 'false', 'Success');\n"
    ));
    f.conn().execute_batch(&sql).unwrap();
    f.set_meta("state", "Completed").unwrap();
    f.path().to_path_buf()
}

fn scope_id(path: &str) -> i64 {
    scope_pages().iter().position(|p| p.path == path).unwrap() as i64 + 1
}

// spec: AI-070 [M4]
// spec: AI-072 [M4]
// acceptance: independent
// Standard-Geltungsbereich für Titles = Vereinigung von `Missing`, `Over 60 Characters`, `Below 30 Characters`,
// `Over 561 Pixels`, `Below 200 Pixels`, `Same as H1` und `Title Not Matching Content (AI)`; jede URL höchstens einmal;
// nur URLs aus S (indexierbar, 200). (`Duplicate` setzt die Crawl-Analyse voraus und ist hier nicht belegt.) Für Meta
// Descriptions mindestens `Missing`.
#[tokio::test]
async fn default_scope_is_the_union_of_the_problem_filters() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let path = scope_file(dir.path());
    let app = app(&home);
    preview(&app, true).await;
    open(&app, &path).await;
    let plan = call(
        &app,
        "ai.suggestPlan",
        json!({"elements": ["title", "meta_description"], "scope": {"kind": "default"}}),
    )
    .await;
    let ids = |e: &str| -> Vec<i64> {
        plan["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["element"] == e)
            .unwrap_or_else(|| panic!("{e} fehlt: {plan}"))["urlIds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect()
    };
    let titles = ids("title");
    let unique: BTreeSet<i64> = titles.iter().copied().collect();
    assert_eq!(unique.len(), titles.len(), "jede URL höchstens einmal");
    let expected: BTreeSet<i64> = [
        "/missing/",
        "/long/",
        "/short/",
        "/wide/",
        "/same-h1/",
        "/mismatch/",
    ]
    .iter()
    .map(|p| scope_id(p))
    .collect();
    assert_eq!(unique, expected, "Titles: {titles:?}");
    let metas: BTreeSet<i64> = ids("meta_description").into_iter().collect();
    assert!(
        metas.contains(&scope_id("/fine-no-meta/")),
        "Missing Meta: {metas:?}"
    );
    assert!(!metas.contains(&scope_id("/fine/")));
    assert!(!metas.contains(&scope_id("/noindex-long/")) && !metas.contains(&scope_id("/gone/")));
}

// spec: AI-020 [M4]
// spec: AI-070 [M4]
// acceptance: independent
// Festlegung des Orchestrators: Ein Lauf aus der App endet bis zur Verdrahtung (M4-D05/N01) mit `Pending (confirmation)`
// und den Zahlen des Voranschlags; die Einträge des Geltungsbereichs tragen diesen Status.
#[tokio::test]
async fn app_run_waits_for_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let path = scope_file(dir.path());
    let app = app(&home);
    preview(&app, true).await;
    open(&app, &path).await;
    let op: Value = call(
        &app,
        "ai.suggestRun",
        json!({"elements": ["title"], "scope": {"kind": "default"}}),
    )
    .await;
    let op_id = op["opId"]
        .as_str()
        .unwrap_or_else(|| panic!("OpHandle: {op}"))
        .to_string();
    let status = app.ops().wait(&op_id).await.expect("Ergebnis");
    let result = status
        .result
        .unwrap_or_else(|| panic!("ohne Ergebnis: {:?}", status.error));
    assert_eq!(result["state"], "pendingConfirmation", "{result}");
    let entries = result["elements"][0]["entries"].as_u64().unwrap();
    assert_eq!(entries, 6);
    assert!(
        result["elements"][0]["estimate"].is_object(),
        "Zahlen des Voranschlags: {result}"
    );
    let d = call(
        &app,
        "ai.suggestDetail",
        json!({"urlId": scope_id("/long/")}),
    )
    .await;
    assert_eq!(element(&d, "title")["status"], "Pending (confirmation)");
    let d = call(
        &app,
        "ai.suggestDetail",
        json!({"urlId": scope_id("/fine/")}),
    )
    .await;
    assert!(
        element(&d, "title")["status"].is_null(),
        "außerhalb des Geltungsbereichs kein Eintrag"
    );
}

// spec: AI-070 [M4]
// acceptance: independent
// Fokus-Begriffe per CSV `Address,Keyword`: bekannte Adressen importiert, unbekannte gemeldet, am Eintrag sichtbar.
#[tokio::test]
async fn focus_keywords_are_imported_from_csv() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = with_states(dir.path(), true).await;
    let csv = format!(
        "Address,Keyword\n{},Wanderausrüstung\nhttp://ai.test:8080/unbekannt/,x\n",
        address("/valid-open/")
    );
    let r = call(&app, "ai.suggestFocusImport", json!({"csv": csv})).await;
    assert_eq!(r["imported"], 1, "{r}");
    assert_eq!(r["unknown"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(
        title_of(&app, "/valid-open/").await["focus"],
        "Wanderausrüstung"
    );
}
