//! Unabhängige Abnahmetests M4-V01 (1.1-Etappe) gegen den **ai-mock** (`os-testsite`) mit dem echten Anthropic-Adapter:
//! genau ein Request je URL und Runde (AI-070, AI-075), Protokollzeile je Request (AI-014), Cache auch in der
//! Validierungsschleife (AI-015), Schalter „KI (Vorschau)“, Refusal (AI-038) und Ausgabegrenze (AI-020 Nachtrag
//! 24.11.2026). Eigene Antwortfragmente unter `tests/fixtures/ai/v01-acceptance/mock-answers.d/`.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md`. Nur 127.0.0.1, nur erfundene Schlüssel.
// acceptance: independent

#[path = "acceptance_v01_support/mod.rs"]
mod support;

use os_ai::tasks::suggest::{Element, RunError};
use os_ai::PreviewSwitch;
use serde_json::{json, Value};
use support::*;

fn fragment(id: &str) -> Vec<String> {
    let doc: Value = serde_json::from_str(
        &std::fs::read_to_string(fixture_dir().join("mock-answers.d/v01-acceptance.json")).unwrap(),
    )
    .unwrap();
    doc[id]["default"]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn page(path: &str, marker: &str) -> Page {
    Page::new(path, Some(&format!("Alt {path}")))
        .text(&format!("Seitentext der Seite {path}. {marker} Ende."))
}

// spec: AI-070 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Sofort gültige Antwort: genau 1 Request im Mock-Log, 1 Protokollzeile mit Kosten, Kandidaten in Antwortreihenfolge.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn valid_answer_is_one_request_and_one_log_row() {
    let mock = Mock::start().await;
    let kit = Kit::new("m-valid", vec![page("/v/", "MOCK-ID:v01acc-valid")]);
    let adapter = mock.anthropic();
    let meter = TableMeter::default();
    kit.engine(&adapter, &meter)
        .run(&kit.job(Element::Title, options()))
        .await
        .unwrap();
    assert_eq!(mock.posts().len(), 1);
    let r = kit.record("/v/", Element::Title);
    assert_eq!(r.status, "Valid");
    let values: Vec<String> = r.candidates.iter().map(|c| c.value.clone()).collect();
    assert_eq!(values, fragment("v01acc-valid"));
    let rows = kit.log_rows("/v/");
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].0 && rows[0].1 > 0, "{rows:?}");
}

// spec: AI-075 [M4]
// spec: AI-015 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Dauerhaft ungültige (immer gleiche) Antwort: Runde 1 und 2 gehen an den Mock, der zweite Request enthält die konkreten
// Verstöße (Form „612 px > 561 px“); Runde 3 ist derselbe Request wie Runde 2 → Cache-Treffer ohne neuen Request. Ende
// nach 1 + 2 Runden mit `No valid suggestion`; je Runde eine Protokollzeile.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn permanently_invalid_answer_runs_three_rounds_with_one_cache_hit() {
    let mock = Mock::start().await;
    let kit = Kit::new("m-invalid", vec![page("/i/", "MOCK-ID:v01acc-invalid")]);
    let adapter = mock.anthropic();
    let meter = TableMeter::default();
    kit.engine(&adapter, &meter)
        .run(&kit.job(Element::Title, options()))
        .await
        .unwrap();
    let posts = mock.posts();
    assert_eq!(posts.len(), 2, "Runde 3 aus dem Cache");
    let first = &fragment("v01acc-invalid")[0];
    let px = 9 * first.chars().count();
    let expected = format!("{px} px > 561 px");
    assert!(
        posts[1].contains(&expected),
        "zweiter Request ohne „{expected}“"
    );
    assert!(!posts[0].contains(&expected));
    let r = kit.record("/i/", Element::Title);
    assert_eq!(r.status, "No valid suggestion");
    assert_eq!(r.rounds, 3);
    let rows = kit.log_rows("/i/");
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert!(
        rows[2].0 && rows[2].1 == 0,
        "dritte Zeile Cache-Treffer: {rows:?}"
    );
}

// spec: AI-015 [M4]
// acceptance: independent
// Zweiter Lauf: 0 Requests an den Mock.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rerun_sends_nothing_to_the_mock() {
    let mock = Mock::start().await;
    let kit = Kit::new(
        "m-rerun",
        vec![
            page("/v/", "MOCK-ID:v01acc-valid"),
            page("/i/", "MOCK-ID:v01acc-invalid"),
        ],
    );
    let adapter = mock.anthropic();
    let meter = TableMeter::default();
    for _ in 0..2 {
        kit.engine(&adapter, &meter)
            .run(&kit.job(Element::Title, options()))
            .await
            .unwrap();
    }
    assert_eq!(mock.posts().len(), 3, "nur der erste Lauf fragt an (1 + 2)");
    assert_eq!(kit.record("/v/", Element::Title).status, "Valid");
    assert_eq!(
        kit.record("/i/", Element::Title).status,
        "No valid suggestion"
    );
}

// spec: AI-070 [M4]
// acceptance: independent
// Schalter aus → 0 Requests an den Mock.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preview_off_sends_nothing_to_the_mock() {
    let mock = Mock::start().await;
    let mut kit = Kit::new("m-off", vec![page("/v/", "MOCK-ID:v01acc-valid")]);
    kit.switch = PreviewSwitch::OFF;
    let adapter = mock.anthropic();
    let meter = TableMeter::default();
    let result = kit
        .engine(&adapter, &meter)
        .run(&kit.job(Element::Title, options()))
        .await;
    assert!(matches!(result, Err(RunError::PreviewOff)));
    assert!(mock.mock.requests().is_empty());
}

// spec: AI-038 [M4]
// spec: AI-015 [M4]
// acceptance: independent
// Refusal: Status `Refusal`, 1 Request, kein Reparaturversuch; Refusals werden gecacht (erneuter Lauf ohne Request).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refusal_is_final_and_cached() {
    let mock = Mock::start().await;
    let kit = Kit::new(
        "m-refusal",
        vec![page("/r/", "MOCK-ID:v01acc-valid MOCK-SCENARIO:refusal")],
    );
    let adapter = mock.anthropic();
    let meter = TableMeter::default();
    for _ in 0..2 {
        kit.engine(&adapter, &meter)
            .run(&kit.job(Element::Title, options()))
            .await
            .unwrap();
    }
    assert_eq!(mock.posts().len(), 1);
    assert_eq!(kit.record("/r/", Element::Title).status, "Refusal");
}

// spec: AI-020 [M4]
// spec: AI-021 [M4]
// acceptance: independent
// Szenario `output-limit`: `Error: Output limit reached`, genau 1 Request (kein Reparaturversuch), Eintrag beim nächsten
// Lauf erneut angefragt (Fehler werden nicht gecacht, AI-015 Nachtrag).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn output_limit_is_not_repaired_and_not_cached() {
    let mock = Mock::start().await;
    let kit = Kit::new(
        "m-limit",
        vec![page(
            "/o/",
            "MOCK-ID:v01acc-valid MOCK-SCENARIO:output-limit",
        )],
    );
    let adapter = mock.anthropic();
    let meter = TableMeter::default();
    kit.engine(&adapter, &meter)
        .run(&kit.job(Element::Title, options()))
        .await
        .unwrap();
    assert_eq!(mock.posts().len(), 1);
    assert_eq!(
        kit.record("/o/", Element::Title).status,
        "Error: Output limit reached"
    );
    kit.engine(&adapter, &meter)
        .run(&kit.job(Element::Title, options()))
        .await
        .unwrap();
    assert_eq!(mock.posts().len(), 2, "Fehler nicht gecacht");
}

// spec: AI-014 [M4]
// acceptance: independent
// AI-014 Nachtrag 24.11.2026: „Szenario `output-limit` je Anbieter → Zeile mit `Status = Error: Output limit reached`,
// `HTTP Status = 200`, `Error Code` wie oben, `Output Tokens` = gesendete Grenze, `Cost` > 0.“ (Bekannte Lücke über
// `guard::execute`: keine `usage` verbucht – hier nur bestätigt.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "wartet auf M4-N01: Lücke im Kern – bei Abbruch an der Ausgabegrenze verbucht guard::execute keine usage (Output Tokens leer, Cost 0); Auflage N01, Entscheidung des Orchestrators 27.11.2026"]
async fn output_limit_row_books_the_reported_tokens() {
    let mock = Mock::start().await;
    let kit = Kit::new(
        "m-limit-row",
        vec![page(
            "/o/",
            "MOCK-ID:v01acc-valid MOCK-SCENARIO:output-limit",
        )],
    );
    let adapter = mock.anthropic();
    let meter = TableMeter::default();
    kit.engine(&adapter, &meter)
        .run(&kit.job(Element::Title, options()))
        .await
        .unwrap();
    let (http, code, output, cost): (Option<i64>, Option<String>, Option<i64>, i64) = kit.store.with(|f| {
        f.conn()
            .query_row(
                "SELECT http_status, error_code, output_tokens, cost_microusd FROM ai_requests WHERE url_id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap()
    });
    assert_eq!(http, Some(200));
    assert_eq!(code.as_deref(), Some("max_tokens"));
    assert!(output.unwrap_or(0) > 0, "Output Tokens = gesendete Grenze");
    assert!(cost > 0, "Cost > 0");
}

// spec: AI-020 [M4]
// acceptance: independent
// Atomarität und Absturz mit dem Mock-Szenario `hold` (08 Konventionen, Nachtrag 26.11.2026): Während Request 1 gehalten
// wird, endet der Prozess ohne Freigabe → danach Eintrag `Pending`, keine Protokollzeile, kein Cache-Eintrag, keine
// verbuchten Kosten; ein neuer Lauf sendet den Eintrag genau einmal.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "wartet auf M4-D01: ai-mock-Szenario `hold` (POST /__mock/scenario hold, /__mock/release) fehlt auf diesem Zweig"]
async fn held_request_aborted_leaves_entry_pending_without_trace() {
    let mock = Mock::start().await;
    let status = mock
        .control(
            "/__mock/scenario",
            json!({"provider": "anthropic", "scenario": "hold", "count": 1}),
        )
        .await;
    assert!(status.is_success());
    let kit = Kit::new("m-hold", vec![page("/h/", "MOCK-ID:v01acc-valid")]);
    let adapter = mock.anthropic();
    let meter = TableMeter::default();
    let job = kit.job(Element::Title, options());
    let engine = kit.engine(&adapter, &meter);
    let run = engine.run(&job);
    // Abbruch während des gehaltenen Requests (wie geordnetes Beenden/Absturz): die Zukunft wird verworfen.
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), run).await;
    assert_eq!(kit.record("/h/", Element::Title).status, "Pending");
    assert!(kit.log_rows("/h/").is_empty());
    assert_eq!(kit.count("SELECT count(*) FROM ai_cache"), 0);
    let released = mock
        .control("/__mock/release", json!({"provider": "anthropic"}))
        .await;
    assert!(released.is_success());
    kit.engine(&adapter, &meter)
        .run(&kit.job(Element::Title, options()))
        .await
        .unwrap();
    assert_eq!(kit.log_rows("/h/").len(), 1, "genau einmal neu gesendet");
}
