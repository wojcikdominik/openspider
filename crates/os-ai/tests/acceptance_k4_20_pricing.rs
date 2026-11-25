//! Unabhängige Abnahmetests K4-20: Preise in `resources/ai/pricing.json`.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` AI-010 (Nachtrag 25.11.2026 „Preise der Decisions API“:
//! Eingabe 0,10, Ausgabe 0, Cache-Lesen/-Schreiben und Langkontext wie der `responses`-Eintrag, kein Batch, mit den
//! Rechenbeispielen der zwei Kriterien; Nachtrag 25.11.2026 nach K04: `valid_from` optional, `source_url` und
//! `verified_at` Pflicht, Schemas genau einmal unter `resources/ai/schemas/`), AI-011 (Preis je genanntem Modell und
//! Endpunkt aus der Preisdokumentation des Anbieters) und AI-025 (Prüfdatum höchstens 120 Tage alt).
// acceptance: independent

mod acceptance_k4_20_support;

use acceptance_k4_20_support::*;
use os_ai::catalog::pricing::validate_pricing;
use os_ai::catalog::{CivilDate, Endpoint, PriceKey, PriceTable};
use os_ai::cost::{cost_of, FeeCounts, Rates, TokenCounts};
use serde_json::json;

fn table() -> PriceTable {
    PriceTable::bundled().expect("ausgelieferte Preistabelle")
}

fn luna(endpoint: Endpoint) -> os_ai::catalog::PriceEntry {
    table()
        .get(&PriceKey::new("openai", "gpt-6-luna", endpoint))
        .cloned()
        .unwrap_or_else(|| panic!("gpt-6-luna über {endpoint:?} fehlt"))
}

fn micro(tokens: &TokenCounts, rates: &Rates) -> u64 {
    cost_of(tokens, FeeCounts::one_request(0), rates, false)
        .cost
        .0
}

// ---------------------------------------------------------------------------------------------------------------
// Decisions-Preise

// spec: AI-010
// acceptance: independent
#[test]
fn decisions_entry_mirrors_responses_except_output_and_batch() {
    let d = luna(Endpoint::Decisions);
    let r = luna(Endpoint::Responses);
    let dr = d.rates(None).unwrap();
    let rr = r.rates(None).unwrap();
    assert_eq!(dr.input_pico, rr.input_pico, "Eingabe 0,10 wie responses");
    assert_eq!(
        dr.input_pico,
        Rates::per_million("0.1", "0", os_ai::cost::PriceSource::Table)
            .unwrap()
            .input_pico
    );
    assert_eq!(dr.output_pico, 0, "Ausgabe 0");
    assert_eq!(
        dr.cache_read_pico, rr.cache_read_pico,
        "Cache-Lesen wie responses"
    );
    assert!(dr.cache_read_pico > 0);
    assert_eq!(
        dr.cache_write_pico, rr.cache_write_pico,
        "Cache-Schreiben wie responses"
    );
    assert!(dr.cache_write_pico > 0);
    assert_eq!(
        dr.long_context, rr.long_context,
        "Langkontext wie responses"
    );
    assert_eq!(
        dr.long_context.as_ref().map(|l| l.threshold_input_tokens),
        Some(272_000)
    );
    assert_eq!(d.batch_discount, None, "Batch entfällt");
    assert_eq!(dr.batch_discount_ppm, 0);
    assert!(d.source_url.starts_with("https://"), "{}", d.source_url);
    assert!(CivilDate::parse(&d.verified_at).is_some());
}

// spec: AI-010
// acceptance: independent
#[test]
fn decisions_cache_read_example() {
    // 1.000.000 Eingabe-Tokens, davon 400.000 aus dem Cache: 600.000 × 0,10 + 400.000 × 0,01 je 1 Mio.
    // (vor Langkontext-Faktor) = 64.000 µUSD, nicht 0,10 USD pauschal.
    let rates = luna(Endpoint::Decisions).rates(None).unwrap();
    let tokens = TokenCounts {
        input_tokens: 1_000_000,
        cached_input_tokens: 400_000,
        cache_write_tokens: 0,
        output_tokens: 0,
    };
    let without_lc = Rates {
        long_context: None,
        ..rates.clone()
    };
    assert_eq!(micro(&tokens, &without_lc), 64_000);
    let with_lc = micro(&tokens, &rates);
    assert_ne!(with_lc, 100_000, "nicht pauschal 0,10 USD");
    // 1.000.000 liegt über der Schwelle 272.000: Faktoren 2 auf Eingabe und Cache-Lesen
    assert_eq!(with_lc, 128_000);
}

// spec: AI-010
// acceptance: independent
#[test]
fn decisions_long_context_uses_input_factor_output_stays_zero() {
    let rates = luna(Endpoint::Decisions).rates(None).unwrap();
    // unter der Schwelle: 200.000 × 0,10 = 20.000 µUSD; Ausgabe kostet nichts
    assert_eq!(micro(&TokenCounts::plain(200_000, 5_000), &rates), 20_000);
    // oberhalb: 300.000 × 0,20 = 60.000 µUSD; 1.000 Ausgabe-Tokens bleiben 0
    assert_eq!(micro(&TokenCounts::plain(300_000, 1_000), &rates), 60_000);
    assert_eq!(micro(&TokenCounts::plain(300_000, 0), &rates), 60_000);
    // gleicher Eingabeanteil wie responses
    let resp = luna(Endpoint::Responses).rates(None).unwrap();
    assert_eq!(micro(&TokenCounts::plain(300_000, 0), &resp), 60_000);
}

// spec: AI-010
// acceptance: independent
#[test]
fn decisions_cache_write_is_charged_like_responses() {
    let d = luna(Endpoint::Decisions).rates(None).unwrap();
    let r = luna(Endpoint::Responses).rates(None).unwrap();
    let tokens = TokenCounts {
        input_tokens: 100_000,
        cached_input_tokens: 20_000,
        cache_write_tokens: 30_000,
        output_tokens: 0,
    };
    assert_eq!(micro(&tokens, &d), micro(&tokens, &r));
    // 50.000 × 0,10 + 20.000 × 0,01 + 30.000 × 0,125 je 1 Mio. (Cache-Schreibpreis des responses-Eintrags)
    let write = r.cache_write_pico as u128 * 30_000 / 1_000_000;
    assert_eq!(micro(&tokens, &d) as u128, 5_000 + 200 + write);
}

// spec: AI-010
// acceptance: independent
#[test]
fn spec_test_pricing_example_still_holds() {
    // Grundkriterium mit pricing-test.json: 1.000 Eingabe-Tokens über decisions → 100 µUSD
    let path = repo_root().join("tests/fixtures/ai/pricing-test.json");
    let t = PriceTable::from_json_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let d = t
        .get(&PriceKey::new("openai", "gpt-6-luna", Endpoint::Decisions))
        .unwrap()
        .rates(None)
        .unwrap();
    assert_eq!(micro(&TokenCounts::plain(1_000, 0), &d), 100);
}

// ---------------------------------------------------------------------------------------------------------------
// Datei und Schema

// spec: AI-010
// acceptance: independent
#[test]
fn shipped_pricing_validates_and_each_entry_has_source_and_date() {
    let doc = resource_json("pricing.json");
    validate_pricing(&doc).expect("pricing.json gültig");
    let schema = resource_json("schemas/pricing.schema.json");
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&doc));
    for e in table().entries() {
        assert!(
            !e.source_url.trim().is_empty(),
            "{}/{}",
            e.provider,
            e.model
        );
        assert!(
            CivilDate::parse(&e.verified_at).is_some(),
            "{}/{}",
            e.provider,
            e.model
        );
    }
}

// spec: AI-010
// acceptance: independent
#[test]
fn valid_from_is_optional_source_and_date_are_required() {
    let base = json!({
        "provider": "openai", "model": "acc", "endpoint": "responses",
        "input_per_million": 1, "output_per_million": 2,
        "source_url": "https://example.test/acc", "verified_at": "2026-11-25"
    });
    assert!(validate_pricing(&json!({ "prices": [base.clone()] })).is_ok());
    let mut with_from = base.clone();
    with_from["valid_from"] = json!("2026-11-01");
    assert!(validate_pricing(&json!({ "prices": [with_from] })).is_ok());
    for field in ["source_url", "verified_at"] {
        let mut bad = base.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(
            validate_pricing(&json!({ "prices": [bad.clone()] })).is_err(),
            "ohne {field}"
        );
        assert!(PriceTable::from_json_str(&json!({ "prices": [bad] }).to_string()).is_err());
    }
}

fn collect_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if matches!(
                name.as_str(),
                "target" | "node_modules" | ".git" | "acc-log"
            ) {
                continue;
            }
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

// spec: AI-010
// acceptance: independent
#[test]
fn data_schemas_exist_exactly_once_under_resources() {
    let root = repo_root();
    let mut files = Vec::new();
    collect_files(&root, &mut files);
    for name in [
        "pricing.schema.json",
        "task-defaults.schema.json",
        "provider-limits.schema.json",
    ] {
        let hits: Vec<_> = files
            .iter()
            .filter(|p| p.file_name().is_some_and(|f| f == name))
            .collect();
        assert_eq!(hits.len(), 1, "{name}: {hits:?}");
        let expected = root.join("resources/ai/schemas").join(name);
        assert_eq!(
            hits[0].canonicalize().unwrap(),
            expected.canonicalize().unwrap(),
            "{name}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Prüfdatum (AI-025)

// spec: AI-025
// acceptance: independent
#[test]
fn verification_dates_of_defaults_and_prices_are_recent() {
    // Release-Check: kein verwendeter Preis und kein Standardmodell älter als 120 Tage. Bezugstag ist der
    // Bestätigungstag der Liste (25.11.2026); dazu: kein Prüfdatum in der Zukunft.
    let day = CivilDate::parse("2026-11-25").unwrap();
    for e in table().entries() {
        let v = CivilDate::parse(&e.verified_at).unwrap();
        let age = day.days_since(v);
        assert!(
            (0..=120).contains(&age),
            "{}/{}: {}",
            e.provider,
            e.model,
            e.verified_at
        );
    }
    let doc = resource_json("task-defaults.json");
    let v = CivilDate::parse(doc["verified_at"].as_str().unwrap()).unwrap();
    assert!((0..=120).contains(&day.days_since(v)));
}
