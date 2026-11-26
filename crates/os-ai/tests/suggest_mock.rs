//! Vorschlagslauf gegen den ai-mock (OpenAI Responses, `tests/fixtures/ai/mock-answers.d/v01.json`): Rahmen (AI-070),
//! Title- und Meta-Description-Vorschläge (AI-071, AI-072), Prüfung (AI-075), Cache (AI-015), Budget (AI-013),
//! Einwilligung (AI-018) und Schalter „KI (Vorschau)“ (Plan M4 E1).
#[path = "suggest_support.rs"]
mod support;

use std::collections::BTreeMap;

use os_ai::tasks::suggest::{Element, RunError};
use os_ai::PreviewSwitch;
use os_model::budget::{BudgetLimits, MicroUsd};
use os_store::ai::suggestions::{self, ReviewStatus};
use support::*;

fn title_pages() -> Vec<FixturePage> {
    pages()
}

fn only(pages: &[os_ai::tasks::suggest::PageInput], paths: &[&str]) -> Vec<os_ai::tasks::suggest::PageInput> {
    pages
        .iter()
        .filter(|p| paths.iter().any(|path| p.address == format!("{BASE}{path}")))
        .cloned()
        .collect()
}

const TITLE_PATHS: [&str; 7] = ["/a/", "/b/", "/c/", "/d/", "/e/", "/f/", "/g/"];
const META_PATHS: [&str; 5] = ["/m1/", "/m2/", "/m3/", "/m4/", "/m5/"];

fn record(kit: &Kit, element: Element, path: &str) -> suggestions::ElementRecord {
    let id = title_pages()
        .iter()
        .position(|p| p.path == path)
        .unwrap() as i64
        + 1;
    kit.store
        .with(|f| suggestions::load(f.conn(), id, element.id()).unwrap())
        .unwrap()
}

// spec: AI-070 [M4]
// spec: AI-071 [M4]
// Je URL und Element genau ein Request (alle Kandidaten der Fixture sind gültig bzw. ein gültiger bleibt);
// Kandidaten in der Reihenfolge der Antwort; Fokus-Begriff nur im Request der URL mit Eintrag (Mock-Log).
#[tokio::test]
async fn one_request_per_url_candidates_in_answer_order_and_focus_only_where_imported() {
    let dir = TempDir::new("mock-titles");
    let fixture = title_pages();
    let file = crawl_file(&dir.0, &fixture);
    let kit = Kit::new(file, Element::Title, &fixture, Kit::unlimited());
    let mock = Mock::start().await;
    let provider = mock.provider();
    let focus = BTreeMap::from([(format!("{BASE}/a/"), "wanderschuhe damen".to_string())]);
    let all = inputs(&fixture, &focus);
    let pages = only(&all, &TITLE_PATHS);
    let summary = kit
        .engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    assert_eq!(summary.entries, 7);
    assert_eq!(summary.requests, 7, "genau ein Request je URL: {summary:?}");
    assert_eq!(mock.responses().len(), 7);
    let bodies = mock.bodies();
    let with_focus: Vec<&serde_json::Value> = bodies
        .iter()
        .filter(|b| b.to_string().contains("wanderschuhe damen"))
        .collect();
    assert_eq!(with_focus.len(), 1);
    assert!(with_focus[0].to_string().contains("MOCK-ID:v01-a"));
    // Kandidaten in Antwortreihenfolge gespeichert.
    let a = record(&kit, Element::Title, "/a/");
    assert_eq!(
        a.candidates.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(),
        vec![
            "Leichte Wanderschuhe für Damen und Herren",
            "Wanderschuhe im Test: die besten Modelle 2026",
            "Wanderschuhe für jeden Weg – Ratgeber und Test"
        ]
    );
    assert_eq!(a.status, "Valid");
    assert_eq!(a.focus.as_deref(), Some("wanderschuhe damen"));
    assert!(!a.provisional, "nach dem Nachlauf nicht mehr vorläufig");
    // Jede Protokollzeile trägt Aufgabe, Stufe, Kosten > 0 und Status Success.
    let log = kit.log("suggest-title");
    assert_eq!(log.len(), 7);
    assert!(log.iter().all(|r| r.record.status == "Success" && r.record.cost > MicroUsd(0)));
    assert!(log
        .iter()
        .all(|r| r.details.reasoning_level.as_deref() == Some("off")));
    // Jeder Request trägt `store: false` und das Antwortschema (AI-003, AI-021).
    assert!(bodies.iter().all(|b| b["store"] == false));
    assert!(bodies
        .iter()
        .all(|b| b["text"]["format"]["schema"]["required"][0] == "candidates"));
}

// spec: AI-071 [M4]
// Seite ohne Title erhält Kandidaten (Title 1 bleibt leer); kein gültiger Kandidat gleicht dem Title einer anderen
// indexierbaren URL (Stufe 1, /d/ → Title von /e/); Nachlauf: /a/ behält „X“, bei /b/ trägt „X“
// `duplicate_candidate:http://ai.test:8080/a/` und „Y“ ist Kandidat 1.
#[tokio::test]
async fn titles_missing_title_stage_one_and_stage_two_duplicates() {
    let dir = TempDir::new("mock-dups");
    let fixture = title_pages();
    let file = crawl_file(&dir.0, &fixture);
    let kit = Kit::new(file, Element::Title, &fixture, Kit::unlimited());
    let mock = Mock::start().await;
    let provider = mock.provider();
    let pages = only(&inputs(&fixture, &BTreeMap::new()), &TITLE_PATHS);
    kit.engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    let c = record(&kit, Element::Title, "/c/");
    assert_eq!(c.status, "Valid");
    assert_eq!(c.candidates.len(), 3);
    let current: Option<String> = kit.store.with(|f| {
        f.conn()
            .query_row("SELECT title_1 FROM urls WHERE id = 3", [], |r| r.get(0))
            .unwrap()
    });
    assert_eq!(current, None, "Title 1 bleibt leer");
    let d = record(&kit, Element::Title, "/d/");
    assert_eq!(
        d.candidates[0].violations,
        vec!["duplicate_of:http://ai.test:8080/e/".to_string()]
    );
    assert_eq!(d.selected, Some(2));
    let a = record(&kit, Element::Title, "/a/");
    let b = record(&kit, Element::Title, "/b/");
    assert_eq!(a.selected_candidate().unwrap().value, "Leichte Wanderschuhe für Damen und Herren");
    assert_eq!(
        b.candidates[0].violations,
        vec!["duplicate_candidate:http://ai.test:8080/a/".to_string()]
    );
    assert_eq!(
        b.displayed()[0].value,
        "Rucksäcke für Wanderungen im großen Vergleich"
    );
    // Kein gültiger Kandidat gleicht dem Title einer anderen indexierbaren URL.
    let titles: Vec<String> = fixture
        .iter()
        .filter(|p| p.indexable)
        .filter_map(|p| p.title.clone())
        .map(|t| os_ai::tasks::suggest::normalized(&t))
        .collect();
    for path in TITLE_PATHS {
        let r = record(&kit, Element::Title, path);
        for cand in r.candidates.iter().filter(|c| c.valid) {
            assert!(!titles.contains(&os_ai::tasks::suggest::normalized(&cand.value)), "{path}: {}", cand.value);
        }
    }
}

// spec: AI-072 [M4]
// Meta Descriptions: Spalten analog (Prüfwerte gespeichert), Zeilenumbrüche und umschließende Anführungszeichen
// werden vor der Prüfung entfernt, Dubletten in zwei Stufen wie bei Titles.
#[tokio::test]
async fn meta_descriptions_are_cleaned_checked_and_deduplicated() {
    let dir = TempDir::new("mock-meta");
    let fixture = title_pages();
    let file = crawl_file(&dir.0, &fixture);
    let kit = Kit::new(file, Element::MetaDescription, &fixture, Kit::unlimited());
    let mock = Mock::start().await;
    let provider = mock.provider();
    let pages = only(&inputs(&fixture, &BTreeMap::new()), &META_PATHS);
    let summary = kit
        .engine(&provider)
        .run(&kit.job(Element::MetaDescription, &pages))
        .await
        .unwrap();
    assert_eq!(summary.requests, 5);
    let m3 = record(&kit, Element::MetaDescription, "/m3/");
    let first = &m3.candidates[0];
    assert!(!first.value.contains('\n') && !first.value.starts_with('"') && !first.value.ends_with('"'));
    assert!(first.valid, "{:?}", first.violations);
    assert_eq!(first.pixel_width, Some(fake_px(&first.value, 14.0)));
    assert_eq!(first.length, first.value.chars().count() as u32);
    let m5 = record(&kit, Element::MetaDescription, "/m5/");
    assert_eq!(
        m5.candidates[0].violations,
        vec!["duplicate_candidate:http://ai.test:8080/m4/".to_string()]
    );
    assert_eq!(m5.selected, Some(2));
    assert!(kit.log("suggest-meta").len() == 5);
}

// spec: AI-070 [M4]
// spec: AI-015 [M4]
// Vorschläge, Prüfstatus und Nutzerentscheidungen bleiben nach Speichern und Öffnen der Crawl-Datei erhalten; ein
// zweiter Lauf mit gleicher Eingabe sendet keinen Request (Cache), die Entscheidungen bleiben.
#[tokio::test]
async fn results_and_decisions_survive_reopen_and_a_second_run_hits_the_cache() {
    let dir = TempDir::new("mock-reopen");
    let fixture = title_pages();
    let file = crawl_file(&dir.0, &fixture);
    let path = file.path().to_path_buf();
    let kit = Kit::new(file, Element::Title, &fixture, Kit::unlimited());
    let mock = Mock::start().await;
    let provider = mock.provider();
    let pages = only(&inputs(&fixture, &BTreeMap::new()), &["/a/", "/c/"]);
    kit.engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    let mut a = record(&kit, Element::Title, "/a/");
    a.review = ReviewStatus::Rejected;
    kit.store
        .with(|f| suggestions::save(f.conn(), &a).unwrap());
    drop(kit);
    let reopened = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
    let back = suggestions::load(reopened.conn(), 1, "title").unwrap().unwrap();
    assert_eq!(back.review, ReviewStatus::Rejected);
    assert_eq!(back.candidates.len(), 3);
    assert_eq!(back.status, "Valid");
    let kit = Kit::new(reopened, Element::Title, &fixture, Kit::unlimited());
    let before = mock.responses().len();
    let summary = kit
        .engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    assert_eq!(mock.responses().len(), before, "zweiter Lauf: 0 Requests");
    assert_eq!(summary.cache_hits, 2);
    assert_eq!(record(&kit, Element::Title, "/a/").review, ReviewStatus::Rejected);
    let hits: Vec<_> = kit
        .log("suggest-title")
        .into_iter()
        .filter(|r| r.record.cache_hit)
        .collect();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|r| r.record.cost == MicroUsd(0) && r.record.status == "Success (cached)"));
}

// Plan M4 E1 (E-2 b): Mit Schalter „KI (Vorschau)“ aus geht kein Request hinaus, es entsteht keine Zeile.
// spec: AI-070 [M4]
#[tokio::test]
async fn preview_switch_off_sends_nothing_and_writes_nothing() {
    let dir = TempDir::new("mock-off");
    let fixture = title_pages();
    let file = crawl_file(&dir.0, &fixture);
    let kit = Kit::new(file, Element::Title, &fixture, Kit::unlimited());
    let mock = Mock::start().await;
    let provider = mock.provider();
    let pages = only(&inputs(&fixture, &BTreeMap::new()), &TITLE_PATHS);
    let mut engine = kit.engine(&provider);
    engine.switch = PreviewSwitch::OFF;
    assert_eq!(
        engine.run(&kit.job(Element::Title, &pages)).await,
        Err(RunError::PreviewOff)
    );
    assert!(mock.responses().is_empty());
    assert!(kit
        .store
        .with(|f| suggestions::load_all(f.conn(), "title").unwrap())
        .is_empty());
}

// spec: AI-018 [M4]
// spec: AI-020 [M4]
// Ohne Einwilligung kein Request: Einträge `Pending (consent)`, 0 Requests, keine Kosten, keine offene Reservierung.
#[tokio::test]
async fn without_consent_entries_wait_for_consent_and_nothing_is_sent() {
    let dir = TempDir::new("mock-consent");
    let fixture = title_pages();
    let file = crawl_file(&dir.0, &fixture);
    let mut kit = Kit::new(file, Element::Title, &fixture, Kit::unlimited());
    kit.consent = consent(&[]);
    let mock = Mock::start().await;
    let provider = mock.provider();
    let pages = only(&inputs(&fixture, &BTreeMap::new()), &TITLE_PATHS);
    let summary = kit
        .engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    assert!(mock.responses().is_empty());
    assert_eq!(summary.statuses.get("Pending (consent)"), Some(&7));
    let snapshot = os_model::budget::BudgetLedger::snapshot(&kit.ledger).unwrap();
    assert_eq!(snapshot.reserved, MicroUsd(0));
    assert!(kit.log("suggest-title").is_empty());
}

// spec: AI-013 [M4]
// spec: AI-020 [M4]
// Budget, das nur wenige Requests deckt → höchstens so viele Ergebnisse, Rest `Budget exceeded`; nie mehr Kosten
// als das Budget.
#[tokio::test]
async fn a_small_budget_ends_further_entries_with_budget_exceeded() {
    let dir = TempDir::new("mock-budget");
    let fixture = title_pages();
    let file = crawl_file(&dir.0, &fixture);
    let limits = BudgetLimits {
        crawl: MicroUsd(600),
        ..BudgetLimits::default()
    };
    let mut kit = Kit::new(file, Element::Title, &fixture, limits);
    kit.limiter = limiter(1);
    let mock = Mock::start().await;
    let provider = mock.provider();
    let pages = only(&inputs(&fixture, &BTreeMap::new()), &TITLE_PATHS);
    let mut engine = kit.engine(&provider);
    engine.workers = 1;
    let summary = engine.run(&kit.job(Element::Title, &pages)).await.unwrap();
    let exceeded = summary.statuses.get("Budget exceeded").copied().unwrap_or(0);
    assert!(exceeded >= 1, "{summary:?}");
    assert_eq!(mock.responses().len(), 7 - exceeded);
    let spent: u64 = kit.log("suggest-title").iter().map(|r| r.record.cost.0).sum();
    assert!(spent <= 600, "{spent}");
}
