//! Umgebung eines KI-Laufs: Anbieter (Schlüssel, Basis-URLs), Routing, Preise, Einwilligung, Schwärzung, Begrenzung,
//! Schwelle der Kostenbestätigung, Uhr und Besitzerart.
//!
//! Auflage E8 (K4-29, 08 AI-020 „Kontingentsperre je KI-Lauf“): [`ProviderSetup::adapters`] legt je KI-Lauf **eigene**
//! Adapter-Instanzen an. Die Kontingentsperren (`QuotaLatch`, `OpenAiAccount`, `UnreachableLatch`) hängen an diesen
//! Instanzen; zwei gleichzeitige Läufe teilen deshalb keine Sperre, und ein später gestarteter Lauf beginnt ohne Sperre.
use std::collections::BTreeMap;
use std::sync::Arc;

use os_model::budget::MicroUsd;
use os_model::config::AiAppSettings;
use os_model::Clock;

use crate::adapters::anthropic::{AnthropicAdapter, AnthropicConfig};
use crate::adapters::ollama::{OllamaAdapter, OllamaConfig};
use crate::adapters::openai_decisions::OpenAiDecisions;
use crate::adapters::openai_responses::{OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses};
use crate::adapters::openrouter::{OpenRouterAdapter, OpenRouterConfig};
use crate::catalog::{
    Endpoint, PriceKey, PriceTable, ProviderState, TaskDefaults, TaskSettings, TaskType,
};
use crate::cost::Rates;
use crate::decisions::exec::{Adapters, PriceLookup};
use crate::decisions::routing::{RoutingContext, RoutingPolicy};
use crate::keys::{AiKeyProvider, AiKeyStore};
use crate::limits::{Limiter, LimitsConfig, TokioClock};
use crate::policy::{ConsentGate, RedactionSettings, Redactor};
use crate::PreviewSwitch;

/// Anbieter der KI-Läufe.
pub const PROVIDERS: [&str; 4] = ["openai", "anthropic", "openrouter", "ollama"];

/// Umgebungsvariable der Basis-URL eines Anbieters (INTEG-019: `OPENSPIDER_API_BASE_<ANBIETER>`; nur in Test- und
/// Debug-Builds wirksam, wie die API-Quellen aus 04).
pub fn base_url_env(provider: &str) -> String {
    format!("OPENSPIDER_API_BASE_{}", provider.to_uppercase())
}

/// Zugang zu den Anbietern: HTTP-Client, Schlüssel und Basis-URLs (ohne Basis-URL gilt die Standardadresse des
/// Adapters). Schlüssel erscheinen nie in `Debug`.
#[derive(Clone)]
pub struct ProviderSetup {
    pub client: reqwest::Client,
    /// Anbieter → Schlüssel (`openai`, `anthropic`, `openrouter`).
    pub keys: BTreeMap<String, String>,
    /// Anbieter → Basis-URL (OpenAI einschließlich `/v1`, OpenRouter einschließlich `/api/v1`, Anthropic und Ollama
    /// ohne Pfad).
    pub base_urls: BTreeMap<String, String>,
    /// Ollama verwenden (lokal, ohne Schlüssel).
    pub ollama: bool,
}

impl std::fmt::Debug for ProviderSetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderSetup")
            .field("keys", &self.keys.keys().collect::<Vec<_>>())
            .field("base_urls", &self.base_urls)
            .field("ollama", &self.ollama)
            .finish()
    }
}

impl ProviderSetup {
    /// Schlüssel aus dem Schlüsselbund bzw. `OPENSPIDER_AI_KEY_<ANBIETER>` (AI-009), Basis-URLs aus den App-Einstellungen
    /// (`provider_endpoints[..].base_url`) und – nur in Test- und Debug-Builds – aus `OPENSPIDER_API_BASE_<ANBIETER>`.
    pub fn from_store(keys: &AiKeyStore, settings: &AiAppSettings) -> Self {
        let mut found = BTreeMap::new();
        for p in AiKeyProvider::ALL {
            if let Ok(Some(k)) = keys.resolve(p) {
                found.insert(p.id().to_string(), k.secret.expose().to_string());
            }
        }
        let mut base_urls = BTreeMap::new();
        for p in PROVIDERS {
            let configured = settings
                .provider_endpoints
                .get(p)
                .and_then(|e| e.base_url.as_deref())
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .map(str::to_string);
            let env = cfg!(debug_assertions)
                .then(|| std::env::var(base_url_env(p)).ok())
                .flatten()
                .filter(|u| !u.trim().is_empty());
            if let Some(url) = env.or(configured) {
                base_urls.insert(p.to_string(), url);
            }
        }
        Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap_or_default(),
            keys: found,
            ollama: base_urls.contains_key("ollama"),
            base_urls,
        }
    }

    /// Schlüssel vorhanden (Ollama: nur mit eingestellter Server-URL).
    pub fn has_key(&self, provider: &str) -> bool {
        match provider {
            "ollama" => self.ollama,
            p => self.keys.contains_key(p),
        }
    }

    /// Neue Adapter-Instanzen für **einen** KI-Lauf (Auflage E8).
    pub fn adapters(&self) -> AdapterSet {
        let base = |p: &str| self.base_urls.get(p).cloned();
        let openai = self.keys.get("openai").map(|key| {
            let mut config =
                OpenAiConfig::new(crate::adapters::openai_responses::ApiKey::new(key.clone()));
            if let Some(url) = base("openai") {
                config = config.with_base_url(url);
            }
            OpenAiHttp::new(self.client.clone(), config, OpenAiAccount::new())
        });
        AdapterSet {
            decisions: openai.clone().map(OpenAiDecisions::new),
            responses: openai.map(OpenAiResponses::new),
            anthropic: self.keys.get("anthropic").map(|key| {
                let mut config =
                    AnthropicConfig::new(crate::adapters::anthropic::ApiKey::new(key.clone()));
                if let Some(url) = base("anthropic") {
                    config = config.with_base_url(url);
                }
                AnthropicAdapter::new(self.client.clone(), config)
            }),
            openrouter: self.keys.get("openrouter").map(|key| {
                let mut config =
                    OpenRouterConfig::new(crate::adapters::openrouter::ApiKey::new(key.clone()));
                if let Some(url) = base("openrouter") {
                    config = config.with_base_url(url);
                }
                OpenRouterAdapter::new(self.client.clone(), config)
            }),
            ollama: self.ollama.then(|| {
                let mut config = OllamaConfig::new();
                if let Some(url) = base("ollama") {
                    config = config.with_base_url(url);
                }
                OllamaAdapter::new(self.client.clone(), config)
            }),
        }
    }
}

/// Adapter-Instanzen eines KI-Laufs.
pub struct AdapterSet {
    pub decisions: Option<OpenAiDecisions>,
    pub responses: Option<OpenAiResponses>,
    pub anthropic: Option<AnthropicAdapter>,
    pub openrouter: Option<OpenRouterAdapter>,
    pub ollama: Option<OllamaAdapter>,
}

impl AdapterSet {
    /// Sicht für `DecisionRun::adapters`.
    pub fn view(&self) -> Adapters<'_> {
        Adapters {
            decisions: self.decisions.as_ref(),
            responses: self.responses.as_ref(),
            anthropic: self.anthropic.as_ref(),
            openrouter: self.openrouter.as_ref(),
            ollama: self.ollama.as_ref(),
        }
    }
}

/// Preise aus der Preistabelle (mitgeliefert bzw. in Test- und Debug-Builds `OPENSPIDER_AI_PRICING_FILE`).
#[derive(Debug, Clone)]
pub struct TablePrices(pub PriceTable);

impl TablePrices {
    /// Tabelle nach AI-010 (Testdatei nur in Test- und Debug-Builds).
    pub fn load() -> Result<Self, String> {
        PriceTable::load(&|k| std::env::var(k).ok())
            .map(Self)
            .map_err(|e| e.to_string())
    }
}

impl PriceLookup for TablePrices {
    fn rates(&self, provider: &str, model: &str, endpoint: &str) -> Option<Rates> {
        let endpoint = Endpoint::parse(endpoint)?;
        self.0
            .get(&PriceKey::new(provider, model, endpoint))
            .and_then(|e| e.rates(None).ok())
    }
}

/// Alles, was ein KI-Lauf über die Crawl-Datei hinaus braucht. Wird je Lauf bzw. je Sitzung der Warteschlange gebaut.
#[derive(Clone)]
pub struct RunEnv {
    pub providers: ProviderSetup,
    /// Anbieterzustand für das Routing (Schlüssel und bekannte Modelle).
    pub provider_states: BTreeMap<String, ProviderState>,
    pub defaults: TaskDefaults,
    /// Einstellungen je Aufgabe aus dem Profil (`ai.tasks`).
    pub task_settings: BTreeMap<TaskType, TaskSettings>,
    pub policy: RoutingPolicy,
    pub limits: LimitsConfig,
    pub prices: Arc<dyn PriceLookup + Send + Sync>,
    pub consent: ConsentGate,
    pub switch: PreviewSwitch,
    pub redaction: RedactionSettings,
    /// „Bestätigung verlangen ab“ (AI-012).
    pub threshold: MicroUsd,
    /// Höchstzahl gleichzeitig laufender Requests eines Laufs.
    pub concurrency: usize,
    /// Zeitstempel der Protokollzeilen und der Besitzerangaben.
    pub clock: Arc<dyn Clock>,
    /// `owner_kind` des Prozesses (`app`, `cli`, `mcp-stdio`).
    pub owner_kind: &'static str,
}

impl std::fmt::Debug for RunEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunEnv")
            .field("providers", &self.providers)
            .field("switch", &self.switch)
            .field("threshold", &self.threshold)
            .field("owner_kind", &self.owner_kind)
            .finish_non_exhaustive()
    }
}

impl RunEnv {
    /// Routing-Kontext (AI-035).
    pub fn routing(&self) -> RoutingContext<'_> {
        RoutingContext {
            defaults: &self.defaults,
            settings: &self.task_settings,
            providers: &self.provider_states,
            policy: self.policy,
        }
    }

    /// Begrenzung eines Laufs (eigene Instanz je Lauf).
    pub fn limiter(&self) -> Limiter {
        Limiter::new(self.limits.clone(), Arc::new(TokioClock::new()))
    }

    /// Schwärzung (AI-024).
    pub fn redactor(&self) -> Redactor {
        Redactor::new(self.redaction.clone())
    }

    /// Anbieterzustand aus Katalogmodellen (Preistabelle mit Fähigkeiten) und den Schlüsseln; für CLI und MCP, die keine
    /// Live-Modelllisten kennen (U01/P01 liefern sie in der App).
    pub fn catalog_states(
        prices: &PriceTable,
        providers: &ProviderSetup,
    ) -> BTreeMap<String, ProviderState> {
        let mut out: BTreeMap<String, ProviderState> = PROVIDERS
            .iter()
            .map(|p| {
                (
                    p.to_string(),
                    ProviderState {
                        has_key: providers.has_key(p),
                        models: Vec::new(),
                    },
                )
            })
            .collect();
        for entry in prices.entries() {
            let Some(caps) = entry.capabilities else {
                continue;
            };
            if let Some(state) = out.get_mut(&entry.provider) {
                state.models.push(crate::catalog::AvailableModel {
                    model: crate::provider::Model {
                        provider: entry.provider.clone(),
                        id: entry.model.clone(),
                        endpoint: entry.endpoint.as_str().to_string(),
                        capabilities: caps.into(),
                        capability_source: crate::provider::CapabilitySource::ModelCatalog,
                        reasoning_levels: entry.reasoning_levels(),
                    },
                    max_output_tokens: entry.max_output_tokens,
                });
            }
        }
        out
    }

    /// Umgebung aus den App-Einstellungen für CLI und MCP STDIO (Schlüssel aus `keys`, Einwilligung aus `consent`,
    /// Anbieterzustand aus dem Katalog).
    pub fn from_settings(
        settings: &AiAppSettings,
        config: &os_model::CrawlConfig,
        keys: &AiKeyStore,
        consent: ConsentGate,
        clock: Arc<dyn Clock>,
        owner_kind: &'static str,
    ) -> Result<Self, String> {
        let providers = ProviderSetup::from_store(keys, settings);
        let prices = TablePrices::load()?;
        let provider_states = Self::catalog_states(&prices.0, &providers);
        Ok(Self {
            provider_states,
            defaults: TaskDefaults::bundled().map_err(|e| e.to_string())?,
            task_settings: crate::catalog::choice::settings_from_profile(&config.ai.tasks)
                .map_err(|e| e.to_string())?,
            policy: RoutingPolicy::default(),
            limits: LimitsConfig::from_app_settings(settings),
            prices: Arc::new(prices),
            consent,
            switch: PreviewSwitch::from_ai_settings(settings),
            redaction: RedactionSettings::from_app_settings(settings).unwrap_or_else(|_| {
                RedactionSettings::from_app_settings_with(
                    &AiAppSettings {
                        redaction_patterns: Vec::new(),
                        ..settings.clone()
                    },
                    &crate::policy::FancyRegexEngine,
                )
                .unwrap_or_else(|_| RedactionSettings::off())
            }),
            threshold: settings.confirmation_threshold,
            concurrency: usize::from(
                settings
                    .provider_endpoints
                    .get("openai")
                    .map_or(4, |e| e.concurrent_requests)
                    .max(1),
            ),
            providers,
            clock,
            owner_kind,
        })
    }
}
