//! Unabhängige Abnahmetests M4-F02: KI-Command-Katalog (IPC) und Schalter „KI (Vorschau)“ über `settings.*`.
//! acceptance: independent
//!
//! Erwartungen ausschließlich aus `docs/plan/M4.md` 3.3 (Command-Katalog: Stubs mit ts-rs-Ausgabe), Abschnitt 2 E1
//! (E-2 b: Schalter, Standard aus) und `docs/spec/schemas/settings.schema.json` (`ai_app.preview_enabled`, Standard
//! `false`). Bericht, Commit-Texte und Tests des Bauers wurden nicht gelesen.
//!
//! Schreibweise der Namen (Entscheidung des Orchestrators vom 23.11.2026 zur Unklarheit aus dem Bericht): genau ein
//! Punkt, `ai.<bereich><Aktion>` – die Bereiche aus 3.3 (`ai.cost.estimate` …) erscheinen als `ai.costEstimate` usw.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use os_app::{App, AppConfig, CallCtx, ErrorCode, WindowId};
use serde_json::{json, Value};

/// Bereiche des Command-Katalogs aus M4.md 3.3 in der dort genannten Reihenfolge.
const AREAS: [&str; 15] = [
    "accounts",
    "settings",
    "cost",
    "consent",
    "decisions",
    "questions",
    "tester",
    "suggest",
    "redirects",
    "links",
    "ask",
    "audit",
    "prompts",
    "semantic",
    "search",
];

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn ctx() -> CallCtx {
    CallCtx {
        window: WindowId("main".into()),
        cancel: Default::default(),
    }
}

fn app(home: &Path) -> App {
    App::new(AppConfig::new(home)).expect("App startet")
}

/// Alle registrierten Commands der Domäne `ai`.
fn ai_commands(app: &App) -> Vec<String> {
    app.registry()
        .manifest()
        .into_iter()
        .map(|m| m.name.to_string())
        .filter(|n| n.starts_with("ai."))
        .collect()
}

/// Gehört `name` zum Bereich `area` (`ai.<area>.<x>` oder `ai.<area><Aktion>`)?
fn in_area(name: &str, area: &str) -> bool {
    let Some(rest) = name.strip_prefix("ai.") else {
        return false;
    };
    let Some(tail) = rest.strip_prefix(area) else {
        return false;
    };
    tail.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && !tail.contains('.')
}

/// Hat der Bereich die Aktion `action` (`ai.costEstimate` für `ai.cost.estimate` aus 3.3)?
fn has_action(names: &[String], area: &str, action: &str) -> bool {
    let mut camel = action.to_string();
    camel[..1].make_ascii_uppercase();
    names.iter().any(|n| n == &format!("ai.{area}{camel}"))
}

fn preview_of(settings: &Value) -> Value {
    settings["aiApp"]["previewEnabled"].clone()
}

// spec: UI-004 [M4]
// spec: UI-013 [M4]
// acceptance: independent
// M4.md 3.3: der Katalog deckt alle 15 Bereiche ab; jeder Bereich hat mindestens einen Command.
#[tokio::test]
async fn every_area_of_the_ai_catalog_has_a_command() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    let names = ai_commands(&app);
    let missing: Vec<&str> = AREAS
        .iter()
        .copied()
        .filter(|area| !names.iter().any(|n| in_area(n, area)))
        .collect();
    assert!(
        missing.is_empty(),
        "Bereiche ohne Command: {missing:?}\nvorhanden: {names:?}"
    );
}

// spec: UI-013 [M4]
// acceptance: independent
// M4.md 3.3 nennt die Aktionen ausdrücklich: `ai.cost.estimate|budget|log` und `ai.tester.run`.
#[tokio::test]
async fn named_actions_cost_estimate_budget_log_and_tester_run_exist() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    let names = ai_commands(&app);
    for (area, action) in [
        ("cost", "estimate"),
        ("cost", "budget"),
        ("cost", "log"),
        ("tester", "run"),
    ] {
        assert!(
            has_action(&names, area, action),
            "ai.{area}.{action} fehlt; vorhanden: {names:?}"
        );
    }
}

// spec: UI-004 [M4]
// acceptance: independent
// Entscheidung 23.11.2026: jeder KI-Command hat genau einen Punkt (`ai.<bereich><Aktion>`).
#[tokio::test]
async fn every_ai_command_name_has_exactly_one_dot() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    let names = ai_commands(&app);
    assert!(!names.is_empty());
    let wrong: Vec<&String> = names
        .iter()
        .filter(|n| n.matches('.').count() != 1)
        .collect();
    assert!(
        wrong.is_empty(),
        "Namen mit mehr als einem Punkt: {wrong:?}"
    );
}

// spec: UI-004 [M4]
// acceptance: independent
// M4.md 3.3 „Commands (Stubs, ts-rs)“: jeder KI-Command steht im Manifest `commands.json`, und seine Argument- und
// Rückgabetypen haben eine erzeugte TypeScript-Datei unter `apps/desktop/src/ipc/gen/`.
#[tokio::test]
async fn ai_commands_are_in_the_manifest_with_generated_types() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    let names: BTreeSet<String> = ai_commands(&app).into_iter().collect();
    assert!(!names.is_empty(), "keine KI-Commands registriert");

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(repo("apps/desktop/src/ipc/commands.json"))
            .expect("commands.json lesbar"),
    )
    .unwrap();
    let in_file: BTreeSet<String> = manifest
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["name"].as_str())
        .filter(|n| n.starts_with("ai."))
        .map(str::to_string)
        .collect();
    assert_eq!(in_file, names, "commands.json und Registry weichen ab");

    let gen = repo("apps/desktop/src/ipc/gen");
    let primitives = ["void", "string", "number", "boolean", "null"];
    for entry in manifest.as_array().unwrap() {
        let name = entry["name"].as_str().unwrap_or_default();
        if !name.starts_with("ai.") {
            continue;
        }
        for field in ["args", "ret"] {
            let expr = entry[field].as_str().expect("Typangabe");
            for part in expr.split('|').map(str::trim) {
                let base = part.trim_end_matches("[]");
                if primitives.contains(&base) {
                    continue;
                }
                let file = if base == "JsonValue" {
                    gen.join("serde_json").join("JsonValue.ts")
                } else {
                    gen.join(format!("{base}.ts"))
                };
                assert!(
                    file.is_file(),
                    "{name}: {field} {base} ohne erzeugte Datei {}",
                    file.display()
                );
            }
        }
    }
}

// spec: UI-004 [M4]
// acceptance: independent
// M4.md 3.3: die Commands sind Stubs. Ein Stub antwortet in der Form des Command-Vertrags (M2-ui-concept 4.2) mit
// `Unsupported`/`err-not-implemented` und nennt sich selbst – bei Schalter aus und an; ein bekannter Command ist nie
// `NotFound`.
#[tokio::test]
async fn ai_stubs_answer_not_implemented_with_switch_off_and_on() {
    for on in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let app = app(home.path());
        if on {
            app.dispatch(
                ctx(),
                "settings.set",
                json!({ "patch": { "aiApp": { "previewEnabled": true } } }),
            )
            .await
            .expect("settings.set");
        }
        let names = ai_commands(&app);
        assert!(!names.is_empty());
        // Von Fachpaketen umgesetzte Commands sind keine Stubs mehr (M4-V01: Optimierungsvorschläge).
        const IMPLEMENTED: &[&str] = &[
            "ai.suggestRun",
            "ai.suggestPlan",
            "ai.suggestDetail",
            "ai.suggestReview",
            "ai.suggestCheck",
            "ai.suggestRecheck",
            "ai.suggestFocusImport",
        ];
        for name in names.iter().filter(|n| !IMPLEMENTED.contains(&n.as_str())) {
            let err = match app.dispatch(ctx(), name, json!({})).await {
                Ok(v) => panic!("{name} (Schalter {on}) ist ein Stub und darf nicht gelingen: {v}"),
                Err(e) => e,
            };
            assert_eq!(err.code, ErrorCode::Unsupported, "{name}: {err:?}");
            assert_eq!(err.message_key, "err-not-implemented", "{name}: {err:?}");
            assert_eq!(
                err.args.get("command"),
                Some(&json!(name)),
                "{name}: {err:?}"
            );
        }
    }
}

// spec: AGENT-030
// acceptance: independent
// settings.schema.json: `ai_app.preview_enabled` Standard `false`; über IPC als `aiApp.previewEnabled`.
#[tokio::test]
async fn preview_switch_is_off_by_default_over_ipc() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    let settings = app
        .dispatch(ctx(), "settings.get", Value::Null)
        .await
        .expect("settings.get");
    assert_eq!(preview_of(&settings), json!(false), "{settings}");
}

// spec: AGENT-030
// acceptance: independent
// Einschalten über `settings.set` wirkt sofort, bleibt nach einem Neustart erhalten, Ausschalten ebenso.
#[tokio::test]
async fn preview_switch_roundtrips_through_settings_set_and_restart() {
    let home = tempfile::tempdir().unwrap();
    {
        let app = app(home.path());
        let after = app
            .dispatch(
                ctx(),
                "settings.set",
                json!({ "patch": { "aiApp": { "previewEnabled": true } } }),
            )
            .await
            .expect("settings.set");
        assert_eq!(preview_of(&after), json!(true), "{after}");
    }
    {
        let app = app(home.path());
        let got = app
            .dispatch(ctx(), "settings.get", Value::Null)
            .await
            .unwrap();
        assert_eq!(preview_of(&got), json!(true), "nach Neustart: {got}");
        let off = app
            .dispatch(
                ctx(),
                "settings.set",
                json!({ "patch": { "aiApp": { "previewEnabled": false } } }),
            )
            .await
            .unwrap();
        assert_eq!(preview_of(&off), json!(false), "{off}");
    }
    let app = app(home.path());
    let got = app
        .dispatch(ctx(), "settings.get", Value::Null)
        .await
        .unwrap();
    assert_eq!(preview_of(&got), json!(false), "nach Neustart: {got}");
}
