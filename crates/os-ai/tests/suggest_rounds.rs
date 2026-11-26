//! Validierungsschleife (AI-075), Dublettenprüfung mit Nachlauf und Determinismus (AI-071), Reparaturversuch
//! (AI-021), Ausgabegrenze (AI-020), Refusal (AI-038), Stoppen und Atomarität (AI-020, Nachtrag 26.11.2026) mit einem
//! skriptbaren Anbieter (Rundenfolgen und Verzögerungen ohne HTTP).
#[path = "suggest_support.rs"]
mod support;

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use os_ai::provider::ProviderError;
use os_ai::tasks::suggest::{
    finish_without_request, revalidate, Checker, Element, ElementLimits, PageInput, RuleSet,
    SuggestStore,
};
use os_model::ai::AiStatus;
use os_model::budget::{BudgetLedger, MicroUsd};
use os_store::ai::suggestions::{self, ElementRecord};
use serde_json::json;
use support::*;

fn two_pages() -> Vec<FixturePage> {
    let mut all = pages();
    all.retain(|p| p.path == "/a/" || p.path == "/b/");
    all
}

fn record_of(kit: &Kit, url_id: i64) -> ElementRecord {
    kit.store
        .with(|f| suggestions::load(f.conn(), url_id, "title").unwrap())
        .unwrap()
}

fn page_inputs(fixture: &[FixturePage]) -> Vec<PageInput> {
    inputs(fixture, &BTreeMap::new())
}

// spec: AI-075 [M4]
// Erster Versuch 612, 580, 590 px, zweiter 540 px → `Valid`, gültiger Kandidat 540 px, 2 Requests im Protokoll, der
// zweite Request enthält die drei Verstöße.
#[tokio::test]
async fn invalid_first_round_triggers_one_repair_round_with_the_concrete_violations() {
    let dir = TempDir::new("rounds-repair");
    let fixture = two_pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    let round1 = [
        "Wanderschuhe im Test mit langen Namen Eins",
        "Wanderschuhe im Test mit langen Namen Zwei",
        "Wanderschuhe im Test mit langen Namen Drei",
    ];
    let round2 = "Wanderschuhe im Test: die besten Modelle";
    let widths: HashMap<&str, u32> = HashMap::from([
        (round1[0], 612),
        (round1[1], 580),
        (round1[2], 590),
        (round2, 540),
    ]);
    let meter = move |t: &str, s: f64| -> u32 { widths.get(t).copied().unwrap_or_else(|| fake_px(t, s)) };
    let provider = Scripted::new().answer(
        "v01-a",
        vec![candidates(&round1), candidates(&[round2])],
    );
    let mut engine = kit.engine(&provider);
    engine.meter = &meter;
    let pages: Vec<PageInput> = page_inputs(&fixture).into_iter().take(1).collect();
    engine.run(&kit.job(Element::Title, &pages)).await.unwrap();
    let r = record_of(&kit, 1);
    assert_eq!(r.status, "Valid");
    let chosen = r.selected_candidate().unwrap();
    assert_eq!((chosen.value.as_str(), chosen.pixel_width), (round2, Some(540)));
    assert_eq!(r.rounds, 2);
    assert_eq!(r.candidates.len(), 4, "alle Runden in Antwortreihenfolge");
    assert_eq!(kit.log("suggest-title").len(), 2);
    let calls = provider.calls();
    assert_eq!(calls.len(), 2);
    for v in ["612 px > 561 px", "580 px > 561 px", "590 px > 561 px"] {
        assert!(calls[1].1.contains(v), "{v} fehlt in {}", calls[1].1);
        assert!(!calls[0].1.contains(v));
    }
    assert!(calls[1].1.contains("Candidate 2"));
}

// spec: AI-075 [M4]
// Dauerhaft ungültige Kandidaten → nach 1 + 2 Requests `No valid suggestion`; sichtbar ist der Kandidat mit den
// wenigsten Verstößen (bei Gleichstand der erste) samt Verstoßliste.
#[tokio::test]
async fn permanently_invalid_candidates_end_after_three_requests_with_the_least_bad_visible() {
    let dir = TempDir::new("rounds-novalid");
    let fixture = two_pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    let long = "Wanderschuhe für Damen und Herren mit sehr vielen Details und Extras";
    let provider = Scripted::new().answer(
        "v01-a",
        vec![candidates(&[long, "Wanderschuhe Damen kurz", "Wanderschuhe Herren kurz"])],
    );
    let engine = kit.engine(&provider);
    let pages: Vec<PageInput> = page_inputs(&fixture).into_iter().take(1).collect();
    engine.run(&kit.job(Element::Title, &pages)).await.unwrap();
    let r = record_of(&kit, 1);
    assert_eq!(r.status, "No valid suggestion");
    assert_eq!(provider.count("v01-a"), 3);
    assert_eq!(kit.log("suggest-title").len(), 3);
    let visible = r.displayed()[0];
    assert_eq!(visible.value, "Wanderschuhe Damen kurz");
    assert_eq!(visible.violations, vec!["length<30".to_string()]);
    assert!(!visible.violations.is_empty());
    assert_eq!(r.violations, visible.violations);
    assert!(r.candidates.iter().all(|c| !c.valid));
}

// spec: AI-075 [M4]
// Grenzwertänderung in den Preferences (Title max. 50 Zeichen) → bestehende Kandidaten werden ohne Request neu
// geprüft; der Status ändert sich entsprechend.
#[tokio::test]
async fn changing_the_limit_rechecks_stored_candidates_without_a_request() {
    let dir = TempDir::new("rounds-recheck");
    let fixture = two_pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    let provider = Scripted::new().answer(
        "v01-a",
        vec![candidates(&["Wanderschuhe für Damen und Herren im großen Vergleich"])],
    );
    let engine = kit.engine(&provider);
    let pages: Vec<PageInput> = page_inputs(&fixture).into_iter().take(1).collect();
    engine.run(&kit.job(Element::Title, &pages)).await.unwrap();
    assert_eq!(record_of(&kit, 1).status, "Valid");
    let calls = provider.calls().len();
    let checker = Checker {
        element: Element::Title,
        limits: ElementLimits {
            max_chars: 50,
            ..ElementLimits::from_config(Element::Title, &os_model::CrawlConfig::default())
        },
        rules: RuleSet::default(),
        meter: &fake_px,
        index: &kit.index,
        style: &kit.style,
    };
    let records = kit.store.load_all(Element::Title).await.unwrap();
    let languages = BTreeMap::from([(1, Some("de".to_string()))]);
    let rechecked = revalidate(records, &checker, &languages, &kit.index);
    kit.store.save_all(rechecked).await.unwrap();
    let r = record_of(&kit, 1);
    assert_eq!(r.status, "No valid suggestion");
    assert_eq!(r.violations, vec!["length>50".to_string()]);
    assert_eq!(provider.calls().len(), calls, "keine Requests");
    // Zurück auf 60 → wieder gültig.
    let checker = Checker {
        limits: ElementLimits::from_config(Element::Title, &os_model::CrawlConfig::default()),
        ..checker
    };
    let records = kit.store.load_all(Element::Title).await.unwrap();
    kit.store
        .save_all(revalidate(records, &checker, &languages, &kit.index))
        .await
        .unwrap();
    assert_eq!(record_of(&kit, 1).status, "Valid");
}

fn dedupe_outcome(kit: &Kit) -> Vec<(String, Vec<Vec<String>>, Option<u32>)> {
    [1, 2]
        .iter()
        .map(|id| {
            let r = record_of(kit, *id);
            (
                r.status.clone(),
                r.candidates.iter().map(|c| c.violations.clone()).collect(),
                r.selected,
            )
        })
        .collect()
}

// spec: AI-071 [M4]
// /a/ und /b/ liefern denselben ersten Kandidaten „X“ (/b/ als zweiten „Y“) → nach dem Nachlauf ist bei /a/ „X“
// Kandidat 1, bei /b/ trägt „X“ `duplicate_candidate:http://ai.test:8080/a/` und „Y“ ist Kandidat 1. In 20
// Wiederholungen mit 4 Workern und zufälliger Verzögerung (Seed) identisch, auch wenn /b/ vor /a/ fertig wird.
#[tokio::test(start_paused = true)]
async fn stage_two_is_deterministic_across_workers_and_completion_order() {
    let x = "Leichte Wanderschuhe für Damen und Herren";
    let y = "Rucksäcke für Wanderungen im großen Vergleich";
    let mut seen = Vec::new();
    let mut seed: u64 = 0x5eed_0071;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % 2_000
    };
    for run in 0..21 {
        let dir = TempDir::new(&format!("rounds-det-{run}"));
        let fixture = two_pages();
        let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
        // Lauf 20: /a/ sicher langsamer als /b/ (b wird zuerst fertig).
        let (da, db) = if run == 20 {
            (Duration::from_millis(1_500), Duration::from_millis(1))
        } else {
            (Duration::from_millis(next()), Duration::from_millis(next()))
        };
        let provider = Scripted::new()
            .answer("v01-a", vec![candidates(&[x, "Wanderschuhe im Test: die besten Modelle 2026"])])
            .answer("v01-b", vec![candidates(&[x, y])])
            .delay("v01-a", da)
            .delay("v01-b", db);
        kit.engine(&provider)
            .run(&kit.job(Element::Title, &page_inputs(&fixture)))
            .await
            .unwrap();
        let a = record_of(&kit, 1);
        let b = record_of(&kit, 2);
        assert_eq!(a.displayed()[0].value, x);
        assert_eq!(
            b.candidates[0].violations,
            vec!["duplicate_candidate:http://ai.test:8080/a/".to_string()]
        );
        assert_eq!(b.displayed()[0].value, y);
        seen.push(dedupe_outcome(&kit));
    }
    assert!(seen.windows(2).all(|w| w[0] == w[1]), "nicht deterministisch");
}

// spec: AI-071 [M4]
// Hat /b/ nur den doppelten Kandidaten, endet /b/ mit `No valid suggestion`; der Nachlauf sendet keinen Request.
#[tokio::test]
async fn only_duplicate_candidate_ends_without_a_further_request() {
    let dir = TempDir::new("rounds-onlydup");
    let fixture = two_pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    let x = "Leichte Wanderschuhe für Damen und Herren";
    let provider = Scripted::new()
        .answer("v01-a", vec![candidates(&[x])])
        .answer("v01-b", vec![candidates(&[x])]);
    kit.engine(&provider)
        .run(&kit.job(Element::Title, &page_inputs(&fixture)))
        .await
        .unwrap();
    assert_eq!(provider.calls().len(), 2, "je URL ein Request, der Nachlauf sendet nichts");
    let b = record_of(&kit, 2);
    assert_eq!(b.status, "No valid suggestion");
    assert_eq!(
        b.violations,
        vec!["duplicate_candidate:http://ai.test:8080/a/".to_string()]
    );
    assert_eq!(record_of(&kit, 1).status, "Valid");
}

// spec: AI-021 [M4]
// Schemaverstoß einmal, danach gültig → 2 Requests für den Eintrag, der erste mit `Error: Invalid output` und der
// Meldung des Validators im Protokoll.
#[tokio::test]
async fn schema_violation_is_repaired_once() {
    let dir = TempDir::new("rounds-schema");
    let fixture = two_pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    let provider = Scripted::new().answer(
        "v01-a",
        vec![
            Ok(json!({"unexpected": ["not the requested schema"]})),
            candidates(&["Leichte Wanderschuhe für Damen und Herren"]),
        ],
    );
    let pages: Vec<PageInput> = page_inputs(&fixture).into_iter().take(1).collect();
    kit.engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    let log = kit.log("suggest-title");
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].record.status, "Error: Invalid output");
    assert!(log[0].record.error_message.as_deref().unwrap_or("").contains("candidates"));
    assert_eq!(log[1].record.status, "Success");
    assert_eq!(record_of(&kit, 1).status, "Valid");
    assert!(provider.calls()[1].1.contains("rejected by the JSON schema validator"));
}

// spec: AI-020 [M4]
// spec: AI-021 [M4]
// Abbruch an der Ausgabegrenze → genau 1 Request, `Error: Output limit reached`, kein Reparaturversuch; die
// Protokollzeile trägt `HTTP Status = 200` und den Grund als `Error Code`.
#[tokio::test]
async fn output_limit_ends_the_entry_without_repair() {
    let dir = TempDir::new("rounds-limit");
    let fixture = two_pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    let provider = Scripted::new().answer(
        "v01-a",
        vec![Err(ProviderError::output_limit("max_output_tokens"))],
    );
    let pages: Vec<PageInput> = page_inputs(&fixture).into_iter().take(1).collect();
    kit.engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    assert_eq!(provider.calls().len(), 1);
    assert_eq!(record_of(&kit, 1).status, "Error: Output limit reached");
    let log = kit.log("suggest-title");
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].record.http_status, Some(200));
    assert_eq!(log[0].record.error_code.as_deref(), Some("max_output_tokens"));
    let snapshot = kit.ledger.snapshot().unwrap();
    assert_eq!(snapshot.reserved, MicroUsd(0));
}

// spec: AI-038 [M4]
// spec: AI-015 [M4]
// Refusal → Status `Refusal`, keine Reparaturrunde; gecacht mit Kennzeichen, der nächste Lauf fragt nicht erneut,
// „Refusals erneut versuchen“ schon.
#[tokio::test]
async fn refusal_is_final_cached_and_retried_only_on_request() {
    let dir = TempDir::new("rounds-refusal");
    let fixture = two_pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    let provider = Scripted::new().answer("v01-a", vec![Ok(json!("REFUSAL"))]);
    let pages: Vec<PageInput> = page_inputs(&fixture).into_iter().take(1).collect();
    kit.engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    assert_eq!(record_of(&kit, 1).status, "Refusal");
    assert_eq!(provider.calls().len(), 1);
    kit.engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    assert_eq!(provider.calls().len(), 1, "gecachtes Refusal");
    let mut engine = kit.engine(&provider);
    engine.cache.retry_refusals = true;
    engine.run(&kit.job(Element::Title, &pages)).await.unwrap();
    assert_eq!(provider.calls().len(), 2);
}

// spec: AI-020 [M4]
// Nachtrag 26.11.2026 (K4-13): Nach dem Stoppen geht kein weiterer Request hinaus; der laufende wird übernommen,
// offene Einträge bleiben `Pending`, keine offene Reservierung, kein Nachlauf.
#[tokio::test]
async fn stop_sends_no_further_request_and_leaves_open_entries_pending() {
    let dir = TempDir::new("rounds-stop");
    let fixture = pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    let provider = Scripted::new()
        .answer("v01-a", vec![candidates(&["Leichte Wanderschuhe für Damen und Herren"])])
        .delay("v01-a", Duration::from_millis(50));
    let mut engine = kit.engine(&provider);
    engine.workers = 1;
    let all = page_inputs(&fixture);
    let pages: Vec<PageInput> = all.iter().take(4).cloned().collect();
    let job = kit.job(Element::Title, &pages);
    let stopper = async {
        while provider.calls().is_empty() {
            tokio::task::yield_now().await;
        }
        kit.control.stop();
    };
    let (summary, ()) = tokio::join!(engine.run(&job), stopper);
    let summary = summary.unwrap();
    assert!(summary.stopped);
    assert_eq!(provider.calls().len(), 1);
    assert_eq!(record_of(&kit, 1).status, "Valid", "laufender Request übernommen");
    assert!(record_of(&kit, 1).provisional, "ohne Nachlauf vorläufig");
    for id in 2..=4 {
        assert_eq!(record_of(&kit, id).status, "Pending");
    }
    assert_eq!(kit.ledger.snapshot().unwrap().reserved, MicroUsd(0));
    assert_eq!(kit.log("suggest-title").len(), 1);
}

/// Speicher, dessen Commit für eine URL scheitert (Fehlerinjektion zwischen Antwort und Commit).
struct Failing<'a> {
    inner: &'a dyn SuggestStore,
    fail_url: i64,
}

impl SuggestStore for Failing<'_> {
    fn cached(&self, key: String) -> os_ai::tasks::suggest::run::StoreFuture<'_, Option<os_ai::tasks::suggest::CachedAnswer>> {
        self.inner.cached(key)
    }
    fn commit(&self, commit: suggestions::SuggestionCommit) -> os_ai::tasks::suggest::run::StoreFuture<'_, Vec<i64>> {
        if commit.record.url_id == self.fail_url && !commit.requests.is_empty() {
            return Box::pin(async { Err("injected".to_string()) });
        }
        self.inner.commit(commit)
    }
    fn load_all(&self, element: Element) -> os_ai::tasks::suggest::run::StoreFuture<'_, Vec<ElementRecord>> {
        self.inner.load_all(element)
    }
    fn save_all(&self, records: Vec<ElementRecord>) -> os_ai::tasks::suggest::run::StoreFuture<'_, ()> {
        self.inner.save_all(records)
    }
    fn mark_pending(&self, element: Element, url_ids: Vec<i64>, run_id: i64) -> os_ai::tasks::suggest::run::StoreFuture<'_, ()> {
        self.inner.mark_pending(element, url_ids, run_id)
    }
}

// spec: AI-020 [M4]
// Atomarität: Scheitert der Commit nach dem Empfang, gibt es für den Eintrag weder Ergebnis noch Cache-Eintrag noch
// Protokollzeile noch verbuchte Kosten; ohne Injektion alle vier.
#[tokio::test]
async fn a_failed_commit_leaves_no_trace_of_the_answer() {
    let dir = TempDir::new("rounds-atomic");
    let fixture = two_pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    let provider = Scripted::new()
        .answer("v01-a", vec![candidates(&["Leichte Wanderschuhe für Damen und Herren"])])
        .answer("v01-b", vec![candidates(&["Rucksäcke für Wanderungen im großen Vergleich"])]);
    let failing = Failing {
        inner: &kit.store,
        fail_url: 1,
    };
    let mut engine = kit.engine(&provider);
    engine.store = &failing;
    engine
        .run(&kit.job(Element::Title, &page_inputs(&fixture)))
        .await
        .unwrap();
    let a = record_of(&kit, 1);
    assert_eq!(a.status, "Pending");
    assert!(a.candidates.is_empty());
    let log = kit.log("suggest-title");
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].record.url_id, Some(2));
    let snapshot = kit.ledger.snapshot().unwrap();
    assert_eq!(snapshot.reserved, MicroUsd(0));
    assert_eq!(snapshot.spent, log[0].record.cost);
    let cached: i64 = kit.store.with(|f| {
        f.conn()
            .query_row("SELECT count(*) FROM ai_cache", [], |r| r.get(0))
            .unwrap()
    });
    assert_eq!(cached, 1, "nur der Cache-Eintrag von /b/");
    assert_eq!(record_of(&kit, 2).status, "Valid");
}

// spec: AI-020 [M4]
// spec: AI-011 [M4]
// Fehlender Schlüssel (Ergebnis der Modellwahl) → alle Einträge `Error: Missing API key`, 0 Requests.
#[tokio::test]
async fn missing_key_marks_every_entry_without_a_request() {
    let dir = TempDir::new("rounds-nokey");
    let fixture = two_pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    finish_without_request(
        &kit.store,
        Element::Title,
        &page_inputs(&fixture),
        &AiStatus::MissingApiKey,
        1,
    )
    .await
    .unwrap();
    for id in [1, 2] {
        assert_eq!(record_of(&kit, id).status, "Error: Missing API key");
    }
    assert!(kit.log("suggest-title").is_empty());
}

// spec: AI-020 [M4]
// Jede Statusspalte enthält nur Werte aus der Tabelle (bzw. `Valid` / `No valid suggestion`, AI-071).
#[tokio::test]
async fn status_values_stay_within_the_spec_table() {
    let dir = TempDir::new("rounds-status");
    let fixture = pages();
    let kit = Kit::new(crawl_file(&dir.0, &fixture), Element::Title, &fixture, Kit::unlimited());
    let provider = Scripted::new()
        .answer("v01-a", vec![candidates(&["Leichte Wanderschuhe für Damen und Herren"])])
        .answer("v01-b", vec![Err(ProviderError::Http { status: 400, message: "bad".into() })])
        .answer("v01-c", vec![Ok(json!("REFUSAL"))])
        .answer("v01-d", vec![Err(ProviderError::QuotaExceeded)])
        .answer("v01-e", vec![candidates(&["Kurz"])]);
    let all = page_inputs(&fixture);
    let pages: Vec<PageInput> = all.into_iter().take(5).collect();
    let summary = kit
        .engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    let allowed = |s: &str| {
        s == "Valid"
            || s == "No valid suggestion"
            || serde_json::from_value::<AiStatus>(json!(s)).is_ok()
            || s.starts_with("Error: ")
    };
    for status in summary.statuses.keys() {
        assert!(allowed(status), "{status}");
    }
    assert_eq!(summary.statuses.get("Refusal"), Some(&1));
    assert_eq!(summary.statuses.get("Quota exceeded"), Some(&1));
    assert_eq!(summary.statuses.get("Error: 400 bad"), Some(&1));
}
