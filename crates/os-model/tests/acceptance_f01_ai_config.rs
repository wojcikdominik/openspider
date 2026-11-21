//! Unabhängige Abnahmetests M4-F01: KI-Konfiguration im Profil, App-Einstellung „KI (Vorschau)“, Budget-Typen.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-006, AI-012, AI-013, AI-017, AI-019, AI-024),
//! `docs/spec/04-custom-integrations.md` (INTEG-179, INTEG-180 … INTEG-184) und `docs/plan/M4.md` (E1, E-2 b).
// acceptance: independent

use std::path::{Path, PathBuf};

use os_model::budget::{BudgetLimits, MicroUsd};
use os_model::config::{AiAppSettings, AiPolicy, ProviderEndpoint};
use os_model::settings::AppSettings;
use os_model::CrawlConfig;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(rel: &str) -> PathBuf {
    repo().join("tests/fixtures").join(rel)
}

/// Eigenes Temp-Verzeichnis je Test (os-model hat kein `tempfile`).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "os-acceptance-f01-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const USD: u64 = 1_000_000;

// spec: AI-013
// acceptance: independent
// Geldbeträge intern als Ganzzahl in Mikro-USD (1 USD = 1.000.000 µUSD), ohne Gleitkomma.
#[test]
fn micro_usd_is_an_integer_with_one_million_per_dollar() {
    assert_eq!(MicroUsd::default(), MicroUsd(0));
    let five = MicroUsd(5 * USD);
    assert_eq!(
        serde_json::to_value(five).unwrap(),
        serde_json::json!(5_000_000u64)
    );
    let back: MicroUsd = serde_json::from_str("4000").unwrap();
    assert_eq!(back, MicroUsd(4_000)); // 0,004 USD aus AI-013
                                       // Keine Brüche: 0,0040 USD als Gleitkommazahl ist kein gültiger Betrag.
    assert!(serde_json::from_str::<MicroUsd>("0.004").is_err());
    assert!(serde_json::from_str::<MicroUsd>("-1").is_err());
    // Beispiel AI-019: 1.000 µUSD + 100 µUSD Zuschlag = 1.100 µUSD – exakte Ganzzahlarithmetik.
    assert_eq!(MicroUsd(1_000).0 + MicroUsd(100).0, 1_100);
    assert!(MicroUsd(10_000) > MicroUsd(8_000));
}

// spec: AI-013
// acceptance: independent
// Crawl-Budget Standard 5,00 USD; „Unbegrenzt“ nur über ausdrückliche Checkbox (Standard aus); Teilbudget
// „KI gesamt“ leer = kein Teilbudget; Teilbudgets je Aufgabentyp optional (leer); Warnschwelle 80 %.
#[test]
fn budget_defaults_follow_the_spec() {
    let d = BudgetLimits::default();
    assert_eq!(d.crawl, MicroUsd(5 * USD));
    assert!(!d.unlimited);
    assert!(
        d.areas.is_empty(),
        "kein Teilbudget „KI gesamt“ im Standard"
    );
    assert!(d.tasks.is_empty(), "keine Aufgaben-Teilbudgets im Standard");
    assert_eq!(d.warning_percent, 80);
    // Dasselbe im Standardprofil.
    assert_eq!(CrawlConfig::default().ai.budget, d);
}

// spec: AI-013
// acceptance: independent
// „0 = keine kostenpflichtigen Aufrufe“ – ein Budget 0 im Profil bleibt 0 und wird nie stillschweigend zu
// „Unbegrenzt“.
#[test]
fn zero_budget_stays_zero_and_not_unlimited() {
    let text = "schema_version = 2\n[ai.budget]\ncrawl = 0\n";
    let loaded = CrawlConfig::load_str(text).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(loaded.config.ai.budget.crawl, MicroUsd(0));
    assert!(!loaded.config.ai.budget.unlimited);
    let again = CrawlConfig::load_str(&loaded.config.to_osconfig_string().unwrap())
        .unwrap()
        .config;
    assert_eq!(again.ai.budget.crawl, MicroUsd(0));
    assert!(!again.ai.budget.unlimited);
}

// spec: AI-019
// acceptance: independent
// Richtlinie: „Keine Einschränkung“ (Standard) / „EU-Verarbeitung erzwingen“ / „Nur lokal“.
#[test]
fn policy_default_is_unrestricted_and_all_three_roundtrip() {
    assert_eq!(CrawlConfig::default().ai.policy, AiPolicy::Unrestricted);
    for p in [
        AiPolicy::Unrestricted,
        AiPolicy::EuOnly,
        AiPolicy::LocalOnly,
    ] {
        let mut c = CrawlConfig::default();
        c.ai.policy = p.clone();
        let back = CrawlConfig::load_str(&c.to_osconfig_string().unwrap())
            .unwrap()
            .config;
        assert_eq!(back.ai.policy, p);
    }
}

// spec: AI-001
// acceptance: independent
// KI im Profil ist standardmäßig aus (kein Lauf ohne ausdrückliche Konfiguration).
#[test]
fn ai_and_embeddings_are_off_in_the_default_profile() {
    let c = CrawlConfig::default();
    assert!(!c.ai.enabled);
    assert!(c.ai.tasks.is_empty());
    assert!(c.ai.active_questions.is_empty());
    assert!(!c.embeddings.enabled);
}

// spec: INTEG-180 [M4]
// acceptance: independent
// INTEG-180: genau die zehn Schlüssel `embeddings.*`; Anbieter, Modell, Dimension und Inhaltsbegrenzung haben
// keine `embeddings.*`-Schlüssel. Standardwerte: Ähnlichkeit 0,95 (INTEG-181), Low Relevance 0,4 (INTEG-182),
// nur indexierbare Seiten an, nur innerhalb gleicher Regel vergleichen aus (INTEG-184).
#[test]
fn embeddings_section_has_exactly_the_spec_keys_and_defaults() {
    let e = CrawlConfig::default().embeddings;
    let v = serde_json::to_value(&e).unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "compare_within_rule",
            "enabled",
            "low_relevance_enabled",
            "low_relevance_threshold",
            "only_indexable",
            "pair_rules",
            "rules",
            "similarity_enabled",
            "similarity_threshold",
            "source_prompt",
        ]
    );
    assert_eq!(e.similarity_threshold, 0.95);
    assert_eq!(e.low_relevance_threshold, 0.4);
    assert!(e.only_indexable);
    assert!(!e.compare_within_rule);
    assert!(e.rules.is_empty() && e.pair_rules.is_empty());
}

// spec: INTEG-180 [M4]
// acceptance: independent
// Die Schlüssel aus INTEG-180 sind in einer Profildatei ohne Warnung lesbar.
#[test]
fn embeddings_keys_from_spec_load_without_warnings() {
    let text = r#"schema_version = 2
[embeddings]
enabled = true
source_prompt = "emb"
similarity_enabled = true
similarity_threshold = 0.5
low_relevance_enabled = true
low_relevance_threshold = 0.1
only_indexable = false
rules = [{ mode = "include", regex = "/blog/" }, { mode = "exclude", regex = "/shop/" }]
compare_within_rule = true
pair_rules = [{ source_regex = "a", target_regex = "b" }]
"#;
    let loaded = CrawlConfig::load_str(text).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let e = &loaded.config.embeddings;
    assert!(e.enabled && e.similarity_enabled && e.low_relevance_enabled && e.compare_within_rule);
    assert!(!e.only_indexable);
    assert_eq!(e.source_prompt.as_deref(), Some("emb"));
    assert_eq!(e.similarity_threshold, 0.5);
    assert_eq!(e.low_relevance_threshold, 0.1);
    assert_eq!(e.rules.len(), 2);
    assert_eq!(e.pair_rules.len(), 1);
}

// spec: AI-011
// acceptance: independent
// Rundreise der vollständigen KI-Konfiguration (Fixture): Fragen, Modellwahl je Aufgabe mit Ersatzliste und
// Parametern, Vorschläge/Stilregeln, Richtlinie, Budget und Teilbudgets, Embeddings – verlustfrei über
// Speichern und Laden (Datei).
#[test]
fn full_ai_profile_fixture_roundtrips_losslessly_through_a_file() {
    let src = fixture("ai-acceptance/profile-ai-full.osconfig");
    let loaded = CrawlConfig::load_file(&src).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let c = &loaded.config;

    // Stichproben der Fixture-Werte.
    assert!(c.ai.enabled);
    assert_eq!(c.ai.active_questions, ["page-type", "ymyl"]);
    assert_eq!(c.ai.questions.len(), 2);
    assert_eq!(c.ai.questions[0]["choices"][1]["value"], "category");
    assert_eq!(c.ai.prompts.len(), 1);
    assert_eq!(c.ai.policy, AiPolicy::EuOnly);
    let decision = &c.ai.tasks["decision"];
    assert_eq!(decision.model.as_ref().unwrap().model, "gpt-6-luna");
    assert_eq!(decision.model.as_ref().unwrap().endpoint, "decisions");
    assert_eq!(decision.fallback.len(), 1);
    assert_eq!(decision.fallback[0].provider, "anthropic");
    let title = &c.ai.tasks["suggest-title"];
    assert_eq!(title.max_tokens, Some(400));
    assert_eq!(title.temperature, Some(0.2));
    assert_eq!(title.effort.as_deref(), Some("low"));
    assert_eq!(title.max_input_chars, Some(8000));
    assert!(c.ai.suggestions.title && c.ai.suggestions.meta && c.ai.suggestions.alt);
    assert!(!c.ai.suggestions.h1);
    assert_eq!(c.ai.suggestions.style.forbidden, ["billig"]);
    assert!(c.ai.suggestions.segment_styles.contains_key("shop"));
    assert_eq!(c.ai.budget.crawl, MicroUsd(10_000));
    assert_eq!(c.ai.budget.warning_percent, 75);
    assert_eq!(c.ai.budget.areas["ai"], MicroUsd(8_000));
    assert_eq!(c.ai.budget.tasks["ai"]["suggest-title"], MicroUsd(500_000));
    assert_eq!(c.embeddings.similarity_threshold, 0.9);
    assert_eq!(c.speed.threads, 3);

    // Speichern → Laden → gleich; Speichern ist stabil.
    let dir = scratch("profile");
    let out = dir.join("roundtrip.osconfig");
    c.save_file(&out).unwrap();
    let reloaded = CrawlConfig::load_file(&out).unwrap();
    assert!(reloaded.warnings.is_empty(), "{:?}", reloaded.warnings);
    assert_eq!(&reloaded.config, c);
    let first = c.to_osconfig_string().unwrap();
    let second = reloaded.config.to_osconfig_string().unwrap();
    assert_eq!(first, second, "Schreiben → Laden → Schreiben stabil");
    let _ = std::fs::remove_dir_all(dir);
}

// spec: AI-011
// acceptance: independent
// AI-011: Modellwahl ist Teil des Konfigurationsprofils **ohne Schlüssel**; Datenhaltung: API-Schlüssel nie in
// Profil/Crawl-Datei. Ein Profil mit KI-Konfiguration enthält keinen Schlüssel-Eintrag.
#[test]
fn saved_ai_profile_contains_no_api_key_fields() {
    let c = CrawlConfig::load_file(&fixture("ai-acceptance/profile-ai-full.osconfig"))
        .unwrap()
        .config;
    let text = c.to_osconfig_string().unwrap().to_lowercase();
    for needle in ["api_key", "apikey", "secret", "password", "authorization"] {
        assert!(!text.contains(needle), "Profil enthält {needle}");
    }
}

// spec: AI-001
// acceptance: independent
// Alte Profile ohne KI-Abschnitt (Archiv-Fixture Stand Profilversion 2) laden ohne Fehler und ohne Warnung;
// KI und Embeddings sind dann aus, alle übrigen Werte bleiben.
#[test]
fn legacy_profile_without_ai_section_loads_cleanly() {
    let path = fixture("migrations/config-v2/profile.osconfig");
    let original = std::fs::read_to_string(&path).unwrap();
    assert!(
        !original.contains("[ai"),
        "Fixture darf keinen KI-Abschnitt haben"
    );
    let loaded = CrawlConfig::load_file(&path).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let c = loaded.config;
    assert!(!c.ai.enabled);
    assert!(!c.embeddings.enabled);
    assert_eq!(c.ai, CrawlConfig::default().ai);
    assert_eq!(c.embeddings, CrawlConfig::default().embeddings);
    // Die Datei selbst wird beim Laden nicht verändert.
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    // Rundreise des aufgewerteten Profils ist verlustfrei.
    let back = CrawlConfig::load_str(&c.to_osconfig_string().unwrap())
        .unwrap()
        .config;
    assert_eq!(back, c);
}

// spec: AI-001
// acceptance: independent
// Profil der ersten Version (Archiv-Fixture) lädt ebenfalls ohne Fehler; KI bleibt aus.
#[test]
fn version_one_profile_loads_with_ai_off() {
    let loaded = CrawlConfig::load_file(&fixture("migrations/config-v1/profile.osconfig")).unwrap();
    assert!(!loaded.config.ai.enabled);
    assert!(!loaded.config.embeddings.enabled);
    assert_eq!(loaded.config.ai, CrawlConfig::default().ai);
}

// spec: AI-001
// acceptance: independent
// Plan M4 E1 / E-2 b: App-Einstellung „KI (Vorschau)“, Standard aus. Alte Einstellungsdateien (Archiv 1.0 und
// eigene Fixture ohne `[ai_app]`) laden ohne Fehler mit Schalter aus und behalten ihre übrigen Werte.
#[test]
fn legacy_settings_without_ai_section_load_with_preview_off() {
    for rel in [
        "migrations/app-v1/settings.toml",
        "ai-acceptance/settings-1.0.toml",
    ] {
        let text = std::fs::read_to_string(fixture(rel)).unwrap();
        assert!(!text.contains("ai_app"));
        let s = AppSettings::from_toml_str(&text).unwrap();
        assert!(!s.ai_app.preview_enabled, "{rel}");
        assert_eq!(s.ai_app, AiAppSettings::default(), "{rel}");
    }
    let s = AppSettings::from_toml_str(
        &std::fs::read_to_string(fixture("ai-acceptance/settings-1.0.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!(s.general.language, os_model::UiLanguage::De);
    assert_eq!(s.appearance.live_interval_ms, 1000);
    // Ohne Datei gelten die Standardwerte (Schalter aus).
    let empty = scratch("settings-none");
    assert!(!AppSettings::load(&empty).unwrap().ai_app.preview_enabled);
    let _ = std::fs::remove_dir_all(empty);
}

// spec: AI-001
// acceptance: independent
// Schalter an bleibt über Speichern und Laden erhalten; Ausschalten ebenso.
#[test]
fn preview_switch_roundtrips_through_settings_file() {
    let on = AppSettings::from_toml_str(
        &std::fs::read_to_string(fixture("ai-acceptance/settings-preview-on.toml")).unwrap(),
    )
    .unwrap();
    assert!(on.ai_app.preview_enabled);
    assert_eq!(on.general.language, os_model::UiLanguage::En);

    let dir = scratch("settings");
    on.save(&dir).unwrap();
    let loaded = AppSettings::load(&dir).unwrap();
    assert!(loaded.ai_app.preview_enabled);
    assert_eq!(loaded, on);

    let mut off = loaded.clone();
    off.ai_app.preview_enabled = false;
    off.save(&dir).unwrap();
    assert!(!AppSettings::load(&dir).unwrap().ai_app.preview_enabled);
    let _ = std::fs::remove_dir_all(dir);
}

// spec: AI-012
// acceptance: independent
// App-Einstellungen KI: „Bestätigung verlangen ab“ 1,00 USD (AI-012), Token-Zählung über Anbieter-API aus
// (AI-012), Schwärzung personenbezogener Daten an (AI-024), Request-Timeout 120 s bzw. 300 s für Ollama
// (INTEG-179), gleichzeitige Anfragen je Anbieter 4 (AI-017), bei Ollama 1 (AI-006), EU-Freischaltung nicht
// bestätigt (AI-019).
#[test]
fn ai_app_settings_defaults_follow_the_spec() {
    let s = AiAppSettings::default();
    assert!(!s.preview_enabled);
    assert_eq!(s.confirmation_threshold, MicroUsd(USD));
    assert!(!s.count_token_sample);
    assert!(s.redact_personal_data);

    let generic = ProviderEndpoint::default();
    assert_eq!(generic.timeout_seconds, 120);
    assert_eq!(generic.concurrent_requests, 4);
    assert!(!generic.eu_account_confirmed);
    assert!(!generic.use_eu_endpoint);

    let ollama = &s.provider_endpoints["ollama"];
    assert_eq!(ollama.timeout_seconds, 300);
    assert_eq!(ollama.concurrent_requests, 1);
    // Die Cloud-Anbieter weichen nicht vom allgemeinen Standard ab.
    for p in ["anthropic", "openai", "openrouter", "gemini"] {
        if let Some(e) = s.provider_endpoints.get(p) {
            assert_eq!(e.timeout_seconds, 120, "{p}");
            assert_eq!(e.concurrent_requests, 4, "{p}");
        }
    }
}

// spec: AI-012
// acceptance: independent
// Vollständige Rundreise des KI-Abschnitts der App-Einstellungen (Datei), mit Werten abseits des Standards.
#[test]
fn ai_app_settings_roundtrip_losslessly() {
    let mut s = AppSettings::default();
    s.ai_app.preview_enabled = true;
    s.ai_app.confirmation_threshold = MicroUsd(100);
    s.ai_app.count_token_sample = true;
    s.ai_app.redact_personal_data = false;
    s.ai_app.redaction_patterns = vec![r"\bDE\d{20}\b".into()];
    s.ai_app.system_prompt = "Antworte knapp.".into();
    s.ai_app.provider_endpoints.insert(
        "openai".into(),
        ProviderEndpoint {
            timeout_seconds: 60,
            concurrent_requests: 2,
            base_url: Some("https://eu.openai.mock.test/v1".into()),
            use_eu_endpoint: true,
            eu_account_confirmed: true,
        },
    );
    let dir = scratch("settings-full");
    s.save(&dir).unwrap();
    let back = AppSettings::load(&dir).unwrap();
    assert_eq!(back.ai_app, s.ai_app);
    assert_eq!(back.to_toml_string().unwrap(), s.to_toml_string().unwrap());
    let _ = std::fs::remove_dir_all(dir);
}
