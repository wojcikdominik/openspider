//! Gemeinsame Helfer der unabhängigen Abnahmetests M4-K05 (eigener Preis-Loader, eigene Crawl-Datei).
//!
//! Preise kommen aus der Fixture `tests/fixtures/ai/pricing-test.json` (Spec 08 „Konventionen“) und werden hier
//! unabhängig vom Produktcode in die ganzzahligen Sätze von `os_ai::cost::Rates` umgerechnet:
//! 1 USD je 1 Mio. Tokens = 1 µUSD je Token = 1.000.000 pUSD je Token; Anteile/Faktoren in ppm.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use os_ai::cost::{LongContext, PriceSource, Rates};
use serde_json::Value;

pub const PPM: u64 = 1_000_000;

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Dezimaltext × 10^digits als Ganzzahl; nur exakt darstellbare Werte (sonst Panik, damit kein Rundungsfehler
/// unbemerkt in einen Prüfwert gerät).
pub fn scaled(text: &str, digits: u32) -> u64 {
    let (int, frac) = text.split_once('.').unwrap_or((text, ""));
    assert!(
        frac.len() as u32 <= digits,
        "{text} nicht exakt mit {digits} Stellen"
    );
    let int: u64 = if int.is_empty() {
        0
    } else {
        int.parse().unwrap()
    };
    let mut frac_v: u64 = if frac.is_empty() {
        0
    } else {
        frac.parse().unwrap()
    };
    for _ in frac.len() as u32..digits {
        frac_v *= 10;
    }
    int * 10u64.pow(digits) + frac_v
}

fn num_text(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => panic!("keine Zahl: {other}"),
    }
}

/// USD je 1 Mio. Tokens → pUSD je Token.
fn per_million(v: Option<&Value>) -> u64 {
    v.map(|v| scaled(&num_text(v), 6)).unwrap_or(0)
}
/// Anteil/Faktor → ppm.
fn ppm(v: Option<&Value>, default: u64) -> u64 {
    v.map(|v| scaled(&num_text(v), 6)).unwrap_or(default)
}

pub fn pricing_fixture() -> Value {
    let path = repo_root().join("tests/fixtures/ai/pricing-test.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Sätze für (Anbieter, Modell, Endpunkt) aus `pricing-test.json`; `eu` = Basis-URL mit Datenresidenz.
/// `None`, wenn kein Eintrag für genau diesen Endpunkt existiert (AI-010: kein Rückgriff).
pub fn fixture_rates(provider: &str, model: &str, endpoint: &str, eu: bool) -> Option<Rates> {
    let doc = pricing_fixture();
    let e =
        doc["prices"].as_array().unwrap().iter().find(|e| {
            e["provider"] == provider && e["model"] == model && e["endpoint"] == endpoint
        })?;
    let long_context = e.get("long_context").map(|lc| LongContext {
        threshold_input_tokens: lc["threshold_input_tokens"].as_u64().unwrap(),
        input_factor_ppm: ppm(lc.get("input_factor"), PPM),
        output_factor_ppm: ppm(lc.get("output_factor"), PPM),
        // Fixture nennt keinen Cache-Lese-Faktor; 1,0 (in den Tests ohne Cache-Lesen im Langkontext).
        cache_read_factor_ppm: ppm(lc.get("cache_read_factor"), PPM),
    });
    let uplift = if eu {
        ppm(e.get("regional_surcharge").and_then(|r| r.get("eu")), 0)
    } else {
        0
    };
    Some(Rates {
        input_pico: per_million(e.get("input_per_million")),
        output_pico: per_million(e.get("output_per_million")),
        cache_read_pico: per_million(e.get("cache_read_per_million")),
        cache_write_pico: per_million(e.get("cache_write_per_million")),
        batch_discount_ppm: ppm(e.get("batch_discount"), 0),
        long_context,
        regional_uplift_ppm: uplift,
        per_request_pico: 0,
        per_image_pico: 0,
        image_token_estimate: 0,
        source: PriceSource::Table,
    })
}

/// OpenRouter-Vision-Modell der Mock-Modellliste (Spec 08 „Konventionen“, AI-012/AI-013):
/// `pricing.request = 0.001`, `pricing.image = 0.002` USD; Tokenpreise wie die Mock-Liste (1 / 5 USD je 1 Mio.).
/// Preisquelle `api` (AI-005).
pub fn openrouter_vision() -> Rates {
    Rates {
        input_pico: 1_000_000,
        output_pico: 5_000_000,
        cache_read_pico: 0,
        cache_write_pico: 0,
        batch_discount_ppm: 0,
        long_context: None,
        regional_uplift_ppm: 0,
        per_request_pico: scaled("0.001", 12),
        per_image_pico: scaled("0.002", 12),
        image_token_estimate: 85,
        source: PriceSource::Api,
    }
}

/// Temporäres Verzeichnis ohne zusätzliche Abhängigkeit; wird beim Drop gelöscht.
pub struct TempDir(pub PathBuf);
impl TempDir {
    pub fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Laufende Nummer: Die Systemuhr allein trennt gleichzeitig startende Tests nicht sicher (macOS: Mikrosekunden).
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!(
            "os-ai-k05-acc-{tag}-{}-{nanos}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Neue Crawl-Datei mit einem KI-Lauf (id 1).
pub fn crawl_file(dir: &Path) -> os_store::CrawlFile {
    use os_model::time::{IdSource, SeededIds};
    let created = os_model::Timestamp::from_db("2026-11-24T10:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: SeededIds::new(77).new_crawl_id(created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "Abnahme".into(),
        name: "K05".into(),
        config: os_model::CrawlConfig::default(),
        created_at: created,
    };
    let f = os_store::CrawlFile::create(
        &dir.join("acc.ospider"),
        &init,
        os_store::StorageProfile::Balanced,
    )
    .unwrap();
    f.conn()
        .execute_batch(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES(1, '2026-11-24T10:00:00Z', '{}');",
        )
        .unwrap();
    f
}

pub fn ctx(task: &str, provider: &str, model: &str, endpoint: &str) -> os_ai::cost::LogContext {
    os_ai::cost::LogContext {
        run_id: 1,
        timestamp: "2026-11-24T10:01:00Z".into(),
        task: task.into(),
        provider: provider.into(),
        model: model.into(),
        endpoint: endpoint.into(),
        url_id: None,
        definition_version: Some("1".into()),
        batch_id: None,
        redactions: 0,
    }
}

pub fn usage(input: u64, output: u64) -> os_ai::provider::Usage {
    os_ai::provider::Usage {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: None,
    }
}

/// Einfacher deterministischer Zufall (LCG).
pub struct Lcg(pub u64);
impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
}
