//! Preistabelle `resources/ai/pricing.json` (AI-010): Laden mit Schemaprüfung, Preisschlüssel (Anbieter, Modell,
//! Endpunkt), Auflösung mit Vorrang Nutzer > Tabelle/API, Hinweis „Preis möglicherweise veraltet“, Fähigkeiten und
//! Reasoning-Stufen der OpenAI-Katalogmodelle (AI-010, Nachtrag Q00).
use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

use crate::adapters::openai_responses::CapabilityLookup;
use crate::cost::rates::{fraction_to_ppm, usd_per_million_to_pico, usd_to_pico, PPM};
use crate::cost::{LongContext, PriceSource, Rates};
use crate::provider::{Capabilities, ReasoningLevel};

use super::{CatalogError, CivilDate, Endpoint};

/// JSON-Schema der Preistabelle (CI-Test AI-010). Liegt bei K04, weil `resources/ai/schemas/` nicht zum Paket gehört
/// (Bericht K04, Tabelle fremder Dateien).
pub const PRICING_SCHEMA: &str = include_str!("schemas/pricing.schema.json");
/// Ausgelieferte Preistabelle.
pub const BUNDLED_PRICING: &str = include_str!("../../../../resources/ai/pricing.json");
/// Umgebungsvariable für eine Testpreisdatei (nur Test- bzw. Debug-Builds, 08 Konventionen).
pub const PRICING_FILE_ENV: &str = "OPENSPIDER_AI_PRICING_FILE";
/// Ab diesem Alter (Tage seit `verified_at`) gilt ein Tabellenpreis als „möglicherweise veraltet“ (AI-010).
pub const STALE_AFTER_DAYS: i64 = 120;

/// Preisschlüssel (AI-010).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PriceKey {
    pub provider: String,
    pub model: String,
    pub endpoint: Endpoint,
}

impl PriceKey {
    pub fn new(provider: &str, model: &str, endpoint: Endpoint) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            endpoint,
        }
    }
}

/// Langkontext-Preise ab einer Schwelle (Faktoren auf die Grundpreise).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LongContextEntry {
    pub threshold_input_tokens: u64,
    pub input_factor: Number,
    pub output_factor: Number,
    /// Fehlt der Wert, gilt der Eingabefaktor (Auslegung K04).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_factor: Option<Number>,
}

/// Fähigkeiten eines Katalogmodells (nur OpenAI und Ollama, AI-010).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CatalogCapabilities {
    pub text: bool,
    pub structured_output: bool,
    pub vision: bool,
    pub embeddings: bool,
    pub decisions: bool,
    pub batch: bool,
}

impl From<CatalogCapabilities> for Capabilities {
    fn from(c: CatalogCapabilities) -> Self {
        Capabilities {
            text: c.text,
            structured_output: c.structured_output,
            vision: c.vision,
            embeddings: c.embeddings,
            decisions: c.decisions,
            batch: c.batch,
            prices_via_api: false,
            local: false,
        }
    }
}

/// Ein Eintrag der Preistabelle. Preise in USD je 1 Mio. Tokens, Gebühren in USD, Anteile als Bruch (0.5 = 50 %).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceEntry {
    pub provider: String,
    pub model: String,
    pub endpoint: Endpoint,
    pub input_per_million: Number,
    pub output_per_million: Number,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_per_million: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_per_million: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_discount: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_context: Option<LongContextEntry>,
    /// Regionalzuschlag je Region (`eu`), als Anteil.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub regional_surcharge: BTreeMap<String, Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_request: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_image: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    pub source_url: String,
    pub verified_at: String,
    /// OpenAI: von der Responses API angenommene Reasoning-Werte (AI-010, Nachtrag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_efforts: Option<Vec<String>>,
    /// OpenAI: Ausgabegrenze des Modells (AI-011, Nachtrag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    /// OpenAI/Ollama: Fähigkeiten des Modells (AI-010).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<CatalogCapabilities>,
    /// Hinweis zur Herkunft einzelner Werte (frei, z. B. „vor dem Release zu prüfen“).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl PriceEntry {
    pub fn key(&self) -> PriceKey {
        PriceKey::new(&self.provider, &self.model, self.endpoint)
    }

    /// Prüfdatum.
    pub fn verified_date(&self) -> Option<CivilDate> {
        CivilDate::parse(&self.verified_at)
    }

    /// Verfügbare Reasoning-Stufen nach `reasoning_efforts` (`none` → `off`), aufsteigend; ohne Feld leer
    /// (= nur `model-default`).
    pub fn reasoning_levels(&self) -> Vec<ReasoningLevel> {
        self.reasoning_efforts
            .as_deref()
            .map(super::reasoning::levels_from_efforts)
            .unwrap_or_default()
    }

    /// Aufgelöste Sätze; `region` wählt den Regionalzuschlag (z. B. `Some("eu")` für die EU-Basis-URL, AI-019).
    pub fn rates(&self, region: Option<&str>) -> Result<Rates, CatalogError> {
        let bad = |field: &str| CatalogError::Invalid(format!("{}: {field}", self.key_text()));
        let per_million = |field: &str, v: Option<&Number>| match v {
            None => Ok(0),
            Some(n) => usd_per_million_to_pico(&n.to_string()).ok_or_else(|| bad(field)),
        };
        let fee = |field: &str, v: Option<&Number>| match v {
            None => Ok(0),
            Some(n) => usd_to_pico(&n.to_string()).ok_or_else(|| bad(field)),
        };
        let ppm =
            |field: &str, n: &Number| fraction_to_ppm(&n.to_string()).ok_or_else(|| bad(field));
        let long_context = match &self.long_context {
            None => None,
            Some(lc) => {
                let input_factor_ppm = ppm("long_context.input_factor", &lc.input_factor)?;
                Some(LongContext {
                    threshold_input_tokens: lc.threshold_input_tokens,
                    input_factor_ppm,
                    output_factor_ppm: ppm("long_context.output_factor", &lc.output_factor)?,
                    cache_read_factor_ppm: match &lc.cache_read_factor {
                        Some(f) => ppm("long_context.cache_read_factor", f)?,
                        None => input_factor_ppm,
                    },
                })
            }
        };
        let regional_uplift_ppm = match region.and_then(|r| self.regional_surcharge.get(r)) {
            Some(n) => ppm("regional_surcharge", n)?,
            None => 0,
        };
        let batch_discount_ppm = match &self.batch_discount {
            Some(n) => ppm("batch_discount", n)?,
            None => 0,
        };
        if batch_discount_ppm > PPM {
            return Err(bad("batch_discount"));
        }
        Ok(Rates {
            input_pico: per_million("input_per_million", Some(&self.input_per_million))?,
            output_pico: per_million("output_per_million", Some(&self.output_per_million))?,
            cache_read_pico: per_million(
                "cache_read_per_million",
                self.cache_read_per_million.as_ref(),
            )?,
            cache_write_pico: per_million(
                "cache_write_per_million",
                self.cache_write_per_million.as_ref(),
            )?,
            batch_discount_ppm,
            long_context,
            regional_uplift_ppm,
            per_request_pico: fee("per_request", self.per_request.as_ref())?,
            per_image_pico: fee("per_image", self.per_image.as_ref())?,
            image_token_estimate: self.image_tokens.unwrap_or(0),
            source: PriceSource::Table,
        })
    }

    fn key_text(&self) -> String {
        format!(
            "{}/{}/{}",
            self.provider,
            self.model,
            self.endpoint.as_str()
        )
    }
}

/// Inhalt der Preisdatei.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceFile {
    /// Nur Testdaten (`pricing-test.json`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub test_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub prices: Vec<PriceEntry>,
}

/// Geprüfte Preistabelle.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceTable {
    file: PriceFile,
    index: BTreeMap<PriceKey, usize>,
}

/// Prüft ein Preisdokument: JSON-Schema (Pflichtfelder, nichtnegative Preise, Werte von `reasoning_efforts`, keine
/// Fähigkeitsfelder bei `anthropic`) und eindeutige Tripel (Anbieter, Modell, Endpunkt). Liefert alle Befunde.
pub fn validate_pricing(doc: &Value) -> Result<(), Vec<String>> {
    let schema: Value =
        serde_json::from_str(PRICING_SCHEMA).expect("Preisschema ist gültiges JSON");
    let validator = crate::schema::ResponseSchema::new(schema).expect("Preisschema kompiliert");
    validator.validate(doc)?;
    let mut seen = BTreeMap::new();
    let mut errors = Vec::new();
    for (i, entry) in doc["prices"].as_array().into_iter().flatten().enumerate() {
        let key = (
            entry["provider"].as_str().unwrap_or_default().to_string(),
            entry["model"].as_str().unwrap_or_default().to_string(),
            entry["endpoint"].as_str().unwrap_or_default().to_string(),
        );
        if let Some(first) = seen.insert(key.clone(), i) {
            errors.push(format!(
                "/prices/{i}: Tripel {}/{}/{} doppelt (zuerst /prices/{first})",
                key.0, key.1, key.2
            ));
        }
        if CivilDate::parse(entry["verified_at"].as_str().unwrap_or_default()).is_none() {
            errors.push(format!("/prices/{i}/verified_at: kein Datum JJJJ-MM-TT"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

impl PriceTable {
    /// Lädt und prüft ein Preisdokument.
    pub fn from_json_str(text: &str) -> Result<Self, CatalogError> {
        let doc: Value =
            serde_json::from_str(text).map_err(|e| CatalogError::Invalid(e.to_string()))?;
        validate_pricing(&doc).map_err(|e| CatalogError::Invalid(e.join("; ")))?;
        let file: PriceFile =
            serde_json::from_value(doc).map_err(|e| CatalogError::Invalid(e.to_string()))?;
        let index = file
            .prices
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key(), i))
            .collect();
        let table = Self { file, index };
        for entry in &table.file.prices {
            entry.rates(None)?;
            for region in entry.regional_surcharge.keys() {
                entry.rates(Some(region))?;
            }
        }
        Ok(table)
    }

    /// Die mitgelieferte Tabelle `resources/ai/pricing.json`.
    pub fn bundled() -> Result<Self, CatalogError> {
        Self::from_json_str(BUNDLED_PRICING)
    }

    /// Mitgelieferte Tabelle bzw. in Test- und Debug-Builds die Datei aus `OPENSPIDER_AI_PRICING_FILE`
    /// (08 Konventionen „Preisdatei für Tests“). In Release-Builds wird die Variable ignoriert.
    pub fn load(env: &dyn Fn(&str) -> Option<String>) -> Result<Self, CatalogError> {
        match env(PRICING_FILE_ENV).filter(|_| cfg!(debug_assertions)) {
            Some(path) => {
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| CatalogError::Io(format!("{path}: {e}")))?;
                Self::from_json_str(&text)
            }
            None => Self::bundled(),
        }
    }

    pub fn file(&self) -> &PriceFile {
        &self.file
    }

    pub fn entries(&self) -> &[PriceEntry] {
        &self.file.prices
    }

    /// Eintrag genau für (Anbieter, Modell, Endpunkt) – kein Rückgriff auf einen anderen Endpunkt (AI-010).
    pub fn get(&self, key: &PriceKey) -> Option<&PriceEntry> {
        self.index.get(key).map(|&i| &self.file.prices[i])
    }

    /// Fähigkeiten der Katalogmodelle eines Endpunkts für die OpenAI-Adapter (`with_capabilities`, K02/K09).
    pub fn capability_lookup(&self, provider: &str, endpoint: Endpoint) -> CapabilityLookup {
        let map: BTreeMap<String, Capabilities> = self
            .entries()
            .iter()
            .filter(|e| e.provider == provider && e.endpoint == endpoint)
            .filter_map(|e| e.capabilities.map(|c| (e.model.clone(), c.into())))
            .collect();
        Arc::new(move |model: &str| map.get(model).copied())
    }

    /// Reasoning-Stufen der Katalogmodelle eines Endpunkts (OpenAI Responses, AI-003/AI-010); unbekanntes Modell →
    /// leer (nur `model-default`).
    pub fn reasoning_lookup(&self, provider: &str, endpoint: Endpoint) -> ReasoningLookup {
        let map: BTreeMap<String, Vec<ReasoningLevel>> = self
            .entries()
            .iter()
            .filter(|e| e.provider == provider && e.endpoint == endpoint)
            .map(|e| (e.model.clone(), e.reasoning_levels()))
            .collect();
        Arc::new(move |model: &str| map.get(model).cloned().unwrap_or_default())
    }

    /// Ausgabegrenze eines Katalogmodells (`max_output_tokens`).
    pub fn max_output_tokens(&self, key: &PriceKey) -> Option<u64> {
        self.get(key).and_then(|e| e.max_output_tokens)
    }
}

/// Reasoning-Stufen je Modell-ID.
pub type ReasoningLookup = Arc<dyn Fn(&str) -> Vec<ReasoningLevel> + Send + Sync>;

/// Nutzerüberschreibung einzelner Preise (Dialog „KI-Anbieter › Preise“). Felder ohne Wert behalten den Preis aus
/// Tabelle bzw. API; gibt es dort keinen Eintrag, gelten sie als 0. Dezimaltexte wie in der Tabelle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PriceOverride {
    pub input_per_million: Option<String>,
    pub output_per_million: Option<String>,
    pub cache_read_per_million: Option<String>,
    pub cache_write_per_million: Option<String>,
    pub per_request: Option<String>,
    pub per_image: Option<String>,
}

impl PriceOverride {
    fn apply(&self, base: &Rates) -> Result<Rates, CatalogError> {
        let pm = |v: &Option<String>, old: u64| match v {
            None => Ok(old),
            Some(t) => usd_per_million_to_pico(t)
                .ok_or_else(|| CatalogError::Invalid(format!("Preis {t:?} ungültig"))),
        };
        let fee = |v: &Option<String>, old: u64| match v {
            None => Ok(old),
            Some(t) => {
                usd_to_pico(t).ok_or_else(|| CatalogError::Invalid(format!("Preis {t:?} ungültig")))
            }
        };
        Ok(Rates {
            input_pico: pm(&self.input_per_million, base.input_pico)?,
            output_pico: pm(&self.output_per_million, base.output_pico)?,
            cache_read_pico: pm(&self.cache_read_per_million, base.cache_read_pico)?,
            cache_write_pico: pm(&self.cache_write_per_million, base.cache_write_pico)?,
            per_request_pico: fee(&self.per_request, base.per_request_pico)?,
            per_image_pico: fee(&self.per_image, base.per_image_pico)?,
            source: PriceSource::User,
            ..base.clone()
        })
    }
}

/// Aufgelöster Preis mit Quelle und Prüfdatum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPrice {
    pub rates: Rates,
    /// `table` bzw. `api` bzw. `user` (Spalte `Price Source`).
    pub source: PriceSource,
    /// Prüfdatum des Tabelleneintrags (bei `user` das des überschriebenen Tabelleneintrags, falls vorhanden).
    pub verified_at: Option<CivilDate>,
}

impl ResolvedPrice {
    /// Hinweis „Preis möglicherweise veraltet“ (AI-010): Tabellenpreis mit `verified_at` älter als 120 Tage.
    pub fn possibly_stale(&self, today: CivilDate) -> bool {
        self.source == PriceSource::Table
            && self
                .verified_at
                .is_some_and(|v| today.days_since(v) > STALE_AFTER_DAYS)
    }
}

/// Preiskatalog: Tabelle, Preise aus Anbieter-APIs (OpenRouter) und Nutzerüberschreibungen.
#[derive(Debug, Clone)]
pub struct PriceCatalog {
    table: PriceTable,
    api: BTreeMap<PriceKey, Rates>,
    overrides: BTreeMap<PriceKey, PriceOverride>,
}

impl PriceCatalog {
    pub fn new(table: PriceTable) -> Self {
        Self {
            table,
            api: BTreeMap::new(),
            overrides: BTreeMap::new(),
        }
    }

    pub fn table(&self) -> &PriceTable {
        &self.table
    }

    /// Preise aus einer Anbieter-API (OpenRouter-Modellliste, AI-005); ersetzt frühere API-Preise des Schlüssels.
    pub fn set_api_rates(&mut self, key: PriceKey, mut rates: Rates) {
        rates.source = PriceSource::Api;
        self.api.insert(key, rates);
    }

    /// Nutzerüberschreibung setzen (Vorrang vor Tabelle und API).
    pub fn set_override(
        &mut self,
        key: PriceKey,
        price: PriceOverride,
    ) -> Result<(), CatalogError> {
        price.apply(&Rates::free(PriceSource::User))?;
        self.overrides.insert(key, price);
        Ok(())
    }

    pub fn overrides(&self) -> &BTreeMap<PriceKey, PriceOverride> {
        &self.overrides
    }

    /// „Preise zurücksetzen“: entfernt alle Überschreibungen.
    pub fn reset_overrides(&mut self) {
        self.overrides.clear();
    }

    /// Preis für (Anbieter, Modell, Endpunkt) und Region; `None` = „Preis unbekannt“.
    pub fn resolve(
        &self,
        key: &PriceKey,
        region: Option<&str>,
    ) -> Result<Option<ResolvedPrice>, CatalogError> {
        let entry = self.table.get(key);
        let base = match (self.api.get(key), entry) {
            (Some(api), _) => Some(api.clone()),
            (None, Some(e)) => Some(e.rates(region)?),
            (None, None) => None,
        };
        let verified_at = entry.and_then(PriceEntry::verified_date);
        match (self.overrides.get(key), base) {
            (Some(o), base) => {
                let base = base.unwrap_or_else(|| Rates::free(PriceSource::User));
                Ok(Some(ResolvedPrice {
                    rates: o.apply(&base)?,
                    source: PriceSource::User,
                    verified_at,
                }))
            }
            (None, Some(rates)) => Ok(Some(ResolvedPrice {
                source: rates.source,
                rates,
                verified_at: if self.api.contains_key(key) {
                    None
                } else {
                    verified_at
                },
            })),
            (None, None) => Ok(None),
        }
    }
}
