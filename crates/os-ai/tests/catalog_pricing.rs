//! Preistabelle und Preisauflösung (AI-010) gegen `tests/fixtures/ai/pricing-test.json` (T01) und die Testdaten
//! `tests/fixtures/ai/k04/`. Die Kostenformel stammt aus `os_ai::cost` (K05).
use std::path::PathBuf;

use os_ai::catalog::pricing::{validate_pricing, PRICING_FILE_ENV};
use os_ai::catalog::{CivilDate, Endpoint, PriceCatalog, PriceKey, PriceOverride, PriceTable};
use os_ai::cost::{cost_of, FeeCounts, PriceSource, Rates, TokenCounts};
use serde_json::Value;

fn fixture(parts: &[&str]) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in ["..", "..", "tests", "fixtures", "ai"].iter().chain(parts) {
        p.push(part);
    }
    p
}

fn read(parts: &[&str]) -> String {
    std::fs::read_to_string(fixture(parts)).unwrap()
}

fn test_table() -> PriceTable {
    PriceTable::from_json_str(&read(&["pricing-test.json"])).unwrap()
}

fn k04_table() -> PriceTable {
    PriceTable::from_json_str(&read(&["k04", "pricing-catalog.json"])).unwrap()
}

fn luna(endpoint: Endpoint) -> PriceKey {
    PriceKey::new("openai", "gpt-6-luna", endpoint)
}

fn micro(rates: &Rates, input: u64, output: u64) -> u64 {
    cost_of(
        &TokenCounts::plain(input, output),
        FeeCounts::one_request(0),
        rates,
        false,
    )
    .cost
    .0
}

fn today() -> CivilDate {
    CivilDate::parse("2026-11-24").unwrap()
}

// spec: AI-010
// `gpt-6-luna` über `responses` mit 1.000 Eingabe- und 200 Ausgabe-Tokens → 200 µUSD; dieselben 1.000 Eingabe-Tokens
// über `decisions` → 100 µUSD (Preisschlüssel inkl. Endpunkt, `pricing-test.json`).
#[test]
fn price_key_includes_the_endpoint() {
    let catalog = PriceCatalog::new(test_table());
    let responses = catalog
        .resolve(&luna(Endpoint::Responses), None)
        .unwrap()
        .unwrap();
    assert_eq!(responses.source, PriceSource::Table);
    assert_eq!(micro(&responses.rates, 1_000, 200), 200);
    let decisions = catalog
        .resolve(&luna(Endpoint::Decisions), None)
        .unwrap()
        .unwrap();
    assert_eq!(micro(&decisions.rates, 1_000, 0), 100);
}

// spec: AI-010
// Langkontext über `responses`: 300.000 Eingabe- und 1.000 Ausgabe-Tokens → 60.750 µUSD; Regionalzuschlag 10 % auf der
// EU-Basis-URL auf den Gesamtbetrag.
#[test]
fn long_context_and_regional_uplift() {
    let catalog = PriceCatalog::new(test_table());
    let global = catalog
        .resolve(&luna(Endpoint::Responses), None)
        .unwrap()
        .unwrap();
    assert_eq!(micro(&global.rates, 300_000, 1_000), 60_750);
    // Knapp unter der Schwelle: Grundpreise.
    assert_eq!(micro(&global.rates, 271_999, 1_000), 27_200 + 500);
    let eu = catalog
        .resolve(&luna(Endpoint::Responses), Some("eu"))
        .unwrap()
        .unwrap();
    assert_eq!(eu.rates.regional_uplift_ppm, 100_000);
    assert_eq!(micro(&eu.rates, 300_000, 1_000), 66_825);
}

// spec: AI-010
// Kein Eintrag für den angefragten Endpunkt → Preis unbekannt, kein Rückgriff auf einen anderen Endpunkt.
#[test]
fn missing_endpoint_entry_means_price_unknown() {
    let catalog = PriceCatalog::new(test_table());
    assert!(catalog
        .resolve(&luna(Endpoint::Embeddings), None)
        .unwrap()
        .is_none());
    assert!(catalog
        .resolve(
            &PriceKey::new("anthropic", "mock-claude", Endpoint::Responses),
            None
        )
        .unwrap()
        .is_none());
    assert!(catalog
        .resolve(
            &PriceKey::new("anthropic", "mock-claude", Endpoint::Messages),
            None
        )
        .unwrap()
        .is_some());
}

// spec: AI-010
// `verified_at` älter als 120 Tage → „Preis möglicherweise veraltet“; Grenze 120 Tage ist noch aktuell.
#[test]
fn table_prices_older_than_120_days_are_flagged() {
    let catalog = PriceCatalog::new(k04_table());
    let old = catalog
        .resolve(
            &PriceKey::new("anthropic", "mock-claude-old", Endpoint::Messages),
            None,
        )
        .unwrap()
        .unwrap();
    assert_eq!(old.verified_at, CivilDate::parse("2026-05-08"));
    assert_eq!(today().days_since(old.verified_at.unwrap()), 200);
    assert!(old.possibly_stale(today()));
    let fresh = catalog
        .resolve(
            &PriceKey::new("anthropic", "mock-claude", Endpoint::Messages),
            None,
        )
        .unwrap()
        .unwrap();
    assert!(!fresh.possibly_stale(today()));
    let verified = fresh.verified_at.unwrap();
    let day_120 = CivilDate::parse("2027-03-24").unwrap();
    assert_eq!(day_120.days_since(verified), 120);
    assert!(!fresh.possibly_stale(day_120));
    assert!(fresh.possibly_stale(CivilDate::parse("2027-03-25").unwrap()));
}

// spec: AI-010
// Nutzerüberschreibung hat Vorrang vor Tabelle und API; „Preise zurücksetzen“ entfernt sie.
#[test]
fn user_override_wins_and_reset_removes_it() {
    let mut catalog = PriceCatalog::new(test_table());
    let key = luna(Endpoint::Responses);
    catalog
        .set_override(
            key.clone(),
            PriceOverride {
                input_per_million: Some("0.05".into()),
                ..PriceOverride::default()
            },
        )
        .unwrap();
    let user = catalog.resolve(&key, None).unwrap().unwrap();
    assert_eq!(user.source, PriceSource::User);
    assert!(!user.possibly_stale(CivilDate::parse("2030-01-01").unwrap()));
    // Eingabe überschrieben, Ausgabe aus der Tabelle.
    assert_eq!(micro(&user.rates, 1_000, 200), 50 + 100);

    // API-Preis (OpenRouter) vor Tabelle, Nutzer vor API.
    let or_key = PriceKey::new("openrouter", "mock/text", Endpoint::ChatCompletions);
    catalog.set_api_rates(
        or_key.clone(),
        Rates::per_million("1", "5", PriceSource::Table).unwrap(),
    );
    let api = catalog.resolve(&or_key, None).unwrap().unwrap();
    assert_eq!(api.source, PriceSource::Api);
    assert_eq!(api.verified_at, None);
    catalog
        .set_override(
            or_key.clone(),
            PriceOverride {
                output_per_million: Some("1".into()),
                ..PriceOverride::default()
            },
        )
        .unwrap();
    let over = catalog.resolve(&or_key, None).unwrap().unwrap();
    assert_eq!(over.source, PriceSource::User);
    assert_eq!(micro(&over.rates, 1_000, 1_000), 1_000 + 1_000);
    // Überschreibung für ein Modell ohne Tabellen- oder API-Preis macht es bepreist.
    let unknown = PriceKey::new("anthropic", "mock-claude-c", Endpoint::Messages);
    assert!(catalog.resolve(&unknown, None).unwrap().is_none());
    catalog
        .set_override(
            unknown.clone(),
            PriceOverride {
                input_per_million: Some("2".into()),
                output_per_million: Some("4".into()),
                ..PriceOverride::default()
            },
        )
        .unwrap();
    assert_eq!(
        micro(
            &catalog.resolve(&unknown, None).unwrap().unwrap().rates,
            1_000,
            1_000
        ),
        6_000
    );
    assert!(catalog
        .set_override(
            unknown.clone(),
            PriceOverride {
                input_per_million: Some("-1".into()),
                ..PriceOverride::default()
            }
        )
        .is_err());

    catalog.reset_overrides();
    assert!(catalog.overrides().is_empty());
    assert_eq!(
        catalog.resolve(&key, None).unwrap().unwrap().source,
        PriceSource::Table
    );
    assert_eq!(
        catalog.resolve(&or_key, None).unwrap().unwrap().source,
        PriceSource::Api
    );
    assert!(catalog.resolve(&unknown, None).unwrap().is_none());
}

// spec: AI-010
// Testpreisdatei über `OPENSPIDER_AI_PRICING_FILE` (nur Test- und Debug-Builds); ohne Variable die ausgelieferte Datei.
#[test]
fn pricing_file_env_selects_the_test_table() {
    let path = fixture(&["pricing-test.json"])
        .to_string_lossy()
        .to_string();
    let with =
        PriceTable::load(&|name: &str| (name == PRICING_FILE_ENV).then(|| path.clone())).unwrap();
    assert!(with.file().test_only);
    assert!(with
        .get(&PriceKey::new(
            "anthropic",
            "mock-claude",
            Endpoint::Messages
        ))
        .is_some());
    let without = PriceTable::load(&|_: &str| None).unwrap();
    assert_eq!(without, PriceTable::bundled().unwrap());
    assert!(!without.file().test_only);
    let missing = PriceTable::load(&|_: &str| Some("/nicht/vorhanden.json".into()));
    assert!(missing.is_err());
}

// spec: AI-010
// Schema: Pflichtfelder inkl. `endpoint`, nichtnegative Preise, eindeutige Tripel; derselbe Anbieter/Modell mit zwei
// Endpunkten ist gültig; `anthropic` mit Fähigkeitsfeldern (`effort`, `vision`, `capabilities`) ist ungültig.
#[test]
fn pricing_schema_rejects_invalid_files() {
    let valid: Value = serde_json::from_str(&read(&["k04", "pricing-catalog.json"])).unwrap();
    assert_eq!(validate_pricing(&valid), Ok(()));
    // Zwei Endpunkte desselben Modells.
    let table = k04_table();
    assert!(table.get(&luna(Endpoint::Responses)).is_some());
    assert!(table.get(&luna(Endpoint::Decisions)).is_some());
    let mut names: Vec<_> = std::fs::read_dir(fixture(&["k04", "invalid"]))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("pricing-"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 16, "{names:?}");
    for name in names {
        let text = read(&["k04", "invalid", &name]);
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert!(
            validate_pricing(&doc).is_err(),
            "{name} gilt fälschlich als gültig"
        );
        assert!(PriceTable::from_json_str(&text).is_err(), "{name}");
    }
    let dup: Value =
        serde_json::from_str(&read(&["k04", "invalid", "pricing-duplicate-triple.json"])).unwrap();
    let errors = validate_pricing(&dup).unwrap_err();
    assert!(errors.iter().any(|e| e.contains("doppelt")), "{errors:?}");
}

// spec: AI-010
// Fähigkeiten und Reasoning-Stufen der OpenAI-Katalogmodelle für die Adapter (`with_capabilities`, K02/K09).
#[test]
fn catalog_supplies_capabilities_and_reasoning_levels_for_openai() {
    use os_ai::provider::ReasoningLevel::*;
    let table = k04_table();
    let caps = table.capability_lookup("openai", Endpoint::Responses);
    let luna_caps = caps("gpt-6-luna").unwrap();
    assert!(luna_caps.text && luna_caps.structured_output && luna_caps.vision);
    assert!(!luna_caps.decisions);
    assert_eq!(caps("unbekannt"), None);
    let decisions = table.capability_lookup("openai", Endpoint::Decisions);
    assert!(decisions("gpt-6-luna").unwrap().decisions);
    let levels = table.reasoning_lookup("openai", Endpoint::Responses);
    assert_eq!(levels("mock-reasoning"), vec![Off, Low, Medium, High]);
    assert_eq!(levels("mock-plain"), vec![]);
    assert_eq!(levels("unbekannt"), vec![]);
    assert_eq!(
        table.max_output_tokens(&PriceKey::new(
            "openai",
            "mock-reasoning",
            Endpoint::Responses
        )),
        Some(32_000)
    );
}

// spec: AI-010
// Sätze aus der Tabelle: Cache-Lesen, Batch-Rabatt, Langkontext-Faktor für Cache-Lesen (fehlend = Eingabefaktor).
#[test]
fn entry_rates_cover_all_price_positions() {
    let table = test_table();
    let rates = table
        .get(&luna(Endpoint::Responses))
        .unwrap()
        .rates(None)
        .unwrap();
    assert_eq!(rates.input_pico, 100_000);
    assert_eq!(rates.output_pico, 500_000);
    assert_eq!(rates.cache_read_pico, 10_000);
    assert_eq!(rates.batch_discount_ppm, 500_000);
    let lc = rates.long_context.unwrap();
    assert_eq!(lc.threshold_input_tokens, 272_000);
    assert_eq!(
        (
            lc.input_factor_ppm,
            lc.output_factor_ppm,
            lc.cache_read_factor_ppm
        ),
        (2_000_000, 1_500_000, 2_000_000)
    );
    assert_eq!(rates.source, PriceSource::Table);
}
