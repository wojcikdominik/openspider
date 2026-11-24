//! M4-K05: Kostenvoranschlag je Aufgabe, Kalibrierung, Cache-Zählung und Bestätigungsregel (AI-012).
#[path = "cost_support.rs"]
mod support;

use os_ai::cost::tokens::{char_count, estimate_input_tokens, text_tokens};
use os_ai::cost::{
    confirmation_required, cost_of, estimate_task, Calibration, EstimateItem, EstimateTotals,
    Executable, FeeCounts, NoCache, TaskEstimateInput, TokenCounts,
};
use os_model::budget::MicroUsd;
use std::collections::HashSet;
use support::{mock_input_tokens, openrouter_rates, site_pages, test_rates};

/// Anweisungstext der Seitentyp-Frage im Test (fester Anteil je Request).
const INSTRUCTIONS: &str = "Classify the page type of the given web page: home, category, product, article, contact, legal, search or other.";

fn items_for_scope() -> Vec<EstimateItem> {
    site_pages("site")
        .into_iter()
        .filter(|p| p.in_scope)
        .map(|p| EstimateItem {
            cache_key: Some(format!("page-type:{}", p.url)),
            input_chars: char_count(&p.text),
            images: 0,
        })
        .collect()
}

fn decision_input<'a>(
    items: &'a [EstimateItem],
    rates: Option<&'a os_ai::cost::Rates>,
    calibration: Calibration,
) -> TaskEstimateInput<'a> {
    TaskEstimateInput {
        task: "decision".into(),
        model: "gpt-6-luna".into(),
        items,
        instruction_tokens: text_tokens(char_count(INSTRUCTIONS)),
        expected_output_tokens: 20,
        rates,
        api_key_present: true,
        calibration,
        batch_capable: false,
    }
}

/// Ist-Kosten, wie sie ai-mock je Request meldet (Eingabe ⌈Zeichen/4⌉ einschließlich Anweisungen).
fn mock_actual(items: &[EstimateItem], rates: &os_ai::cost::Rates) -> (u64, Vec<(u64, u64)>) {
    let mut total = 0;
    let mut samples = Vec::new();
    for item in items {
        let actual_in = mock_input_tokens(item.input_chars + char_count(INSTRUCTIONS), item.images);
        let est_in = estimate_input_tokens(
            item.input_chars,
            item.images,
            rates.image_token_estimate,
            text_tokens(char_count(INSTRUCTIONS)),
        );
        samples.push((est_in, actual_in));
        total += cost_of(
            &TokenCounts::plain(actual_in, 20),
            FeeCounts::one_request(0),
            rates,
            false,
        )
        .cost
        .0;
    }
    (total, samples)
}

// spec: AI-012
#[test]
fn estimate_for_the_fixture_site_scope_and_calibration() {
    let items = items_for_scope();
    assert_eq!(
        items.len(),
        22,
        "|S| = 22; die 3 URLs außerhalb erscheinen nicht"
    );
    let rates = test_rates("openai", "gpt-6-luna", "decisions", false);
    let est = estimate_task(
        &decision_input(&items, Some(&rates), Calibration::UNCALIBRATED),
        &NoCache,
    );
    assert_eq!((est.items, est.cached), (22, 0));
    let (actual, samples) = mock_actual(&items, &rates);
    let expected = est.expected.unwrap().0 as f64;
    let deviation = (expected - actual as f64).abs() / actual as f64;
    assert!(deviation <= 0.25, "unkalibriert {deviation:.3} > 25 %");
    assert!(!est.calibrated);

    // Nach 20 protokollierten Requests: Korrekturfaktor = Median Ist/Schätzung.
    let cal = Calibration::from_samples(samples.iter().copied().take(20));
    assert!(cal.calibrated);
    let est = estimate_task(&decision_input(&items, Some(&rates), cal), &NoCache);
    let deviation = (est.expected.unwrap().0 as f64 - actual as f64).abs() / actual as f64;
    assert!(deviation <= 0.05, "kalibriert {deviation:.3} > 5 %");
    assert!(est.calibrated);
}

// spec: AI-012
#[test]
fn low_and_high_follow_the_calibration_state() {
    let items = items_for_scope();
    let rates = test_rates("anthropic", "mock-claude", "messages", false);
    let un = estimate_task(
        &decision_input(&items, Some(&rates), Calibration::UNCALIBRATED),
        &NoCache,
    );
    let e = un.expected.unwrap().0;
    assert_eq!(un.low.unwrap().0, (e * 3 + 2) / 4);
    assert_eq!(un.high.unwrap().0, (e * 5 + 2) / 4);
    let cal = Calibration {
        factor_ppm: 1_000_000,
        calibrated: true,
    };
    let ca = estimate_task(&decision_input(&items, Some(&rates), cal), &NoCache);
    let e = ca.expected.unwrap().0;
    assert_eq!(ca.low.unwrap().0, (e * 9 + 5) / 10);
    assert_eq!(ca.high.unwrap().0, (e * 11 + 5) / 10);
}

// spec: AI-012
#[test]
fn cached_entries_are_counted_and_cost_nothing() {
    let items = items_for_scope();
    let rates = test_rates("openai", "gpt-6-luna", "decisions", false);
    let all = estimate_task(
        &decision_input(&items, Some(&rates), Calibration::UNCALIBRATED),
        &NoCache,
    );
    let cache: HashSet<String> = items
        .iter()
        .take(5)
        .map(|i| i.cache_key.clone().unwrap())
        .collect();
    let partly = estimate_task(
        &decision_input(&items, Some(&rates), Calibration::UNCALIBRATED),
        &cache,
    );
    assert_eq!((partly.items, partly.cached), (22, 5));
    let rest = estimate_task(
        &decision_input(&items[5..], Some(&rates), Calibration::UNCALIBRATED),
        &NoCache,
    );
    assert_eq!(partly.expected, rest.expected, "Cache-Treffer tragen 0 bei");
    assert!(partly.expected < all.expected);
    let everything: HashSet<String> = items.iter().map(|i| i.cache_key.clone().unwrap()).collect();
    let none = estimate_task(
        &decision_input(&items, Some(&rates), Calibration::UNCALIBRATED),
        &everything,
    );
    assert_eq!(
        (none.cached, none.expected, none.est_input_tokens),
        (22, Some(MicroUsd(0)), 0)
    );
}

// spec: AI-012
#[test]
fn image_task_estimate_contains_request_and_image_fees() {
    let rates = openrouter_rates("mock/vision");
    let items: Vec<EstimateItem> = (0..3)
        .map(|i| EstimateItem {
            cache_key: Some(format!("img-{i}")),
            input_chars: 0,
            images: 1,
        })
        .collect();
    let input = TaskEstimateInput {
        task: "suggest-alt".into(),
        model: "mock/vision".into(),
        items: &items,
        instruction_tokens: 0,
        expected_output_tokens: 0,
        rates: Some(&rates),
        api_key_present: true,
        calibration: Calibration::UNCALIBRATED,
        batch_capable: false,
    };
    let est = estimate_task(&input, &NoCache);
    // 3 × (0,001 + 0,002) USD = 9.000 µUSD + Tokenkosten (3 × 85 Bild-Tokens × 1 µUSD).
    assert_eq!(est.est_input_tokens, 255);
    assert_eq!(est.expected, Some(MicroUsd(9_000 + 255)));
    // Gebühren gehen nicht in den Korrekturfaktor ein.
    let doubled = estimate_task(
        &TaskEstimateInput {
            calibration: Calibration {
                factor_ppm: 2_000_000,
                calibrated: true,
            },
            ..input.clone()
        },
        &NoCache,
    );
    assert_eq!(doubled.expected, Some(MicroUsd(9_000 + 510)));
}

// spec: AI-012
#[test]
fn tasks_without_key_or_price_are_not_executable_with_reason() {
    let items = items_for_scope();
    let rates = test_rates("openai", "gpt-6-luna", "decisions", false);
    let unknown = estimate_task(
        &decision_input(&items, None, Calibration::UNCALIBRATED),
        &NoCache,
    );
    assert_eq!(unknown.executable, Executable::PriceUnknown);
    assert_eq!(unknown.executable.reason(), "Price unknown");
    assert_eq!(
        (unknown.low, unknown.expected, unknown.high),
        (None, None, None)
    );
    let no_key = estimate_task(
        &TaskEstimateInput {
            api_key_present: false,
            ..decision_input(&items, Some(&rates), Calibration::UNCALIBRATED)
        },
        &NoCache,
    );
    assert_eq!(no_key.executable.reason(), "Missing API key");
    let ok = estimate_task(
        &decision_input(&items, Some(&rates), Calibration::UNCALIBRATED),
        &NoCache,
    );
    assert!(ok.executable.is_yes());
    // Die Summe enthält nur ausführbare Aufgaben.
    let totals = EstimateTotals::of_tasks(&[unknown, no_key, ok.clone()]);
    assert_eq!(totals.expected, ok.expected.unwrap());
}

// spec: AI-012
#[test]
fn batch_savings_only_for_batch_capable_tasks() {
    let items = items_for_scope();
    let rates = test_rates("openai", "gpt-6-luna", "responses", false);
    let sync = estimate_task(
        &decision_input(&items, Some(&rates), Calibration::UNCALIBRATED),
        &NoCache,
    );
    assert_eq!(sync.batch_savings, Some(MicroUsd(0)));
    let batch = estimate_task(
        &TaskEstimateInput {
            batch_capable: true,
            ..decision_input(&items, Some(&rates), Calibration::UNCALIBRATED)
        },
        &NoCache,
    );
    let savings = batch.batch_savings.unwrap().0 as i64;
    let half = batch.expected.unwrap().0 as i64 / 2;
    assert!(
        (savings - half).abs() <= 22,
        "50 % Rabatt, Rundung je Eintrag"
    );
}

// spec: AI-012
#[test]
fn confirmation_threshold_and_half_of_the_remaining_budget() {
    let usd = MicroUsd::USD;
    // Standard 1,00 USD; Testprofil 0,0001 USD.
    assert!(!confirmation_required(MicroUsd(usd), MicroUsd(usd), None));
    assert!(confirmation_required(
        MicroUsd(usd + 1),
        MicroUsd(usd),
        None
    ));
    assert!(confirmation_required(
        MicroUsd(101),
        MicroUsd(100),
        Some(MicroUsd(5 * usd))
    ));
    // 50 % des Restbudgets.
    assert!(!confirmation_required(
        MicroUsd(2_500),
        MicroUsd(usd),
        Some(MicroUsd(5_000))
    ));
    assert!(confirmation_required(
        MicroUsd(2_501),
        MicroUsd(usd),
        Some(MicroUsd(5_000))
    ));
    assert!(!confirmation_required(
        MicroUsd(0),
        MicroUsd(usd),
        Some(MicroUsd(0))
    ));
}

// spec: AI-012
#[test]
fn combined_summary_row_adds_dataforseo_min_and_max() {
    // Rechenregel der gemeinsamen Summenzeile (der Dialog selbst ist [M5]).
    let ai = EstimateTotals {
        low: MicroUsd(30_000),
        expected: MicroUsd(40_000),
        high: MicroUsd(50_000),
    };
    let sum = ai.with_dataforseo(MicroUsd(120_000), MicroUsd(240_000));
    assert_eq!(
        (sum.low, sum.expected, sum.high),
        (MicroUsd(150_000), MicroUsd(280_000), MicroUsd(290_000))
    );
}

// spec: AI-012
#[test]
fn calibration_uses_the_median_of_the_last_200_samples_from_20_on() {
    assert!(!Calibration::from_samples((0..19).map(|_| (100, 80))).calibrated);
    let c = Calibration::from_samples((0..20).map(|_| (100, 80)));
    assert_eq!((c.factor_ppm, c.calibrated), (800_000, true));
    // Nur die letzten 200 zählen: 300 alte Paare mit Faktor 2, danach 200 mit Faktor 0,5.
    let samples = (0..300)
        .map(|_| (100, 200))
        .chain((0..200).map(|_| (100, 50)));
    assert_eq!(Calibration::from_samples(samples).factor_ppm, 500_000);
    // Median bei gerader Anzahl; Schätzung 0 zählt nicht.
    let mut s: Vec<(u64, u64)> = (0..10).map(|_| (100, 90)).collect();
    s.extend((0..10).map(|_| (100, 110)));
    s.push((0, 5));
    assert_eq!(Calibration::from_samples(s).factor_ppm, 1_000_000);
    assert_eq!(
        Calibration {
            factor_ppm: 875_000,
            calibrated: true
        }
        .apply(100),
        88
    );
}

// spec: AI-012
#[test]
fn token_estimate_is_chars_over_3_5_plus_images_and_instructions() {
    assert_eq!(text_tokens(0), 0);
    assert_eq!(text_tokens(7), 2);
    assert_eq!(text_tokens(8), 3);
    assert_eq!(text_tokens(35), 10);
    assert_eq!(char_count("äöü"), 3, "Zeichen, nicht Bytes");
    assert_eq!(estimate_input_tokens(35, 2, 85, 30), 10 + 170 + 30);
}
