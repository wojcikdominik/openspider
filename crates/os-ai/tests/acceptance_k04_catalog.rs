//! Unabhängige Abnahmetests M4-K04: Modellkatalog und Preise (AI-010) samt Nachtrag „24.11.2026, Q00“.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` AI-010 (Preisschlüssel Anbieter/Modell/Endpunkt, kein
//! Rückgriff auf einen anderen Endpunkt, Preisquelle `api`|`table`|`user`, Nutzerüberschreibung mit Vorrang und
//! „Preise zurücksetzen“, Hinweis bei `verified_at` älter als 120 Tage, CI-Schematest mit Pflichtfeldern,
//! nichtnegativen Preisen, eindeutigen Tripeln und ohne Fähigkeitsfelder bei `anthropic`; Prüfwerte 200 µUSD /
//! 100 µUSD und 60.750 µUSD; Nachtrag: `reasoning_efforts`, `max_output_tokens`), „Konventionen“
//! (`pricing-test.json`, `OPENSPIDER_AI_PRICING_FILE`) und dem Schemaort `resources/ai/schemas/` (Orchestrator).
// acceptance: independent

mod acceptance_k04_support;

use acceptance_k04_support::*;
use os_ai::catalog::pricing::{validate_pricing, PriceOverride, PRICING_FILE_ENV};
use os_ai::catalog::reasoning::dropdown_levels;
use os_ai::catalog::{CivilDate, Endpoint, PriceCatalog, PriceKey, PriceTable};
use os_ai::cost::{cost_of, FeeCounts, PriceSource, Rates, TokenCounts};
use os_ai::provider::ReasoningLevel;
use serde_json::{json, Value};

fn spec_test_pricing() -> PriceTable {
    let path = repo_root().join("tests/fixtures/ai/pricing-test.json");
    PriceTable::from_json_str(&std::fs::read_to_string(path).unwrap())
        .expect("pricing-test.json ist gültig")
}

fn micro(cost: os_model::budget::MicroUsd) -> u64 {
    cost.0
}

/// Ein minimal gültiges Preisdokument mit einem Eintrag.
fn doc_with(entry: Value) -> Value {
    json!({ "prices": [entry] })
}

fn base_entry() -> Value {
    json!({
        "provider": "openai",
        "model": "acc-model",
        "endpoint": "responses",
        "input_per_million": 0.1,
        "output_per_million": 0.5,
        "source_url": "https://example.test/acc",
        "verified_at": "2026-11-24"
    })
}

fn with(mut entry: Value, field: &str, value: Value) -> Value {
    entry[field] = value;
    entry
}

fn without(mut entry: Value, field: &str) -> Value {
    entry.as_object_mut().unwrap().remove(field);
    entry
}

fn rejected(doc: &Value) -> bool {
    validate_pricing(doc).is_err() && PriceTable::from_json_str(&doc.to_string()).is_err()
}

// ---------------------------------------------------------------------------------------------------------------
// Ausgelieferte Datei

// spec: AI-010
// acceptance: independent
#[test]
fn bundled_pricing_validates_and_every_entry_has_source_and_date() {
    let doc = resource_json("pricing.json");
    validate_pricing(&doc).expect("pricing.json gültig");
    let table = PriceTable::bundled().expect("ausgelieferte Tabelle lädt");
    assert!(!table.entries().is_empty());
    for e in table.entries() {
        assert!(!e.source_url.trim().is_empty(), "{e:?}");
        assert!(
            CivilDate::parse(&e.verified_at).is_some(),
            "verified_at {:?}",
            e.verified_at
        );
    }
}

// spec: AI-010
// acceptance: independent
#[test]
fn bundled_pricing_validates_against_schema_file_in_resources() {
    // CI-Test „pricing.json gegen ein JSON-Schema“; Schemaort laut Orchestrator resources/ai/schemas/.
    let schema = resource_json("schemas/pricing.schema.json");
    let validator = jsonschema::validator_for(&schema).expect("Schema kompiliert");
    let doc = resource_json("pricing.json");
    let errors: Vec<String> = validator.iter_errors(&doc).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    // Anthropic mit Fähigkeitsfeld ist auch gegen die Schemadatei ungültig
    let bad = doc_with(json!({
        "provider": "anthropic", "model": "acc", "endpoint": "messages",
        "input_per_million": 1, "output_per_million": 5,
        "source_url": "https://example.test", "verified_at": "2026-11-24",
        "capabilities": { "vision": true }
    }));
    assert!(!validator.is_valid(&bad));
}

// spec: AI-010
// acceptance: independent
#[test]
fn bundled_gpt_6_luna_prices_match_spec() {
    let table = PriceTable::bundled().unwrap();
    let responses = table
        .get(&PriceKey::new("openai", "gpt-6-luna", Endpoint::Responses))
        .expect("gpt-6-luna über responses");
    let r = responses.rates(None).unwrap();
    // 0,10 / 0,50 USD je 1 Mio., Cache-Lesen 0,01, Batch 50 % (0,05/0,25)
    assert_eq!(
        r.input_pico,
        Rates::per_million("0.1", "0.5", PriceSource::Table)
            .unwrap()
            .input_pico
    );
    assert_eq!(
        r.output_pico,
        Rates::per_million("0.1", "0.5", PriceSource::Table)
            .unwrap()
            .output_pico
    );
    assert_eq!(
        r.cache_read_pico,
        Rates::per_million("0.01", "0", PriceSource::Table)
            .unwrap()
            .input_pico
    );
    assert_eq!(r.batch_discount_ppm, 500_000);
    let lc = r.long_context.expect("Langkontext");
    assert_eq!(lc.threshold_input_tokens, 272_000);
    assert_eq!(lc.input_factor_ppm, 2_000_000);
    assert_eq!(lc.output_factor_ppm, 1_500_000);
    // Cache-Lesen im Langkontext 0,02 (Nachtrag Q00) = Faktor 2
    assert_eq!(lc.cache_read_factor_ppm, 2_000_000);
    assert_eq!(
        responses.rates(Some("eu")).unwrap().regional_uplift_ppm,
        100_000
    );
    // Nachtrag Q00: Reasoning-Stufen aus der Menge, ohne Dubletten; Ausgabegrenze positiv
    let efforts = responses
        .reasoning_efforts
        .clone()
        .expect("reasoning_efforts");
    let allowed = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];
    assert!(efforts.iter().all(|e| allowed.contains(&e.as_str())));
    let mut dedup = efforts.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(dedup.len(), efforts.len());
    assert!(responses.max_output_tokens.unwrap_or(1) > 0);

    // decisions: nur Eingabe 0,10, alle anderen Positionen 0
    let decisions = table
        .get(&PriceKey::new("openai", "gpt-6-luna", Endpoint::Decisions))
        .expect("gpt-6-luna über decisions")
        .rates(None)
        .unwrap();
    assert_eq!(decisions.input_pico, r.input_pico);
    assert_eq!(decisions.output_pico, 0);
    assert_eq!(decisions.cache_read_pico, 0);
    assert_eq!(decisions.cache_write_pico, 0);
}

// spec: AI-010
// acceptance: independent
#[test]
fn bundled_anthropic_entries_carry_no_capability_fields() {
    let doc = resource_json("pricing.json");
    for e in doc["prices"].as_array().unwrap() {
        if e["provider"] == "anthropic" {
            for field in [
                "capabilities",
                "effort",
                "vision",
                "reasoning_efforts",
                "max_output_tokens",
            ] {
                assert!(e.get(field).is_none(), "{field} bei {e}");
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Preisschlüssel und Prüfwerte

// spec: AI-010
// acceptance: independent
#[test]
fn same_model_is_priced_per_endpoint() {
    // suggest-title über responses: 1.000 Eingabe + 200 Ausgabe → 200 µUSD; dieselben 1.000 über decisions → 100 µUSD
    let table = spec_test_pricing();
    let responses = table
        .get(&PriceKey::new("openai", "gpt-6-luna", Endpoint::Responses))
        .unwrap()
        .rates(None)
        .unwrap();
    let decisions = table
        .get(&PriceKey::new("openai", "gpt-6-luna", Endpoint::Decisions))
        .unwrap()
        .rates(None)
        .unwrap();
    let c = cost_of(
        &TokenCounts::plain(1_000, 200),
        FeeCounts::one_request(0),
        &responses,
        false,
    );
    assert_eq!(micro(c.cost), 200);
    let d = cost_of(
        &TokenCounts::plain(1_000, 0),
        FeeCounts::one_request(0),
        &decisions,
        false,
    );
    assert_eq!(micro(d.cost), 100);
}

// spec: AI-010
// acceptance: independent
#[test]
fn long_context_and_regional_uplift() {
    let table = spec_test_pricing();
    let key = PriceKey::new("openai", "gpt-6-luna", Endpoint::Responses);
    let entry = table.get(&key).unwrap();
    let tokens = TokenCounts::plain(300_000, 1_000);
    let base = cost_of(
        &tokens,
        FeeCounts::one_request(0),
        &entry.rates(None).unwrap(),
        false,
    );
    // 300.000 × 0,20 + 1.000 × 0,75 je 1 Mio. = 60.750 µUSD
    assert_eq!(micro(base.cost), 60_750);
    // +10 % auf den Gesamtbetrag auf der EU-Basis-URL
    let eu = cost_of(
        &tokens,
        FeeCounts::one_request(0),
        &entry.rates(Some("eu")).unwrap(),
        false,
    );
    assert_eq!(micro(eu.cost), 66_825);
}

// spec: AI-010
// acceptance: independent
#[test]
fn missing_endpoint_entry_means_price_unknown_without_fallback() {
    let catalog = PriceCatalog::new(spec_test_pricing());
    // gpt-6-luna hat Einträge für responses und decisions, aber nicht für chat-completions/embeddings
    for endpoint in [
        Endpoint::ChatCompletions,
        Endpoint::Embeddings,
        Endpoint::Messages,
    ] {
        let key = PriceKey::new("openai", "gpt-6-luna", endpoint);
        assert_eq!(catalog.resolve(&key, None).unwrap(), None, "{endpoint:?}");
    }
    // unbekanntes Modell
    let key = PriceKey::new("openai", "acc-unknown-model", Endpoint::Responses);
    assert_eq!(catalog.resolve(&key, None).unwrap(), None);
    // falscher Anbieter mit bekanntem Modellnamen
    let key = PriceKey::new("openrouter", "gpt-6-luna", Endpoint::Responses);
    assert_eq!(catalog.resolve(&key, None).unwrap(), None);
}

// spec: AI-010
// acceptance: independent
#[test]
fn price_source_table_api_user_and_reset() {
    let mut catalog = PriceCatalog::new(acceptance_pricing());
    let table_key = PriceKey::new("anthropic", "mock-claude", Endpoint::Messages);
    let api_key = PriceKey::new("openrouter", "acc/m-r1", Endpoint::ChatCompletions);

    let t = catalog.resolve(&table_key, None).unwrap().unwrap();
    assert_eq!(t.source, PriceSource::Table);
    assert_eq!(t.verified_at, CivilDate::parse("2026-11-24"));

    catalog.set_api_rates(
        api_key.clone(),
        Rates::per_million("1", "2", PriceSource::Api).unwrap(),
    );
    let a = catalog.resolve(&api_key, None).unwrap().unwrap();
    assert_eq!(a.source, PriceSource::Api);
    let api_input = a.rates.input_pico;

    // Nutzerüberschreibung hat Vorrang vor Tabelle und API
    let o = PriceOverride {
        input_per_million: Some("7".into()),
        ..PriceOverride::default()
    };
    catalog.set_override(table_key.clone(), o.clone()).unwrap();
    catalog.set_override(api_key.clone(), o).unwrap();
    let seven = Rates::per_million("7", "0", PriceSource::User)
        .unwrap()
        .input_pico;
    for key in [&table_key, &api_key] {
        let u = catalog.resolve(key, None).unwrap().unwrap();
        assert_eq!(u.source, PriceSource::User);
        assert_eq!(u.rates.input_pico, seven);
    }
    // nicht überschriebene Felder behalten den Basispreis
    let u = catalog.resolve(&table_key, None).unwrap().unwrap();
    assert_eq!(
        u.rates.output_pico,
        Rates::per_million("0", "5", PriceSource::Table)
            .unwrap()
            .output_pico
    );

    // Nutzerpreis auch für ein Modell ohne Tabellen-/API-Preis (z. B. nach „Preis unbekannt“)
    let unknown = PriceKey::new("openai", "acc-unknown-model", Endpoint::Responses);
    catalog
        .set_override(
            unknown.clone(),
            PriceOverride {
                input_per_million: Some("3".into()),
                output_per_million: Some("4".into()),
                ..PriceOverride::default()
            },
        )
        .unwrap();
    assert_eq!(
        catalog.resolve(&unknown, None).unwrap().unwrap().source,
        PriceSource::User
    );

    // „Preise zurücksetzen“ entfernt die Überschreibungen
    catalog.reset_overrides();
    assert_eq!(
        catalog.resolve(&table_key, None).unwrap().unwrap().source,
        PriceSource::Table
    );
    let back = catalog.resolve(&api_key, None).unwrap().unwrap();
    assert_eq!(back.source, PriceSource::Api);
    assert_eq!(back.rates.input_pico, api_input);
    assert_eq!(catalog.resolve(&unknown, None).unwrap(), None);
}

// spec: AI-010
// acceptance: independent
#[test]
fn invalid_user_override_is_rejected() {
    let mut catalog = PriceCatalog::new(acceptance_pricing());
    let key = PriceKey::new("anthropic", "mock-claude", Endpoint::Messages);
    for bad in ["-1", "abc", ""] {
        let o = PriceOverride {
            input_per_million: Some(bad.into()),
            ..PriceOverride::default()
        };
        assert!(catalog.set_override(key.clone(), o).is_err(), "{bad:?}");
    }
    assert!(catalog.overrides().is_empty());
}

// spec: AI-010
// acceptance: independent
#[test]
fn stale_price_hint_after_120_days() {
    let catalog = PriceCatalog::new(acceptance_pricing());
    let today = CivilDate::parse("2026-11-24").unwrap();
    // Eintrag mit verified_at vor 200 Tagen → Hinweis
    let old = catalog
        .resolve(
            &PriceKey::new("anthropic", "mock-claude-b", Endpoint::Messages),
            None,
        )
        .unwrap()
        .unwrap();
    assert!(old.possibly_stale(today));
    let fresh = catalog
        .resolve(
            &PriceKey::new("anthropic", "mock-claude", Endpoint::Messages),
            None,
        )
        .unwrap()
        .unwrap();
    assert!(!fresh.possibly_stale(today));
    // Grenze: genau 120 Tage nicht älter als 120 Tage, 121 Tage schon
    for (date, stale) in [("2026-07-27", false), ("2026-07-26", true)] {
        let doc = doc_with(with(base_entry(), "verified_at", json!(date)));
        let c = PriceCatalog::new(PriceTable::from_json_str(&doc.to_string()).unwrap());
        let p = c
            .resolve(
                &PriceKey::new("openai", "acc-model", Endpoint::Responses),
                None,
            )
            .unwrap()
            .unwrap();
        assert_eq!(p.possibly_stale(today), stale, "{date}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Schema-Negativfälle (CI-Test)

// spec: AI-010
// acceptance: independent
#[test]
fn schema_accepts_minimal_entry_and_two_endpoints_of_one_model() {
    assert!(validate_pricing(&doc_with(base_entry())).is_ok());
    let two = json!({ "prices": [
        base_entry(),
        with(with(base_entry(), "endpoint", json!("decisions")), "output_per_million", json!(0))
    ]});
    assert!(validate_pricing(&two).is_ok());
    assert!(PriceTable::from_json_str(&two.to_string()).is_ok());
}

// spec: AI-010
// acceptance: independent
#[test]
fn schema_rejects_missing_required_fields() {
    for field in [
        "provider",
        "model",
        "endpoint",
        "input_per_million",
        "output_per_million",
        "source_url",
        "verified_at",
    ] {
        assert!(
            rejected(&doc_with(without(base_entry(), field))),
            "ohne {field}"
        );
    }
    assert!(rejected(&json!({})), "ohne prices");
}

// spec: AI-010
// acceptance: independent
#[test]
fn schema_rejects_negative_prices_and_bad_values() {
    for field in [
        "input_per_million",
        "output_per_million",
        "cache_read_per_million",
        "cache_write_per_million",
        "per_request",
        "per_image",
    ] {
        assert!(
            rejected(&doc_with(with(base_entry(), field, json!(-0.01)))),
            "{field} negativ"
        );
    }
    assert!(rejected(&doc_with(with(
        base_entry(),
        "endpoint",
        json!("chat")
    ))));
    assert!(rejected(&doc_with(with(
        base_entry(),
        "input_per_million",
        json!("0.1")
    ))));
    assert!(rejected(&doc_with(with(
        base_entry(),
        "verified_at",
        json!("24.11.2026")
    ))));
    assert!(rejected(&doc_with(with(
        base_entry(),
        "verified_at",
        json!("2026-02-30")
    ))));
    assert!(rejected(&doc_with(with(
        base_entry(),
        "source_url",
        json!("")
    ))));
    assert!(PriceTable::from_json_str("{ kein json").is_err());
}

// spec: AI-010
// acceptance: independent
#[test]
fn schema_rejects_duplicate_triples() {
    let dup =
        json!({ "prices": [base_entry(), with(base_entry(), "input_per_million", json!(0.3))] });
    let errors = validate_pricing(&dup).unwrap_err();
    assert!(!errors.is_empty());
    assert!(PriceTable::from_json_str(&dup.to_string()).is_err());
}

// spec: AI-010
// acceptance: independent
#[test]
fn schema_rejects_capability_fields_for_anthropic() {
    let anthropic = json!({
        "provider": "anthropic", "model": "acc-claude", "endpoint": "messages",
        "input_per_million": 1, "output_per_million": 5,
        "source_url": "https://example.test/acc", "verified_at": "2026-11-24"
    });
    assert!(validate_pricing(&doc_with(anthropic.clone())).is_ok());
    for (field, value) in [
        ("effort", json!(true)),
        ("vision", json!(true)),
        ("capabilities", json!({ "vision": true })),
        ("reasoning_efforts", json!(["low"])),
        ("max_output_tokens", json!(4000)),
    ] {
        assert!(
            rejected(&doc_with(with(anthropic.clone(), field, value))),
            "anthropic mit {field}"
        );
    }
}

// spec: AI-010
// acceptance: independent
#[test]
fn schema_rules_for_reasoning_efforts_and_max_output_tokens() {
    // Nachtrag Q00: bei openai akzeptiert (nur Werte der Menge, ohne Dubletten; positive Ganzzahl)
    let all = json!(["none", "minimal", "low", "medium", "high", "xhigh", "max"]);
    assert!(validate_pricing(&doc_with(with(base_entry(), "reasoning_efforts", all))).is_ok());
    assert!(validate_pricing(&doc_with(with(base_entry(), "max_output_tokens", json!(1)))).is_ok());
    for bad in [
        json!(["low", "turbo"]),
        json!(["low", "low"]),
        json!(["off"]),
        json!(["model-default"]),
        json!("low"),
    ] {
        assert!(
            rejected(&doc_with(with(
                base_entry(),
                "reasoning_efforts",
                bad.clone()
            ))),
            "{bad}"
        );
    }
    for bad in [json!(0), json!(-5), json!(1.5), json!("4000")] {
        assert!(
            rejected(&doc_with(with(
                base_entry(),
                "max_output_tokens",
                bad.clone()
            ))),
            "{bad}"
        );
    }
}

// spec: AI-010
// acceptance: independent
#[test]
fn test_pricing_file_is_loaded_via_environment_in_test_builds() {
    let path = fixture_dir().join("pricing-acceptance.json");
    let path_text = path.to_string_lossy().to_string();
    let env = move |name: &str| (name == PRICING_FILE_ENV).then(|| path_text.clone());
    let table = PriceTable::load(&env).expect("Testpreisdatei lädt");
    assert!(table
        .get(&PriceKey::new(
            "openai",
            "acc-no-effort",
            Endpoint::Responses
        ))
        .is_some());
    // ohne Variable die ausgelieferte Tabelle
    let bundled = PriceTable::load(&|_: &str| None).unwrap();
    assert_eq!(bundled.entries(), PriceTable::bundled().unwrap().entries());
    // ungültige bzw. fehlende Datei → Fehler, kein stiller Rückfall
    let bad_path = fixture_dir().join("invalid/pricing-duplicate-triple.json");
    let bad_text = bad_path.to_string_lossy().to_string();
    let bad_env = move |name: &str| (name == PRICING_FILE_ENV).then(|| bad_text.clone());
    assert!(PriceTable::load(&bad_env).is_err());
    let missing = fixture_dir().join("invalid/does-not-exist.json");
    let missing_text = missing.to_string_lossy().to_string();
    let missing_env = move |name: &str| (name == PRICING_FILE_ENV).then(|| missing_text.clone());
    assert!(PriceTable::load(&missing_env).is_err());
}

// ---------------------------------------------------------------------------------------------------------------
// Nachtrag Q00: Reasoning-Stufen im Katalog

// spec: AI-010
// acceptance: independent
#[test]
fn catalog_reasoning_efforts_drive_the_dropdown() {
    let table = acceptance_pricing();
    // reasoning_efforts = ["none", "low", "medium", "high"] → off, low, medium, high und model-default
    let luna = table.reasoning_lookup("openai", Endpoint::Responses)("gpt-6-luna");
    let mut offered = dropdown_levels(&luna);
    offered.sort_by_key(|l| l.rank());
    let mut expected = vec![
        ReasoningLevel::ModelDefault,
        ReasoningLevel::Off,
        ReasoningLevel::Low,
        ReasoningLevel::Medium,
        ReasoningLevel::High,
    ];
    expected.sort_by_key(|l| l.rank());
    assert_eq!(offered, expected);
    // ohne Feld nur model-default
    let none = table.reasoning_lookup("openai", Endpoint::Responses)("acc-no-effort");
    assert_eq!(dropdown_levels(&none), vec![ReasoningLevel::ModelDefault]);
    // unbekanntes Modell: ebenfalls nur model-default
    let unknown = table.reasoning_lookup("openai", Endpoint::Responses)("acc-unknown");
    assert_eq!(
        dropdown_levels(&unknown),
        vec![ReasoningLevel::ModelDefault]
    );
    // Ausgabegrenze aus dem Katalog
    assert_eq!(
        table.max_output_tokens(&PriceKey::new("openai", "gpt-6-luna", Endpoint::Responses)),
        Some(128_000)
    );
    assert_eq!(
        table.max_output_tokens(&PriceKey::new(
            "openai",
            "acc-no-effort",
            Endpoint::Responses
        )),
        None
    );
}

// spec: AI-010
// acceptance: independent
#[test]
fn catalog_capabilities_only_for_the_requested_endpoint() {
    let table = acceptance_pricing();
    let responses = table.capability_lookup("openai", Endpoint::Responses);
    let caps = responses("gpt-6-luna").expect("Fähigkeiten aus dem Katalog");
    assert!(caps.vision && caps.structured_output && caps.text);
    assert!(!caps.decisions);
    let decisions = table.capability_lookup("openai", Endpoint::Decisions);
    assert!(decisions("gpt-6-luna").unwrap().decisions);
    // Anthropic: keine Fähigkeiten aus dem Katalog
    let anthropic = table.capability_lookup("anthropic", Endpoint::Messages);
    assert_eq!(anthropic("mock-claude"), None);
}
