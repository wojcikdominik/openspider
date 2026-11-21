//! Profile-only AI configuration, containing no credentials or consent grants.
use crate::budget::{BudgetLimits, MicroUsd};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AiConfig {
    pub enabled: bool,
    /// Definitions remain JSON documents (AI-030); validation belongs to M4-D01.
    pub questions: Vec<serde_json::Value>,
    pub active_questions: Vec<String>,
    pub prompts: Vec<serde_json::Value>,
    pub tasks: BTreeMap<String, TaskModelConfig>,
    pub suggestions: SuggestionConfig,
    pub policy: AiPolicy,
    pub budget: BudgetLimits,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ModelChoice {
    pub provider: String,
    pub model: String,
    pub endpoint: String,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TaskModelConfig {
    pub model: Option<ModelChoice>,
    pub fallback: Vec<ModelChoice>,
    pub max_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub effort: Option<String>,
    pub max_input_chars: Option<u64>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AiPolicy {
    #[default]
    Unrestricted,
    EuOnly,
    LocalOnly,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SuggestionConfig {
    pub title: bool,
    pub meta: bool,
    pub h1: bool,
    pub alt: bool,
    pub style: StyleRules,
    pub segment_styles: BTreeMap<String, StyleRules>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct StyleRules {
    pub brand: String,
    pub separator: String,
    pub brand_position: BrandPosition,
    pub tone: String,
    pub forbidden: Vec<String>,
    pub required: Vec<String>,
    pub casing: Casing,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BrandPosition {
    Start,
    End,
    #[default]
    None,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Casing {
    Sentence,
    Title,
    #[default]
    Unchanged,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct EmbeddingsConfig {
    pub enabled: bool,
    pub source_prompt: Option<String>,
    pub similarity_enabled: bool,
    pub similarity_threshold: f64,
    pub low_relevance_enabled: bool,
    pub low_relevance_threshold: f64,
    pub only_indexable: bool,
    pub rules: Vec<EmbeddingRule>,
    pub compare_within_rule: bool,
    pub pair_rules: Vec<EmbeddingPairRule>,
}
impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            source_prompt: None,
            similarity_enabled: false,
            similarity_threshold: 0.95,
            low_relevance_enabled: false,
            low_relevance_threshold: 0.4,
            only_indexable: true,
            rules: Vec::new(),
            compare_within_rule: false,
            pair_rules: Vec::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingRuleMode {
    Include,
    Exclude,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddingRule {
    pub mode: EmbeddingRuleMode,
    pub regex: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddingPairRule {
    pub source_regex: String,
    pub target_regex: String,
}
/// App-level preferences are deliberately separate from portable crawl profiles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AiAppSettings {
    /// Schalter „KI (Vorschau)“ (Plan M4 E1, E-2 b): Standard aus; aus = kein KI-Request, kein KI-Schritt.
    pub preview_enabled: bool,
    pub confirmation_threshold: MicroUsd,
    pub count_token_sample: bool,
    pub redact_personal_data: bool,
    pub redaction_patterns: Vec<String>,
    pub system_prompt: String,
    pub provider_endpoints: BTreeMap<String, ProviderEndpoint>,
}
impl Default for AiAppSettings {
    fn default() -> Self {
        Self {
            preview_enabled: false,
            confirmation_threshold: MicroUsd(1_000_000),
            count_token_sample: false,
            redact_personal_data: true,
            redaction_patterns: Vec::new(),
            system_prompt: String::new(),
            provider_endpoints: BTreeMap::from([(
                String::from("ollama"),
                ProviderEndpoint {
                    timeout_seconds: 300,
                    concurrent_requests: 1,
                    ..ProviderEndpoint::default()
                },
            )]),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ProviderEndpoint {
    pub timeout_seconds: u16,
    pub concurrent_requests: u16,
    pub base_url: Option<String>,
    pub use_eu_endpoint: bool,
    pub eu_account_confirmed: bool,
}
impl Default for ProviderEndpoint {
    fn default() -> Self {
        Self {
            timeout_seconds: 120,
            concurrent_requests: 4,
            base_url: None,
            use_eu_endpoint: false,
            eu_account_confirmed: false,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::CrawlConfig;
    // spec: AI-001
    // spec: AI-013
    // spec: INTEG-180 [M4]
    #[test]
    fn profile_fixture_preserves_task_fallback_questions_and_embedding_rules() {
        let fixture = r#"schema_version = 2
[ai]
enabled = true
active_questions = ["page-type"]
questions = [{ id = "page-type", version = "1.0.0", name = "Page Type", type = "predicate", instructions = "Is this a product?" }]
[ai.tasks.decision]
model = { provider = "openai", model = "mock", endpoint = "decisions" }
fallback = [{ provider = "anthropic", model = "mock", endpoint = "messages" }]
[ai.budget]
crawl = 10000
[embeddings]
enabled = true
source_prompt = "embedding-prompt"
rules = [{ mode = "include", regex = "shop" }]
pair_rules = [{ source_regex = "guide", target_regex = "shop" }]
"#;
        let config = CrawlConfig::load_str(fixture).unwrap().config;
        let encoded = config.to_osconfig_string().unwrap();
        assert_eq!(CrawlConfig::load_str(&encoded).unwrap().config, config);
        assert_eq!(config.ai.tasks["decision"].fallback[0].endpoint, "messages");
        assert_eq!(config.ai.budget.crawl, MicroUsd(10000));
        assert!(config.embeddings.only_indexable);
        assert_eq!(config.embeddings.similarity_threshold, 0.95);
        assert_eq!(config.embeddings.low_relevance_threshold, 0.4);
        let value = serde_json::to_value(&config.embeddings).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 10);
        assert!(value.get("model").is_none());
        assert!(AiAppSettings::default().redact_personal_data);
    }
    // spec: AI-001
    // Plan M4 E1 (E-2 b): Schalter „KI (Vorschau)“ ist Standard aus und bleibt beim Speichern erhalten.
    #[test]
    fn preview_switch_defaults_off_and_roundtrips_in_app_settings() {
        use crate::settings::AppSettings;
        let defaults = AppSettings::default();
        assert!(!defaults.ai_app.preview_enabled);
        let legacy = AppSettings::from_toml_str("[general]\nlanguage = \"en\"\n").unwrap();
        assert!(!legacy.ai_app.preview_enabled);
        let mut on = AppSettings::default();
        on.ai_app.preview_enabled = true;
        let text = on.to_toml_string().unwrap();
        assert!(text.contains("preview_enabled = true"));
        assert!(
            AppSettings::from_toml_str(&text)
                .unwrap()
                .ai_app
                .preview_enabled
        );
    }
    // spec: AI-013
    #[test]
    fn unlimited_is_explicit_and_survives_a_profile_roundtrip() {
        let mut cfg = CrawlConfig::default();
        cfg.ai.budget.unlimited = true;
        cfg.ai.budget.crawl = MicroUsd(0);
        let text = cfg.to_osconfig_string().unwrap();
        let restored = CrawlConfig::load_str(&text).unwrap().config;
        assert_eq!(restored.ai.budget, cfg.ai.budget);
        assert!(!CrawlConfig::default().ai.budget.unlimited);
    }
    // spec: AI-001
    #[test]
    fn legacy_profile_fixture_gains_disabled_ai_without_changing_existing_values() {
        let fixture = "schema_version = 2\n[speed]\nthreads = 7\n";
        let old = CrawlConfig::load_str(fixture).unwrap().config;
        assert!(!old.ai.enabled);
        assert!(!old.embeddings.enabled);
        assert_eq!(old.speed.threads, 7);
        let upgraded = old.to_osconfig_string().unwrap();
        assert!(upgraded.contains("[ai]"));
        assert!(upgraded.contains("[embeddings]"));
        assert_eq!(CrawlConfig::load_str(&upgraded).unwrap().config, old);
    }
}
