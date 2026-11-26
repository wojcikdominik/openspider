//! Prüfen, Übernehmen, Bearbeiten, Ablehnen und Massenaktion (AI-076) sowie Kostenvoranschlag (AI-012) der Vorschläge.
#[path = "suggest_support.rs"]
mod support;

use std::collections::BTreeMap;

use os_ai::cost::{Calibration, NoCache, PriceBasis};
use os_ai::tasks::suggest::estimate::{estimate, SuggestEstimate};
use os_ai::tasks::suggest::{
    accept_first, apply, check_text, Checker, Element, ElementLimits, NoStyleRules, PageInput,
    ReviewAction, ReviewError, RuleSet, SuggestOptions,
};
use os_model::CrawlConfig;
use os_store::ai::suggestions::{self, Origin, ReviewStatus};
use support::*;

fn checker<'a>(kit: &'a Kit) -> Checker<'a> {
    Checker {
        element: Element::Title,
        limits: ElementLimits::from_config(Element::Title, &CrawlConfig::default()),
        rules: RuleSet::default(),
        meter: &fake_px,
        index: &kit.index,
        style: &kit.style,
    }
}

/// Lauf über alle Fixture-Seiten mit Antworten je Seite: /a/ … /e/ gültig, /f/ … nur ungültig.
async fn prepared_kit(dir: &TempDir) -> (Kit, Vec<PageInput>) {
    let fixture = pages();
    let kit = Kit::new(
        crawl_file(&dir.0, &fixture),
        Element::Title,
        &fixture,
        Kit::unlimited(),
    );
    let mut provider = Scripted::new();
    for (i, p) in fixture.iter().enumerate() {
        provider = if i < 5 {
            provider.answer(
                &p.mock_id,
                vec![candidates(&[&format!(
                    "Gültiger Vorschlag für die Seite Nummer {i:02}"
                )])],
            )
        } else {
            provider.answer(&p.mock_id, vec![candidates(&["Zu kurz"])])
        };
    }
    let all = inputs(&fixture, &BTreeMap::new());
    let pages: Vec<PageInput> = all.into_iter().take(10).collect();
    kit.engine(&provider)
        .run(&kit.job(Element::Title, &pages))
        .await
        .unwrap();
    (kit, pages)
}

// spec: AI-076 [M4]
// Massenaktion „Kandidat 1 übernehmen“ auf 10 URLs übernimmt nur gültige Kandidaten; URLs mit `No valid suggestion`
// bleiben `Open` und werden gezählt gemeldet.
#[tokio::test]
async fn bulk_accept_takes_only_valid_first_candidates_and_counts_the_rest() {
    let dir = TempDir::new("review-bulk");
    let (kit, _) = prepared_kit(&dir).await;
    let mut records = kit
        .store
        .with(|f| suggestions::load_all(f.conn(), "title").unwrap());
    assert_eq!(records.len(), 10);
    let result = accept_first(&mut records, "2026-11-26T10:00:00Z");
    assert_eq!(result.accepted, 5);
    assert_eq!(result.skipped, 5);
    assert_eq!(result.skipped_url_ids, vec![6, 7, 8, 9, 10]);
    for r in &records {
        if r.url_id <= 5 {
            assert_eq!(r.review, ReviewStatus::Accepted);
            let a = r.accepted.as_ref().unwrap();
            assert_eq!(a.origin, Origin::Ai);
            assert_eq!(a.value, r.selected_candidate().unwrap().value);
        } else {
            assert_eq!(r.status, "No valid suggestion");
            assert_eq!(r.review, ReviewStatus::Open);
            assert!(r.accepted.is_none());
        }
    }
}

// spec: AI-076 [M4]
// Eine bearbeitete Übernahme, die eine Regel verletzt, wird gespeichert, ist aber `Edited` mit Verstoßliste (keine
// stille Korrektur); Live-Prüfung inkl. Pixelbreite; Ablehnen und Zurücksetzen.
#[tokio::test]
async fn edited_values_keep_their_violations_and_reject_reset_work() {
    let dir = TempDir::new("review-edit");
    let (kit, _) = prepared_kit(&dir).await;
    let c = checker(&kit);
    let mut r = kit
        .store
        .with(|f| suggestions::load(f.conn(), 1, "title").unwrap())
        .unwrap();
    let live = check_text(&c, 1, Some("de"), "Viel zu kurz");
    assert_eq!(
        live.violations,
        vec!["length<30".to_string(), "pixel_width<200".to_string()]
    );
    assert_eq!(live.pixel_width, fake_px("Viel zu kurz", 20.0));
    apply(
        &mut r,
        &ReviewAction::Edit {
            value: "Viel zu kurz".into(),
        },
        &c,
        Some("de"),
        "2026-11-26T10:00:00Z",
    )
    .unwrap();
    assert_eq!(r.review, ReviewStatus::Edited);
    let a = r.accepted.clone().unwrap();
    assert_eq!(a.value, "Viel zu kurz");
    assert_eq!(a.origin, Origin::Edited);
    assert_eq!(a.violations, live.violations);
    kit.store.with(|f| suggestions::save(f.conn(), &r).unwrap());
    let back = kit
        .store
        .with(|f| suggestions::load(f.conn(), 1, "title").unwrap())
        .unwrap();
    assert_eq!(back.accepted.unwrap().violations, live.violations);
    apply(&mut r, &ReviewAction::Reject, &c, None, "t").unwrap();
    assert_eq!(
        (r.review, r.accepted.is_none()),
        (ReviewStatus::Rejected, true)
    );
    apply(&mut r, &ReviewAction::Reset, &c, None, "t").unwrap();
    assert_eq!(r.review, ReviewStatus::Open);
    apply(
        &mut r,
        &ReviewAction::Accept { candidate: None },
        &c,
        None,
        "t",
    )
    .unwrap();
    assert_eq!(r.review, ReviewStatus::Accepted);
    // Ein ungültiger Kandidat lässt sich nicht als KI-Vorschlag übernehmen (nur bearbeitet).
    let mut bad = kit
        .store
        .with(|f| suggestions::load(f.conn(), 6, "title").unwrap())
        .unwrap();
    assert_eq!(
        apply(
            &mut bad,
            &ReviewAction::Accept { candidate: Some(1) },
            &c,
            None,
            "t"
        ),
        Err(ReviewError::NoValidCandidate)
    );
    assert_eq!(
        apply(
            &mut bad,
            &ReviewAction::Accept {
                candidate: Some(99)
            },
            &c,
            None,
            "t"
        ),
        Err(ReviewError::UnknownCandidate(99))
    );
}

// spec: AI-012 [M4]
// spec: AI-070 [M4]
// Zahlen für den Kostenvoranschlag (Dialog in M4-U01): je URL ein geplanter Request, Stufe `off`, Kosten mit den
// Testpreisen; ohne Schlüssel nicht ausführbar.
#[test]
fn estimate_plans_one_request_per_url_with_the_effective_level() {
    let fixture = pages();
    let pages: Vec<PageInput> = inputs(&fixture, &BTreeMap::new())
        .into_iter()
        .take(7)
        .collect();
    let rates = rates();
    let plan = plan(Element::Title, os_ai::provider::ReasoningLevel::Off);
    let red = redactor();
    let input = SuggestEstimate {
        element: Element::Title,
        pages: &pages,
        limits: ElementLimits::from_config(Element::Title, &CrawlConfig::default()),
        options: SuggestOptions::default(),
        plan: &plan,
        basis: Some(PriceBasis::Rates(&rates)),
        api_key_present: true,
        schema_overhead_tokens: 80,
        calibration: Calibration::UNCALIBRATED,
        output_calibration: Calibration::UNCALIBRATED,
        redactor: &red,
        style: &NoStyleRules,
    };
    let e = estimate(&input, &NoCache);
    assert_eq!(e.requests, 7);
    assert_eq!(e.estimate.items, 7);
    assert_eq!(e.estimate.task, "suggest-title");
    assert_eq!(e.reasoning, Some(os_ai::provider::ReasoningLevel::Off));
    assert!(e.estimate.expected.unwrap().0 > 0);
    assert!(matches!(
        e.estimate.executable,
        os_ai::cost::Executable::Yes
    ));
    let no_key = estimate(
        &SuggestEstimate {
            api_key_present: false,
            ..input
        },
        &NoCache,
    );
    assert!(matches!(
        no_key.estimate.executable,
        os_ai::cost::Executable::MissingApiKey
    ));
}
