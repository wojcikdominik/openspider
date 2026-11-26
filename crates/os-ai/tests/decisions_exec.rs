//! M4-D01: Ausführung über die Decisions API (AI-033) und Statuswerte eines Laufs auf der Fixture-Site (AI-020).
#[path = "decisions_support.rs"]
mod support;

use std::time::Duration;

use os_ai::adapters::openai_decisions::MaxQuestionsPerRequest;
use os_ai::decisions::exec::{
    ActiveQuestion, Confirmation, DecisionRun, RunControl, RunEnd, RunItem, RunOptions,
};
use os_ai::decisions::format::{InputSpec, QuestionDef};
use os_ai::policy::InputField;
use proptest::prelude::*;
use serde_json::Value;
use support::*;

fn numbered(n: usize) -> Vec<ActiveQuestion> {
    (0..n)
        .map(|i| {
            let mut q = question("soft-404");
            q.id = format!("q{i}");
            q.name = format!("Frage {i}");
            q.inputs = vec![InputSpec::field(InputField::Title)];
            active(q)
        })
        .collect()
}

async fn run_with(
    env: &Env,
    qs: &[ActiveQuestion],
    items: &[RunItem],
    options: RunOptions,
) -> (TempDir, os_store::CrawlFile) {
    let dir = TempDir::new("exec");
    let (file, run) = crawl_file(&dir.0, items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let report = env
        .run(
            file.conn(),
            run,
            qs,
            items,
            &ledger,
            RunControl::new(),
            options,
        )
        .run()
        .await
        .unwrap();
    assert_eq!(report.end, RunEnd::Completed);
    (dir, file)
}

fn question_counts(bodies: &[Value]) -> Vec<usize> {
    bodies
        .iter()
        .map(|b| b["questions"].as_array().unwrap().len())
        .collect()
}

// spec: AI-033 [M4]
// 5 aktive Seitenfragen mit gleicher Eingabe, max. 10 je Request → genau 1 Request je URL mit 5 Fragen; 12 Fragen →
// 2 Requests je URL (10 + 2), jede Frage genau einmal.
#[tokio::test]
async fn bundling_up_to_ten_questions() {
    let env = Env::new(&["openai"]).await;
    let items = vec![text_item(1, "Text"), text_item(2, "Text")];
    let (_d, file) = run_with(&env, &numbered(5), &items, RunOptions::default()).await;
    assert_eq!(question_counts(&env.mock.bodies(DECISIONS)), vec![5, 5]);
    drop(file);
    let env = Env::new(&["openai"]).await;
    let (_d, _file) = run_with(&env, &numbered(12), &items, RunOptions::default()).await;
    let bodies = env.mock.bodies(DECISIONS);
    let mut sizes = question_counts(&bodies);
    sizes.sort();
    assert_eq!(sizes, vec![2, 2, 10, 10]);
    for title in ["Seite 1", "Seite 2"] {
        let mut names: Vec<String> = bodies
            .iter()
            .filter(|b| b["input"].to_string().contains(title))
            .flat_map(|b| b["questions"].as_array().unwrap().clone())
            .map(|q| q["name"].as_str().unwrap().to_string())
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 12);
    }
}

// spec: AI-033 [M4]
// Eine Einstellung über 10 (auch per Profil oder JSON-Ansicht) wird mit Feldfehler abgelehnt.
#[test]
fn more_than_ten_questions_per_request_is_rejected() {
    assert!(MaxQuestionsPerRequest::new(11).is_none());
    assert!(MaxQuestionsPerRequest::new(0).is_none());
    assert!(serde_json::from_value::<MaxQuestionsPerRequest>(serde_json::json!(11)).is_err());
    assert_eq!(MaxQuestionsPerRequest::new(10).unwrap().get(), 10);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 8, .. ProptestConfig::default() })]
    // spec: AI-033 [M4]
    // Kein Request an `/v1/decisions` enthält mehr als 10 Fragen (1–40 aktive Fragen; der Mock lehnt > 10 ab).
    #[test]
    fn never_more_than_ten_questions(n in 1usize..=40, max in 1u8..=10) {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let env = Env::new(&["openai"]).await;
            let items = vec![text_item(1, "Text")];
            let options = RunOptions {
                max_questions: MaxQuestionsPerRequest::new(max).unwrap(),
                ..RunOptions::default()
            };
            let (_d, file) = run_with(&env, &numbered(n), &items, options).await;
            let sizes = question_counts(&env.mock.bodies(DECISIONS));
            assert!(sizes.iter().all(|&s| s <= usize::from(max)));
            assert_eq!(sizes.iter().sum::<usize>(), n);
            assert_eq!(sizes.len(), n.div_ceil(usize::from(max)));
            let c = os_store::ai::answers::list_for_url(file.conn(), 1).unwrap();
            assert!(c.iter().all(|a| a.status == "Success"));
        });
    }
}

fn answer(
    file: &os_store::CrawlFile,
    url_id: i64,
    q: &QuestionDef,
) -> os_store::ai::answers::AnswerRow {
    os_store::ai::answers::get(file.conn(), url_id, &q.id, &q.version.to_string(), "")
        .unwrap()
        .unwrap()
}

// spec: AI-033 [M4]
// Die übernommenen Werte entsprechen exakt den Mock-Werten (Produktseite: `choice = product`, `<Q> Confidence = 0.93`
// als höchste Einzelwahrscheinlichkeit der Mock-`probabilities` mit 14 Einträgen); `<Q> Source = decisions-api`.
#[tokio::test]
async fn values_are_taken_over_unchanged() {
    let env = Env::new(&["openai"]).await;
    let pt = question("page-type");
    let items = vec![text_item(1, "MOCK-ID:product-1 Wanderrucksack")];
    let (_d, file) = run_with(&env, &[active(pt.clone())], &items, RunOptions::default()).await;
    let row = answer(&file, 1, &pt);
    assert_eq!(row.status, "Success");
    assert_eq!(row.source.as_deref(), Some("decisions-api"));
    assert_eq!(row.model.as_deref(), Some("gpt-6-luna"));
    let a: Value = serde_json::from_str(row.answer_json.as_deref().unwrap()).unwrap();
    assert_eq!(a["choice"], "product");
    assert_eq!(a["confidence"], 0.93);
    assert_eq!(a["probabilities"].as_array().unwrap().len(), 14);
    assert!(a.get("normalized").is_none());
    assert_eq!(row.raw_confidence, Some(0.93));
    // Protokollzeile: Decisions-Weg ohne Stufe.
    let log = logged(file.conn());
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].record.endpoint, "decisions");
    assert_eq!(log[0].details.reasoning_level, None);
    assert_eq!(log[0].record.task, "decision");
    assert_eq!(log[0].record.status, "Success");
    assert_eq!(log[0].record.http_status, Some(200));
}

// spec: AI-033 [M4]
// Weicht die Summe der `probabilities` mehr als 0,01 von 1 ab (Mock-Antwort mit Summe 1,03), wird normiert und
// vermerkt (`normalized = true`).
#[tokio::test]
async fn probabilities_off_by_more_than_one_percent_are_normalized() {
    let env = Env::new(&["openai"]).await;
    let pt = question("page-type");
    let items = vec![text_item(1, "MOCK-ID:d01-sum Seite")];
    let (_d, file) = run_with(&env, &[active(pt.clone())], &items, RunOptions::default()).await;
    let a: Value =
        serde_json::from_str(answer(&file, 1, &pt).answer_json.as_deref().unwrap()).unwrap();
    assert_eq!(a["normalized"], true);
    let sum: f64 = a["probabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["probability"].as_f64().unwrap())
        .sum();
    assert!((sum - 1.0).abs() < 1e-9);
    assert!((a["confidence"].as_f64().unwrap() - 0.96 / 1.03).abs() < 1e-9);
}

/// PNG 1 × 1.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44, 0x41, 0x54,
    0x78, 0x9c, 0x63, 0, 1, 0, 0, 5, 0, 1, 0x0d, 0x0a, 0x2d, 0xb4, 0, 0, 0, 0, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

fn image_item(url_id: i64) -> RunItem {
    RunItem {
        url_id,
        is_image: true,
        content_class: "image".into(),
        status_code: Some(200),
        indexable: false,
        segments: Vec::new(),
        data: os_ai::decisions::input::SubjectData {
            url: format!("http://ai.test:8080/images/{url_id}.png"),
            image: Some(os_ai::decisions::input::ImageData {
                media_type: Some("image/png".into()),
                bytes: PNG.to_vec(),
            }),
            alt: Some("Wanderrucksack".into()),
            image_context: Some("Ein Rucksack am Weg".into()),
            ..Default::default()
        },
        params: Default::default(),
    }
}

// spec: AI-033 [M4]
// Bildfragen (`subject = image`) senden genau ein Bild je Request plus die Kontextfelder; Seitenfragen gelten nicht
// für Bilder und umgekehrt.
#[tokio::test]
async fn image_questions_send_one_image_per_request() {
    let env = Env::new(&["openai"]).await;
    let mut alt2 = question("alt-fits");
    alt2.id = "alt-second".into();
    alt2.name = "Alt Second".into();
    let qs = [
        active(question("alt-fits")),
        active(alt2),
        active(question("soft-404")),
    ];
    let mut second = image_item(2);
    // Andere Eingabe, sonst käme das Ergebnis aus dem Cache (AI-015).
    second.data.image_context = Some("Ein Rucksack am Weg, zweites Bild".into());
    let items = vec![image_item(1), second, text_item(3, "Seite")];
    let (_d, file) = run_with(&env, &qs, &items, RunOptions::default()).await;
    let bodies = env.mock.bodies(DECISIONS);
    let image_bodies: Vec<&Value> = bodies
        .iter()
        .filter(|b| b["input"].to_string().contains("input_image"))
        .collect();
    assert_eq!(
        image_bodies.len(),
        2,
        "{:?} / {:?}",
        os_store::ai::answers::list_for_url(file.conn(), 2).unwrap(),
        os_store::ai::answers::list_for_url(file.conn(), 1).unwrap()
    );
    for b in image_bodies {
        let parts = b["input"][0]["content"].as_array().unwrap();
        assert_eq!(
            parts.iter().filter(|p| p["type"] == "input_image").count(),
            1
        );
        assert!(parts[0]["text"]
            .as_str()
            .unwrap()
            .contains("Alt: Wanderrucksack"));
        assert!(parts[0]["text"]
            .as_str()
            .unwrap()
            .contains("Image Context: Ein Rucksack am Weg"));
        assert_eq!(b["questions"].as_array().unwrap().len(), 2);
    }
    // Die Seitenfrage hat keinen Eintrag für Bilder.
    assert!(
        os_store::ai::answers::get(file.conn(), 1, "soft-404", "1.0.0", "")
            .unwrap()
            .is_none()
    );
    assert_eq!(logged(file.conn())[0].record.task, "decision-vision");
}

// spec: AI-033 [M4]
// Voranschlag: 5 aktive Seitenfragen gebündelt in 1 Request je URL → `Est. Input Tokens` = Summe der 5
// Einzelschätzungen nach AI-012 (Nachtrag); „max. 1 Frage je Request“ → gleiche Tokenschätzung, 5-fache Requestzahl.
#[tokio::test]
async fn estimate_per_question_is_independent_of_bundling() {
    let env = Env::new(&["openai"]).await;
    let qs = numbered(5);
    let items = vec![text_item(1, "Text"), text_item(2, "Mehr Text")];
    let dir = TempDir::new("est");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let estimate = |max: u8| {
        env.run(
            file.conn(),
            run,
            &qs,
            &items,
            &ledger,
            RunControl::new(),
            RunOptions {
                max_questions: MaxQuestionsPerRequest::new(max).unwrap(),
                ..RunOptions::default()
            },
        )
        .estimate()
        .unwrap()
    };
    let bundled = estimate(10);
    let single = estimate(1);
    assert_eq!(bundled.tasks.len(), 1);
    let (b, s) = (&bundled.tasks[0], &single.tasks[0]);
    assert_eq!(b.requests, 2);
    assert_eq!(s.requests, 10);
    assert_eq!(b.estimate.items, 10);
    assert_eq!(b.estimate.est_input_tokens, s.estimate.est_input_tokens);
    assert_eq!(b.estimate.est_output_tokens, 0);
    assert_eq!(b.reasoning, None);
    // Summe der Einzelschätzungen je Frage nach AI-012 (Nachtrag).
    let one_question = qs[0].def.wire_json();
    let z_q = os_ai::cost::question_text_chars(&one_question);
    let g = os_ai::cost::decisions::bundled_question_overhead();
    let tokens = |text: &str| {
        let chars = os_ai::cost::tokens::char_count(text);
        5 * os_ai::cost::decisions::question_input_tokens(chars, 0, 0, z_q, g)
    };
    assert_eq!(
        b.estimate.est_input_tokens,
        tokens("Title: Seite 1") + tokens("Title: Seite 2")
    );
    assert_eq!(
        env.mock.posts(DECISIONS).len(),
        0,
        "der Voranschlag sendet nichts"
    );
}

// spec: AI-012 [M4]
// Kostenbestätigung: Voranschlag über der Schwelle → Einträge `Pending (confirmation)`, kein Request; „Starten“
// (bestätigt) sendet sie.
#[tokio::test]
async fn confirmation_above_threshold_holds_the_entries() {
    let env = Env::new(&["openai"]).await;
    let pt = question("page-type");
    let qs = [active(pt.clone())];
    let items = vec![text_item(1, "Text"), text_item(2, "Text 2")];
    let dir = TempDir::new("confirm");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let mut options = RunOptions {
        confirmation: Confirmation::AskAbove(os_model::budget::MicroUsd(1)),
        ..RunOptions::default()
    };
    let report = env
        .run(
            file.conn(),
            run,
            &qs,
            &items,
            &ledger,
            RunControl::new(),
            options.clone(),
        )
        .run()
        .await
        .unwrap();
    assert_eq!(report.end, RunEnd::NeedsConfirmation);
    assert_eq!(report.estimate.as_ref().unwrap().tasks[0].estimate.items, 2);
    assert_eq!(
        count(&counts(file.conn(), &pt), "Pending (confirmation)"),
        2
    );
    assert_eq!(env.mock.posts(DECISIONS).len(), 0);
    options.confirmation = Confirmation::Granted;
    let report = env
        .run(
            file.conn(),
            run,
            &qs,
            &items,
            &ledger,
            RunControl::new(),
            options,
        )
        .run()
        .await
        .unwrap();
    assert_eq!(report.end, RunEnd::Completed);
    assert_eq!(count(&counts(file.conn(), &pt), "Success"), 2);
    assert_eq!(env.mock.posts(DECISIONS).len(), 2);
}

fn site_run<'a>(
    env: &'a Env,
    conn: &'a os_store::ai::cache::Connection,
    run: i64,
    qs: &'a [ActiveQuestion],
    items: &'a [RunItem],
    ledger: &'a os_model::budget::CrawlBudgetLedger,
) -> DecisionRun<'a> {
    env.run(
        conn,
        run,
        qs,
        items,
        ledger,
        RunControl::new(),
        RunOptions::default(),
    )
}

// spec: AI-020 [M4]
// Teilfehler: Seitentyp-Frage auf `site-partial/` → 19 `Success`, 2 `Error: 500 …`, 1 `Refusal`, 3 `Skipped (scope)`;
// Mock-Log 19 + 1 + 2 × 6 = 32 Requests; der Lauf endet regulär.
#[tokio::test]
async fn partial_failures_on_site_partial() {
    let env = Env::new(&["openai"]).await;
    let pt = question("page-type");
    let qs = [active(pt.clone())];
    let items = site_items("site-partial");
    assert_eq!(items.len(), 25);
    let dir = TempDir::new("partial");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let report = site_run(&env, file.conn(), run, &qs, &items, &ledger)
        .run()
        .await
        .unwrap();
    assert_eq!(report.end, RunEnd::Completed);
    let c = counts(file.conn(), &pt);
    assert_eq!(count(&c, "Success"), 19, "{c:?}");
    assert_eq!(count(&c, "Refusal"), 1, "{c:?}");
    assert_eq!(count(&c, "Skipped (scope)"), 3, "{c:?}");
    let errors: u64 = c
        .iter()
        .filter(|(s, _)| s.starts_with("Error: 500"))
        .map(|(_, n)| n)
        .sum();
    assert_eq!(errors, 2, "{c:?}");
    assert_eq!(env.mock.posts(DECISIONS).len(), 32);
    // Jede Statusspalte enthält nur Werte aus der Tabelle; Filter „Errors“ = genau die Fehler.
    let rows = os_store::ai::answers::list_for_question(file.conn(), &pt.id, None).unwrap();
    assert_eq!(rows.len(), 25);
    let errors = rows
        .iter()
        .filter(|r| os_ai::decisions::exec::is_error_status(&r.status))
        .count();
    assert_eq!(errors, 2);
    let log = logged(file.conn());
    let failed: Vec<_> = log
        .iter()
        .filter(|r| r.record.status.starts_with("Error: 500"))
        .collect();
    assert_eq!(failed.len(), 2);
    assert!(failed.iter().all(|r| r.record.retries == 5));
}

// spec: AI-020 [M4]
// Fehlerfall-Matrix, fehlender Schlüssel: kein Anbieter verbunden → 22 × `Error: Missing API key`, 0 Requests;
// 3 × `Skipped (scope)`.
#[tokio::test]
async fn matrix_missing_key() {
    let env = Env::new(&[]).await;
    let pt = question("page-type");
    let qs = [active(pt.clone())];
    let items = site_items("site");
    let dir = TempDir::new("nokey");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    site_run(&env, file.conn(), run, &qs, &items, &ledger)
        .run()
        .await
        .unwrap();
    let c = counts(file.conn(), &pt);
    assert_eq!(count(&c, "Error: Missing API key"), 22, "{c:?}");
    assert_eq!(count(&c, "Skipped (scope)"), 3);
    assert_eq!(env.mock.log().len(), 0);
}

// spec: AI-020 [M4]
// Fehlerfall-Matrix, Kontingent: `insufficient-quota` (OpenAI) → 1 Request (gleichzeitige Anfragen = 1),
// 22 × `Quota exceeded`.
#[tokio::test]
async fn matrix_quota() {
    let env = Env::new(&["openai"]).await;
    env.mock.scenario("openai", "insufficient-quota", 100).await;
    let pt = question("page-type");
    let qs = [active(pt.clone())];
    let items = site_items("site");
    let dir = TempDir::new("quota");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    site_run(&env, file.conn(), run, &qs, &items, &ledger)
        .run()
        .await
        .unwrap();
    let c = counts(file.conn(), &pt);
    assert_eq!(count(&c, "Quota exceeded"), 22, "{c:?}");
    assert_eq!(count(&c, "Skipped (scope)"), 3);
    assert_eq!(env.mock.posts(DECISIONS).len(), 1);
}

// spec: AI-020 [M4]
// Fehlerfall-Matrix, Budget: 0,0001 USD bei Kosten ≥ 0,0001 USD je Request → höchstens so viele `Success` wie das
// Budget deckt, Rest `Budget exceeded`; der Lauf endet regulär.
#[tokio::test]
async fn matrix_budget() {
    let env = Env::new(&["openai"]).await;
    let pt = question("page-type");
    let qs = [active(pt.clone())];
    let items = site_items("site");
    let dir = TempDir::new("budget");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 100);
    let report = site_run(&env, file.conn(), run, &qs, &items, &ledger)
        .run()
        .await
        .unwrap();
    assert_eq!(report.end, RunEnd::Completed);
    let c = counts(file.conn(), &pt);
    let success = count(&c, "Success");
    assert_eq!(success + count(&c, "Budget exceeded"), 22, "{c:?}");
    assert!(count(&c, "Budget exceeded") >= 1);
    assert_eq!(count(&c, "Skipped (scope)"), 3);
    let spent = os_store::ai::requests::total_cost(file.conn()).unwrap();
    assert!(spent.0 <= 100, "{spent:?}");
    assert_eq!(env.mock.posts(DECISIONS).len() as u64, success);
}

// spec: AI-020 [M4]
// Fehlerfall-Matrix, Timeout (`hang`): `Error: Timeout nach <n> s` (Format wie INTEG-179) nach allen Wiederholungen;
// im Test mit 1 s statt 30 s, damit der Lauf kurz bleibt.
#[tokio::test]
async fn matrix_timeout() {
    let mut env = Env::new(&["openai"]).await;
    env.limiter = limiter_with(Duration::from_secs(1));
    env.mock.scenario("openai", "hang", 100).await;
    let pt = question("page-type");
    let qs = [active(pt.clone())];
    let items = vec![text_item(1, "Text")];
    let (_d, file) = run_with(&env, &qs, &items, RunOptions::default()).await;
    assert_eq!(answer(&file, 1, &pt).status, "Error: Timeout nach 1 s");
    let log = logged(file.conn());
    assert_eq!(log[0].record.http_status, None);
    assert_eq!(log[0].record.cost.0, 0);
}

// spec: AI-020 [M4]
// Anbieter nicht erreichbar → nach 1 + 5 Versuchen `Error: Connection failed`; Protokollzeile mit leerem
// `HTTP Status`, `Error Code = transport/connect`, `Retries = 5`, `Cost = 0`.
#[tokio::test]
async fn unreachable_provider_ends_as_connection_failed() {
    let mut env = Env::new(&["openai"]).await;
    // Freier Port ohne Server.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    env.decisions = os_ai::adapters::openai_decisions::OpenAiDecisions::new(
        os_ai::adapters::openai_responses::OpenAiHttp::new(
            client(),
            os_ai::adapters::openai_responses::OpenAiConfig::new(
                os_ai::adapters::openai_responses::ApiKey::new(OPENAI_KEY),
            )
            .with_base_url(format!("http://127.0.0.1:{port}/v1")),
            os_ai::adapters::openai_responses::OpenAiAccount::new(),
        ),
    );
    let pt = question("page-type");
    let items = vec![text_item(1, "Text")];
    let (_d, file) = run_with(&env, &[active(pt.clone())], &items, RunOptions::default()).await;
    assert_eq!(answer(&file, 1, &pt).status, "Error: Connection failed");
    let log = logged(file.conn());
    assert_eq!(log.len(), 1);
    let r = &log[0].record;
    assert_eq!(r.http_status, None);
    assert_eq!(r.error_code.as_deref(), Some("transport/connect"));
    assert_eq!(r.retries, 5);
    assert_eq!(r.cost.0, 0);
}
