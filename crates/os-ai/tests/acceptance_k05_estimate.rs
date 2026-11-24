//! Unabhängige Abnahmetests M4-K05: Kostenvoranschlag vor dem Lauf (AI-012).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` AI-012 (Token-Schätzung ⌈Zeichen / 3,5⌉ + Bild-Schätzwert +
//! fester Anteil für Anweisungen; Korrekturfaktor = Median Ist/Schätzung der letzten 200 protokollierten Requests
//! ab 20; niedrig/hoch × 0,75/1,25 unkalibriert, × 0,9/1,1 kalibriert; Cache-Treffer in `Cached`, Kosten 0;
//! Bildaufgabe 3 × (0,001 + 0,002) USD = 9.000 µUSD zuzüglich Tokenkosten; `Executable = nein` mit Grund
//! `Missing API key` / `Price unknown`; Bestätigung über der Schwelle oder über 50 % des Restbudgets, Standard
//! 1,00 USD), „Konventionen“ (`pricing-test.json`) und Auslegung (b) des Orchestrators (Gebühren gehen nicht in den
//! Korrekturfaktor ein).
// acceptance: independent

mod acceptance_k05_support;

use std::collections::BTreeSet;

use acceptance_k05_support::*;
use os_ai::cost::tokens::{char_count, estimate_input_tokens, text_tokens};
use os_ai::cost::{
    confirmation_required, estimate_task, request_record, Calibration, EstimateItem, Executable,
    NoCache, RequestOutcome, TaskEstimate, TaskEstimateInput,
};
use os_model::budget::MicroUsd;
use os_model::config::AiAppSettings;

fn claude() -> os_ai::cost::Rates {
    fixture_rates("anthropic", "mock-claude", "messages", false).unwrap()
}

fn items(n: usize, chars: u64, images: u64) -> Vec<EstimateItem> {
    (0..n)
        .map(|i| EstimateItem {
            cache_key: Some(format!("key-{i}")),
            input_chars: chars,
            images,
        })
        .collect()
}

fn input<'a>(
    items: &'a [EstimateItem],
    rates: Option<&'a os_ai::cost::Rates>,
    calibration: Calibration,
) -> TaskEstimateInput<'a> {
    TaskEstimateInput {
        task: "suggest-title".into(),
        model: "mock-claude".into(),
        items,
        instruction_tokens: 0,
        expected_output_tokens: 200,
        rates,
        api_key_present: true,
        calibration,
        batch_capable: false,
    }
}

fn calibrated(factor_ppm: u64) -> Calibration {
    Calibration {
        factor_ppm,
        calibrated: true,
    }
}

// spec: AI-012
// acceptance: independent
// Token-Schätzung ⌈Zeichen / 3,5⌉, Zeichen = Unicode-Zeichen.
#[test]
fn text_token_estimate_is_ceiling_of_chars_over_3_5() {
    for (chars, tokens) in [
        (0, 0),
        (1, 1),
        (3, 1),
        (4, 2),
        (7, 2),
        (8, 3),
        (35, 10),
        (36, 11),
        (3_500, 1_000),
        (3_501, 1_001),
        (1_000_000, 285_715),
    ] {
        assert_eq!(text_tokens(chars), tokens, "{chars} Zeichen");
    }
    assert_eq!(char_count("Größenänderung"), 14, "Zeichen, nicht Bytes");
    assert_eq!(char_count(""), 0);
}

// spec: AI-012
// acceptance: independent
// Eingabe-Tokens = ⌈Zeichen / 3,5⌉ + Bild-Schätzwert × Bilder + fester Anteil für Anweisungen.
#[test]
fn input_estimate_adds_images_and_instructions() {
    assert_eq!(estimate_input_tokens(3_500, 2, 85, 50), 1_000 + 170 + 50);
    assert_eq!(estimate_input_tokens(0, 0, 85, 0), 0);
    assert_eq!(estimate_input_tokens(8, 1, 1_000, 7), 3 + 1_000 + 7);
}

fn two_claude_items(cal: Calibration) -> TaskEstimate {
    let it = items(2, 3_500, 0);
    let r = claude();
    estimate_task(&input(&it, Some(&r), cal), &NoCache)
}

// spec: AI-012
// acceptance: independent
// Unkalibriert: niedrig/hoch = erwartet × 0,75 / × 1,25. 2 Einträge à 3.500 Zeichen auf mock-claude
// (1/5 USD je 1M), 200 Ausgabe-Tokens → je 1.000 + 1.000 = 2.000 µUSD, erwartet 4.000, niedrig 3.000, hoch 5.000.
#[test]
fn uncalibrated_spread_is_075_125() {
    let e = two_claude_items(Calibration::UNCALIBRATED);
    assert_eq!(e.items, 2);
    assert_eq!(e.cached, 0);
    assert_eq!(e.est_input_tokens, 2_000);
    assert_eq!(e.est_output_tokens, 400);
    assert_eq!(e.expected, Some(MicroUsd(4_000)));
    assert_eq!(e.low, Some(MicroUsd(3_000)));
    assert_eq!(e.high, Some(MicroUsd(5_000)));
    assert!(!e.calibrated);
    assert_eq!(e.executable, Executable::Yes);
}

// spec: AI-012
// acceptance: independent
// Kalibriert: niedrig/hoch = erwartet × 0,9 / × 1,1; der Korrekturfaktor multipliziert die Eingabe-Tokens.
#[test]
fn calibrated_spread_is_09_11_and_factor_scales_input() {
    let e = two_claude_items(calibrated(1_000_000));
    assert_eq!(e.expected, Some(MicroUsd(4_000)));
    assert_eq!(e.low, Some(MicroUsd(3_600)));
    assert_eq!(e.high, Some(MicroUsd(4_400)));
    assert!(e.calibrated);

    // Faktor 1,2: Eingabe 1.200 je Eintrag → je 1.200 + 1.000 = 2.200, erwartet 4.400.
    let e = two_claude_items(calibrated(1_200_000));
    assert_eq!(e.est_input_tokens, 2_400);
    assert_eq!(e.expected, Some(MicroUsd(4_400)));
    assert_eq!(e.low, Some(MicroUsd(3_960)));
    assert_eq!(e.high, Some(MicroUsd(4_840)));
}

// spec: AI-012
// acceptance: independent
// Einträge mit Cache-Treffer zählen in `Cached` und tragen 0 zu den Kosten bei.
#[test]
fn cache_hits_counted_and_free() {
    let it = items(3, 3_500, 0);
    let r = claude();
    let cache: BTreeSet<String> = ["key-1".to_string()].into();
    let e = estimate_task(&input(&it, Some(&r), Calibration::UNCALIBRATED), &cache);
    assert_eq!(e.items, 3, "Items = Einträge im Geltungsbereich");
    assert_eq!(e.cached, 1);
    assert_eq!(
        e.expected,
        Some(MicroUsd(4_000)),
        "nur 2 nicht gecachte Einträge"
    );
    let all: BTreeSet<String> = ["key-0", "key-1", "key-2"].map(String::from).into();
    let e = estimate_task(&input(&it, Some(&r), Calibration::UNCALIBRATED), &all);
    assert_eq!(e.cached, 3);
    assert_eq!(e.expected, Some(MicroUsd(0)));
    assert_eq!(e.high, Some(MicroUsd(0)));
}

// spec: AI-012
// acceptance: independent
// Bildaufgabe über das OpenRouter-Mock-Modell (0,001 je Request, 0,002 je Bild) für 3 Bilder (je 1 Request,
// 1 Bild) → 3 × (0,001 + 0,002) USD = 9.000 µUSD zuzüglich Tokenkosten.
#[test]
fn image_task_fees_are_nine_thousand_plus_tokens() {
    let mut fees_only = openrouter_vision();
    fees_only.input_pico = 0;
    fees_only.output_pico = 0;
    let it = items(3, 700, 1);
    let mut inp = input(&it, Some(&fees_only), Calibration::UNCALIBRATED);
    inp.task = "suggest-alt".into();
    inp.model = "mock/vision".into();
    let e = estimate_task(&inp, &NoCache);
    assert_eq!(e.expected, Some(MicroUsd(9_000)), "nur Gebühren");

    // Mit Tokenpreisen (1 / 5 µUSD je Token): je Eintrag ⌈700 / 3,5⌉ = 200 + 85 Bild-Schätzwert + 30 Anweisungen
    // = 315 Eingabe-Tokens × 1 + 200 Ausgabe × 5 = 1.315 µUSD; 3 Einträge = 3.945 + 9.000 Gebühren.
    let v = openrouter_vision();
    let mut inp = input(&it, Some(&v), Calibration::UNCALIBRATED);
    inp.instruction_tokens = 30;
    let e = estimate_task(&inp, &NoCache);
    assert_eq!(e.est_input_tokens, 3 * 315);
    assert_eq!(e.expected, Some(MicroUsd(9_000 + 3 * 1_315)));
}

// spec: AI-012
// acceptance: independent
// Gebühren gehen nicht in den Korrekturfaktor ein: ein Korrekturfaktor ändert nur die Tokenkosten.
#[test]
fn calibration_factor_does_not_scale_fees() {
    let mut fees_only = openrouter_vision();
    fees_only.input_pico = 0;
    fees_only.output_pico = 0;
    let it = items(3, 700, 1);
    let e = estimate_task(
        &input(&it, Some(&fees_only), calibrated(2_000_000)),
        &NoCache,
    );
    assert_eq!(e.expected, Some(MicroUsd(9_000)));
}

// spec: AI-012
// acceptance: independent
// Batch-Ersparnis: mock-claude mit 50 % Batch-Rabatt → Ersparnis = halbe Tokenkosten (2 × 1.000 µUSD).
#[test]
fn batch_savings_column() {
    let it = items(2, 3_500, 0);
    let r = claude();
    let mut inp = input(&it, Some(&r), Calibration::UNCALIBRATED);
    inp.batch_capable = true;
    let e = estimate_task(&inp, &NoCache);
    assert_eq!(e.batch_savings, Some(MicroUsd(2_000)));
}

// spec: AI-012
// acceptance: independent
// Aufgaben ohne Preis bzw. ohne Schlüssel: `Executable = nein` mit Grund `Price unknown` bzw. `Missing API key`;
// ohne Preis zeigt die Schätzung „Preis unbekannt“ (keine Beträge).
#[test]
fn not_executable_reasons() {
    let it = items(2, 3_500, 0);
    let e = estimate_task(&input(&it, None, Calibration::UNCALIBRATED), &NoCache);
    assert!(!e.executable.is_yes());
    assert_eq!(e.executable.reason(), "Price unknown");
    assert_eq!((e.low, e.expected, e.high), (None, None, None));

    let r = claude();
    let mut inp = input(&it, Some(&r), Calibration::UNCALIBRATED);
    inp.api_key_present = false;
    let e = estimate_task(&inp, &NoCache);
    assert!(!e.executable.is_yes());
    assert_eq!(e.executable.reason(), "Missing API key");
}

// spec: AI-012
// acceptance: independent
// Bestätigung verlangen ab: Standard 1,00 USD erwarteter Kosten.
#[test]
fn default_confirmation_threshold_is_one_usd() {
    assert_eq!(
        AiAppSettings::default().confirmation_threshold,
        MicroUsd(1_000_000)
    );
}

// spec: AI-012
// acceptance: independent
// Bestätigung nötig, wenn die erwarteten Kosten über der Schwelle oder über 50 % des Restbudgets liegen.
#[test]
fn confirmation_rule_threshold_or_half_of_remaining() {
    let t = MicroUsd(1_000_000);
    let big = Some(MicroUsd(5_000_000));
    assert!(!confirmation_required(MicroUsd(999_999), t, big));
    assert!(
        !confirmation_required(MicroUsd(1_000_000), t, big),
        "„über“ der Schwelle"
    );
    assert!(confirmation_required(MicroUsd(1_000_001), t, big));
    // 50 % des Restbudgets (1,00 USD Rest): 0,50 nicht darüber, 0,500001 darüber.
    let rest = Some(MicroUsd(1_000_000));
    assert!(!confirmation_required(MicroUsd(500_000), t, rest));
    assert!(confirmation_required(MicroUsd(500_001), t, rest));
    // Testprofil mit Schwelle 0,0001 USD.
    assert!(confirmation_required(MicroUsd(101), MicroUsd(100), big));
    assert!(!confirmation_required(MicroUsd(0), MicroUsd(100), big));
}

// spec: AI-012
// acceptance: independent
// Kalibrierung erst ab 20 Requests; Median Ist/Schätzung.
#[test]
fn calibration_needs_twenty_samples() {
    let nineteen: Vec<(u64, u64)> = (0..19).map(|_| (1_000, 1_300)).collect();
    let c = Calibration::from_samples(nineteen.clone());
    assert!(!c.calibrated);
    assert_eq!(c.spread_ppm(), (750_000, 1_250_000));
    let mut twenty = nineteen;
    twenty.push((1_000, 1_300));
    let c = Calibration::from_samples(twenty);
    assert!(c.calibrated);
    assert_eq!(c.factor_ppm, 1_300_000);
    assert_eq!(c.spread_ppm(), (900_000, 1_100_000));
}

// spec: AI-012
// acceptance: independent
// Median (nicht Mittelwert) der Verhältnisse: 21 Werte, Ausreißer verschieben ihn nicht.
#[test]
fn calibration_is_median_not_mean() {
    let mut s: Vec<(u64, u64)> = Vec::new();
    for _ in 0..10 {
        s.push((1_000, 1_100));
    }
    s.push((1_000, 1_150)); // Median
    for _ in 0..10 {
        s.push((1_000, 9_000)); // Ausreißer
    }
    let c = Calibration::from_samples(s);
    assert!(c.calibrated);
    assert_eq!(c.factor_ppm, 1_150_000);
}

// spec: AI-012
// acceptance: independent
// Nur die letzten 200 protokollierten Requests zählen.
#[test]
fn calibration_uses_last_two_hundred() {
    let mut s: Vec<(u64, u64)> = (0..150).map(|_| (1_000, 3_000)).collect(); // alt
    s.extend((0..200).map(|_| (1_000, 1_100))); // neu
    let c = Calibration::from_samples(s);
    assert_eq!(c.factor_ppm, 1_100_000);
}

fn log_rows(
    file: &os_store::CrawlFile,
    provider: &str,
    model: &str,
    rates: &os_ai::cost::Rates,
    n: usize,
    actual: u64,
    images: u32,
) {
    for _ in 0..n {
        let outcome = RequestOutcome::success(usage(actual, 50), images, 10).with_estimate(1_000);
        let rec = request_record(
            &ctx("suggest-alt", provider, model, "chat-completions"),
            rates,
            MicroUsd(1_000_000),
            &outcome,
        );
        os_store::ai::requests::insert(file.conn(), &rec).unwrap();
    }
}

// spec: AI-012
// acceptance: independent
// Korrekturfaktor je Modell aus dem Protokoll der Crawl-Datei: Median Ist/Schätzung der Eingabe-Tokens, ab 20
// Requests; Gebühren (Bildrequests über OpenRouter-Vision) gehen nicht ein; andere Modelle zählen nicht.
#[test]
fn calibration_from_log_per_model_ignores_fees() {
    let dir = TempDir::new("cal");
    let f = crawl_file(&dir.0);
    let v = openrouter_vision();
    let c = claude();
    // Anderes Modell mit Faktor 2,0 – darf mock/vision nicht beeinflussen.
    log_rows(&f, "anthropic", "mock-claude", &c, 25, 2_000, 0);
    log_rows(&f, "openrouter", "mock/vision", &v, 19, 1_200, 1);
    let cal = Calibration::from_log(&f, "openrouter", "mock/vision").unwrap();
    assert!(!cal.calibrated, "19 Requests reichen nicht");
    log_rows(&f, "openrouter", "mock/vision", &v, 1, 1_200, 1);
    let cal = Calibration::from_log(&f, "openrouter", "mock/vision").unwrap();
    assert!(cal.calibrated);
    assert_eq!(
        cal.factor_ppm, 1_200_000,
        "Ist/Schätzung der Eingabe-Tokens, ohne Gebühren"
    );
    let cal_c = Calibration::from_log(&f, "anthropic", "mock-claude").unwrap();
    assert_eq!(cal_c.factor_ppm, 2_000_000);

    // Nach Kalibrierung trifft die Schätzung die Ist-Eingabe: 3.500 Zeichen → 1.000 × 1,2 = 1.200.
    let it = items(1, 3_500, 0);
    let e = estimate_task(&input(&it, Some(&c), cal), &NoCache);
    assert_eq!(e.est_input_tokens, 1_200);
    assert!(e.calibrated);
}
