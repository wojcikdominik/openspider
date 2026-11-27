//! Gemeinsame Helfer der `cost_*`-Tests (M4-K05); wird per `#[path]` eingebunden und enthält selbst keine Tests.
//!
//! Preise kommen aus `tests/fixtures/ai/pricing-test.json` (T01) und `tests/fixtures/ai/k05/openrouter-models.json`;
//! die Tokenzählung bildet die deterministische Zählung von ai-mock nach (08 Konventionen: ⌈Zeichen / 4⌉ Text,
//! 85 je Bild). Seiten der Fixture-Site `tests/fixtures/ai/site/` liefern die Eingaben.
#![allow(dead_code)]

use os_ai::cost::rates::{fraction_to_ppm, usd_per_million_to_pico, usd_to_pico};
use os_ai::cost::{LongContext, PriceSource, Rates};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn fixture(parts: &[&str]) -> PathBuf {
    let mut p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("ai");
    for part in parts {
        p = p.join(part);
    }
    p
}

fn read_json(parts: &[&str]) -> Value {
    serde_json::from_str(&std::fs::read_to_string(fixture(parts)).unwrap()).unwrap()
}

fn num(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => panic!("kein Preis: {other}"),
    }
}

fn per_million(entry: &Value, key: &str) -> u64 {
    entry
        .get(key)
        .map(|v| usd_per_million_to_pico(&num(v)).unwrap())
        .unwrap_or(0)
}

/// Sätze aus `pricing-test.json` für (Anbieter, Modell, Endpunkt); `eu` wählt den Regionalzuschlag.
pub fn test_rates(provider: &str, model: &str, endpoint: &str, eu: bool) -> Rates {
    let doc = read_json(&["pricing-test.json"]);
    let entry = doc["prices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["provider"] == provider && e["model"] == model && e["endpoint"] == endpoint)
        .unwrap_or_else(|| panic!("kein Testpreis für {provider}/{model}/{endpoint}"));
    let ppm = |v: Option<&Value>| v.map(|v| fraction_to_ppm(&num(v)).unwrap());
    Rates {
        input_pico: per_million(entry, "input_per_million"),
        output_pico: per_million(entry, "output_per_million"),
        cache_read_pico: per_million(entry, "cache_read_per_million"),
        cache_write_pico: per_million(entry, "cache_write_per_million"),
        batch_discount_ppm: ppm(entry.get("batch_discount")).unwrap_or(0),
        long_context: entry.get("long_context").map(|lc| LongContext {
            threshold_input_tokens: lc["threshold_input_tokens"].as_u64().unwrap(),
            input_factor_ppm: ppm(lc.get("input_factor")).unwrap_or(1_000_000),
            output_factor_ppm: ppm(lc.get("output_factor")).unwrap_or(1_000_000),
            cache_read_factor_ppm: ppm(lc.get("cache_read_factor")).unwrap_or(1_000_000),
        }),
        regional_uplift_ppm: if eu {
            ppm(entry.get("regional_surcharge").and_then(|r| r.get("eu"))).unwrap_or(0)
        } else {
            0
        },
        per_request_pico: 0,
        per_image_pico: 0,
        image_token_estimate: 85,
        source: PriceSource::Table,
    }
}

/// Sätze eines OpenRouter-Modells aus der Modellliste (`pricing.*` in USD je Token bzw. Request/Bild).
pub fn openrouter_rates(model: &str) -> Rates {
    let doc = read_json(&["k05", "openrouter-models.json"]);
    let m = doc["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == model)
        .unwrap();
    let p = |k: &str| usd_to_pico(m["pricing"][k].as_str().unwrap()).unwrap();
    Rates {
        input_pico: p("prompt"),
        output_pico: p("completion"),
        per_request_pico: p("request"),
        per_image_pico: p("image"),
        image_token_estimate: 85,
        ..Rates::free(PriceSource::Api)
    }
}

/// Eingabe-Tokens nach ai-mock: ⌈Zeichen aller Textteile / 4⌉ + 85 je Bild.
pub fn mock_input_tokens(chars: u64, images: u64) -> u64 {
    chars.div_ceil(4) + 85 * images
}

/// Eine Seite der Fixture-Site.
pub struct Page {
    pub url: String,
    pub mock_id: String,
    pub in_scope: bool,
    pub text: String,
}

/// Alle 25 Seiten-URLs (G) der Fixture-Site mit Kennzeichen „im Standard-Geltungsbereich S“.
pub fn site_pages(site: &str) -> Vec<Page> {
    let manifest = read_json(&[site, "manifest.json"]);
    manifest["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let file = p["file"].as_str().unwrap();
            let mut path = fixture(&[site]);
            for part in file.split('/') {
                path = path.join(part);
            }
            Page {
                url: p["url"].as_str().unwrap().to_string(),
                mock_id: p["mock_id"].as_str().unwrap_or_default().to_string(),
                in_scope: p["status"] == 200 && p["indexable"] == true,
                text: std::fs::read_to_string(path).unwrap_or_default(),
            }
        })
        .collect()
}

/// Temporäres Verzeichnis ohne zusätzliche Abhängigkeit (wird beim Verlassen entfernt).
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
            "os-ai-k05-{tag}-{}-{nanos}-{n}",
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

/// Neue Crawl-Datei mit einem KI-Lauf (`ai_runs.id = 1`) und einer URL (`urls.id = 1`).
pub fn crawl_file(dir: &Path) -> os_store::CrawlFile {
    use os_model::time::{IdSource, SeededIds};
    let created = os_model::Timestamp::from_db("2026-11-24T08:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: SeededIds::new(9).new_crawl_id(created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "K05".into(),
        name: "cost".into(),
        config: os_model::CrawlConfig::default(),
        created_at: created,
    };
    let f = os_store::CrawlFile::create(
        &dir.join("cost.ospider"),
        &init,
        os_store::StorageProfile::Balanced,
    )
    .unwrap();
    f.conn()
        .execute_batch(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES(1, '2026-11-24T08:00:00Z', '{}');
             INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length)
               VALUES(1,'https://ai.test:8080/','https://ai.test:8080/','https://ai.test:8080',1,'Done','Seed',0,21);",
        )
        .unwrap();
    f
}
