//! Trockenlauf (Plan) und Live-Lauf (Ausführung, Aufzeichnung, Ergebnisdateien).

use crate::budget::Ledger;
use crate::cli::{Mode, Options};
use crate::plan::{
    self, planning_rates, worst_case_usd, Endpoints, ModelNeed, Provider, Rates, Step, Via,
    MODEL_PLACEHOLDER, SPEC_DECISIONS_MODEL, UNKNOWN_MODEL,
};
use crate::record::{self, Choice};
use crate::requests::{adapter_request, payload_size, raw_body};
use crate::secret::{ApiSecret, KeyLookup, KeySource, Scrubber, APP_SERVICE, PROBE_SERVICE};
use os_ai::adapters::anthropic::{AnthropicAdapter, AnthropicConfig};
use os_ai::adapters::openai_decisions::OpenAiDecisions;
use os_ai::adapters::openai_responses::{OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses};
use os_ai::adapters::openrouter::{OpenRouterAdapter, OpenRouterConfig};
use os_ai::provider::{AiProvider, Response};
use serde_json::{json, Map, Value};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Eine Zeile des Plans.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanLine {
    pub provider: Provider,
    pub step: &'static str,
    pub via: Via,
    pub method: &'static str,
    pub url: String,
    pub purpose: &'static str,
    pub cost_cap_usd: f64,
}

fn image_b64() -> String {
    os_ai::adapters::anthropic::base64_encode(&crate::png::test_png())
}

/// Obergrenze eines Schritts für ein Modell (Trockenlauf: Platzhalter und Planungspreise).
fn step_cap(step: &Step, model: &str, image: &str, rates: &Rates) -> f64 {
    let payload = match step.via {
        Via::Raw => raw_body(step, model, image),
        Via::Adapter => {
            adapter_request(step, model, image).and_then(|r| serde_json::to_value(r).ok())
        }
    };
    let (chars, images) = payload
        .as_ref()
        .map(|p| payload_size(p, image))
        .unwrap_or((0, 0));
    worst_case_usd(step, chars, images, rates)
}

/// Der Plan für die gewählten Anbieter (ohne Netzwerk, ohne Schlüsselbund).
pub fn plan_lines(options: &Options, endpoints: &Endpoints) -> Vec<PlanLine> {
    let image = image_b64();
    let mut out = Vec::new();
    for provider in options.providers() {
        for step in plan::steps(provider) {
            if options.models_only && !step.models_only {
                continue;
            }
            let model = match step.model {
                ModelNeed::SpecLuna => SPEC_DECISIONS_MODEL,
                ModelNeed::Unknown => UNKNOWN_MODEL,
                _ => MODEL_PLACEHOLDER,
            };
            out.push(PlanLine {
                provider,
                step: step.id,
                via: step.via,
                method: step.method,
                url: format!("{}{}", endpoints.base(provider), step.path),
                purpose: step.purpose,
                cost_cap_usd: step_cap(step, model, &image, &planning_rates(provider)),
            });
        }
    }
    out
}

/// Gibt den Plan aus (Trockenlauf).
pub fn print_plan(options: &Options, endpoints: &Endpoints, out: &mut dyn Write) -> io::Result<()> {
    writeln!(out, "OpenSpider Live-Probe M4-Q00 – Trockenlauf: kein Netzwerkzugriff, kein Schlüsselbundzugriff")?;
    if let Mode::DryRun { note: Some(note) } = &options.mode {
        writeln!(out, "Hinweis: {note}")?;
    }
    writeln!(
        out,
        "Kostenbremse: je Anbieter höchstens {:.2} USD und feste Höchstzahl an Anfragen; gesamt höchstens {:.2} USD; keine Wiederholungen.",
        plan::PROVIDER_CAP_USD, options.max_total_usd
    )?;
    writeln!(
        out,
        "Obergrenzen: Eingabe ⌈Zeichen/2⌉+64 Tokens (+{} je Bild), volle Ausgabegrenze; Preise Anthropic/OpenAI vorsichtig {:.0}/{:.0} USD je 1 Mio. Tokens (Listen ohne Preise), OpenRouter höchstens dieselben Tokenpreise plus {:.2} USD Gebühr je Request/Bild.",
        plan::IMAGE_TOKENS_CEILING,
        plan::CONSERVATIVE.input_per_token * 1e6,
        plan::CONSERVATIVE.output_per_token * 1e6,
        plan::OPENROUTER_FEE_CEILING
    )?;
    if options.models_only {
        writeln!(
            out,
            "--models-only: nur kostenlose Listen- und Kontostandsabfragen."
        )?;
    }
    let lines = plan_lines(options, endpoints);
    for provider in options.providers() {
        let mine: Vec<&PlanLine> = lines.iter().filter(|l| l.provider == provider).collect();
        writeln!(out)?;
        writeln!(
            out,
            "[{}] Schlüssel (nur im Live-Lauf): Dienst {PROBE_SERVICE}, ersatzweise {APP_SERVICE}; Konto {}; höchstens {} Anfragen",
            provider.name(),
            provider.account(),
            provider.max_requests()
        )?;
        for (i, l) in mine.iter().enumerate() {
            writeln!(
                out,
                "  {:>2}. {:<7} {:<4} {:<52} ≤ {:.4} USD  {}  [{}]",
                i + 1,
                l.via.label(),
                l.method,
                l.url,
                l.cost_cap_usd,
                l.purpose,
                l.step
            )?;
        }
        let sum: f64 = mine.iter().map(|l| l.cost_cap_usd).sum();
        writeln!(
            out,
            "  Summe: {} Anfragen, Obergrenze {:.4} USD",
            mine.len(),
            sum
        )?;
    }
    let total: f64 = lines.iter().map(|l| l.cost_cap_usd).sum();
    writeln!(out)?;
    writeln!(
        out,
        "Gesamt: {} Anfragen, Obergrenze {:.4} USD",
        lines.len(),
        total
    )?;
    Ok(())
}

/// Konfiguration eines Live-Laufs.
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub options: Options,
    pub endpoints: Endpoints,
    pub results_dir: PathBuf,
    /// Nur für Tests: kleinere Anbietergrenze (wird auf höchstens 2 USD begrenzt).
    pub provider_cap_usd: Option<f64>,
    pub timeout: Duration,
}

impl RunConfig {
    pub fn new(options: Options, results_dir: PathBuf) -> Self {
        Self {
            options,
            endpoints: Endpoints::default(),
            results_dir,
            provider_cap_usd: None,
            timeout: Duration::from_secs(60),
        }
    }
}

/// Ergebnis eines Laufs.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub requests: usize,
    pub cost_usd: f64,
    pub value: Value,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

enum Adapters {
    Anthropic(AnthropicAdapter),
    OpenAi {
        responses: OpenAiResponses,
        decisions: OpenAiDecisions,
    },
    OpenRouter(OpenRouterAdapter),
}

impl Adapters {
    fn new(provider: Provider, client: &reqwest::Client, base: &str, key: &ApiSecret) -> Self {
        match provider {
            Provider::Anthropic => Adapters::Anthropic(AnthropicAdapter::new(
                client.clone(),
                AnthropicConfig::new(os_ai::adapters::anthropic::ApiKey::new(key.expose()))
                    .with_base_url(base),
            )),
            Provider::OpenAi => {
                let http = OpenAiHttp::new(
                    client.clone(),
                    OpenAiConfig::new(os_ai::adapters::openai_responses::ApiKey::new(key.expose()))
                        .with_base_url(base),
                    OpenAiAccount::new(),
                );
                Adapters::OpenAi {
                    responses: OpenAiResponses::new(http.clone()),
                    decisions: OpenAiDecisions::new(http),
                }
            }
            Provider::OpenRouter => Adapters::OpenRouter(OpenRouterAdapter::new(
                client.clone(),
                OpenRouterConfig::new(os_ai::adapters::openrouter::ApiKey::new(key.expose()))
                    .with_base_url(base),
            )),
        }
    }

    fn provider_for(&self, step: &Step) -> &dyn AiProvider {
        match self {
            Adapters::Anthropic(a) => a,
            Adapters::OpenAi { decisions, .. } if step.id.contains("decisions") => decisions,
            Adapters::OpenAi { responses, .. } => responses,
            Adapters::OpenRouter(a) => a,
        }
    }
}

/// Modellwahl eines Anbieters (aus der rohen Modellliste).
#[derive(Debug, Default)]
struct Selection {
    text: Option<Result<Choice, String>>,
    structured: Option<Result<Choice, String>>,
    vision: Option<Result<Choice, String>>,
}

impl Selection {
    fn get(&self, need: ModelNeed) -> Result<&Choice, String> {
        let slot = match need {
            ModelNeed::Structured => &self.structured,
            ModelNeed::Vision => &self.vision,
            _ => &self.text,
        };
        match slot {
            Some(Ok(c)) => Ok(c),
            Some(Err(e)) => Err(e.clone()),
            None => Err("Modellliste nicht geladen".into()),
        }
    }

    fn to_json(&self) -> Value {
        let one = |s: &Option<Result<Choice, String>>| match s {
            Some(Ok(c)) => json!({"model": c.id, "rule": c.rule, "rates_usd_per_token": c.rates}),
            Some(Err(e)) => json!({"error": e}),
            None => json!({"error": "Modellliste nicht geladen"}),
        };
        json!({"text": one(&self.text), "structured": one(&self.structured), "vision": one(&self.vision)})
    }
}

/// Lauf über alle gewählten Anbieter.
pub struct Probe<'a> {
    config: &'a RunConfig,
    client: reqwest::Client,
    image: String,
    ledger: Ledger,
    scrubber: Scrubber,
    out: &'a mut (dyn Write + Send),
}

struct ProviderState {
    provider: Provider,
    records: Vec<Value>,
    aborted: Option<String>,
    selection: Selection,
    facts: Map<String, Value>,
}

impl<'a> Probe<'a> {
    pub fn new(config: &'a RunConfig, out: &'a mut (dyn Write + Send)) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .connect_timeout(Duration::from_secs(15))
            .retry(reqwest::retry::never())
            .build()
            .map_err(|e| format!("HTTP-Client nicht erstellbar: {e}"))?;
        let mut ledger = Ledger::new(config.options.max_total_usd);
        if let Some(cap) = config.provider_cap_usd {
            ledger = ledger.with_provider_cap(cap);
        }
        Ok(Self {
            config,
            client,
            image: image_b64(),
            ledger,
            scrubber: Scrubber::default(),
            out,
        })
    }

    fn say(&mut self, line: &str) {
        let clean = self.scrubber.text(line);
        let _ = writeln!(self.out, "{clean}");
    }

    /// Führt den Lauf aus und schreibt die Ergebnisdateien.
    pub async fn run(mut self, keys: &dyn KeySource) -> io::Result<Summary> {
        std::fs::create_dir_all(&self.config.results_dir)?;
        let mut providers = Map::new();
        self.say(&format!(
            "OpenSpider Live-Probe M4-Q00 – LIVE ({}), Gesamtgrenze {:.2} USD, je Anbieter {:.2} USD",
            if self.config.options.models_only { "nur Modelllisten" } else { "voller Plan" },
            self.ledger.total_cap(),
            self.ledger.provider_cap()
        ));
        for provider in self.config.options.providers() {
            let result = self.run_provider(provider, keys).await;
            let spent = self.ledger.spent(provider);
            providers.insert(
                provider.name().into(),
                json!({
                    "key": result.get("key").cloned().unwrap_or(Value::Null),
                    "requests": spent.requests,
                    "estimated_cost_usd": spent.usd,
                    "aborted": result.get("totals").and_then(|t| t.get("aborted")).cloned().unwrap_or(Value::Null),
                }),
            );
            self.write_json(&format!("{}.json", provider.name()), &result)?;
        }
        let last_run = json!({
            "generated_at_unix": now_unix(),
            "models_only": self.config.options.models_only,
            "providers": providers,
            "requests": self.ledger.total_requests(),
            "estimated_cost_usd": self.ledger.total_usd(),
            "caps": {
                "provider_usd": self.ledger.provider_cap(),
                "total_usd": self.ledger.total_cap(),
            },
        });
        let summary = compose_summary(&self.config.results_dir, Some(last_run))?;
        self.write_json("summary.json", &summary)?;
        self.say(&format!(
            "Fertig: {} Anfragen, geschätzt höchstens {:.4} USD; Ergebnisse in {}",
            self.ledger.total_requests(),
            self.ledger.total_usd(),
            self.config.results_dir.display()
        ));
        Ok(Summary {
            requests: self.ledger.total_requests(),
            cost_usd: self.ledger.total_usd(),
            value: self.scrubber.json(&summary),
        })
    }

    fn write_json(&self, name: &str, value: &Value) -> io::Result<()> {
        let clean = self.scrubber.json(&record::sanitize(value));
        let text = serde_json::to_string_pretty(&clean).map_err(io::Error::other)?;
        std::fs::write(self.config.results_dir.join(name), format!("{text}\n"))
    }

    async fn run_provider(&mut self, provider: Provider, keys: &dyn KeySource) -> Value {
        let base = self.config.endpoints.base(provider).to_string();
        let mut state = ProviderState {
            provider,
            records: Vec::new(),
            aborted: None,
            selection: Selection::default(),
            facts: Map::new(),
        };
        let (key, key_info) = match keys.lookup(provider) {
            KeyLookup::Found { secret, service } => {
                self.scrubber.add(&secret);
                (
                    Some(secret),
                    json!({"found": true, "service": service, "account": provider.account()}),
                )
            }
            KeyLookup::Missing { note } => (
                None,
                json!({"found": false, "account": provider.account(), "note": note}),
            ),
        };
        match &key {
            Some(_) => self.say(&format!(
                "[{}] Schlüssel gefunden ({})",
                provider.name(),
                key_info["service"].as_str().unwrap_or("")
            )),
            None => {
                self.say(&format!(
                    "[{}] übersprungen: kein Schlüssel ({})",
                    provider.name(),
                    key_info["note"].as_str().unwrap_or("")
                ));
                state.aborted = Some("kein Schlüssel im Schlüsselbund".into());
            }
        }
        let adapters = key
            .as_ref()
            .map(|k| Adapters::new(provider, &self.client, &base, k));
        for step in plan::steps(provider) {
            if self.config.options.models_only && !step.models_only {
                state.records.push(skipped(step, "--models-only"));
                continue;
            }
            if let Some(reason) = &state.aborted {
                state.records.push(skipped(step, reason));
                continue;
            }
            let (Some(key), Some(adapters)) = (key.as_ref(), adapters.as_ref()) else {
                continue;
            };
            let (model, rates) = match step.model {
                ModelNeed::None => (String::new(), planning_rates(provider)),
                ModelNeed::SpecLuna => (SPEC_DECISIONS_MODEL.to_string(), planning_rates(provider)),
                ModelNeed::Unknown => (UNKNOWN_MODEL.to_string(), planning_rates(provider)),
                need => match state.selection.get(need) {
                    Ok(c) => (c.id.clone(), c.rates),
                    Err(e) => {
                        state
                            .records
                            .push(skipped(step, &format!("kein Modell: {e}")));
                        continue;
                    }
                },
            };
            let worst = step_cap(step, &model, &self.image, &rates);
            if let Err(reason) = self.ledger.admit(provider, worst) {
                self.say(&format!("[{}] Abbruch: {reason}", provider.name()));
                state.records.push(skipped(step, &reason));
                state.aborted = Some(reason);
                continue;
            }
            let record = match step.via {
                Via::Raw => {
                    self.raw_step(&mut state, step, key, &base, &model, &rates, worst)
                        .await
                }
                Via::Adapter => {
                    self.adapter_step(&mut state, step, adapters, &model, &rates, worst)
                        .await
                }
            };
            state.records.push(record);
            if let Some(reason) = self.ledger.reached(provider) {
                self.say(&format!("[{}] Abbruch: {reason}", provider.name()));
                state.aborted = Some(reason);
            }
        }
        let spent = self.ledger.spent(provider);
        json!({
            "probe": "M4-Q00",
            "provider": provider.name(),
            "base_url": base,
            "generated_at_unix": now_unix(),
            "models_only": self.config.options.models_only,
            "key": key_info,
            "price_basis": match provider {
                Provider::OpenRouter => "Preise aus der OpenRouter-Modellliste des gewählten Modells; ohne Modell vorsichtige Obergrenze",
                _ => "vorsichtige Annahme 30 USD (Eingabe) / 150 USD (Ausgabe) je 1 Mio. Tokens – Obergrenze, keine Abrechnung",
            },
            "selection": state.selection.to_json(),
            "facts": state.facts,
            "requests": state.records,
            "totals": {"requests": spent.requests, "estimated_cost_usd": spent.usd, "aborted": state.aborted},
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn raw_step(
        &mut self,
        state: &mut ProviderState,
        step: &Step,
        key: &ApiSecret,
        base: &str,
        model: &str,
        rates: &Rates,
        worst: f64,
    ) -> Value {
        let provider = state.provider;
        let url = format!("{base}{}", step.path);
        let body = raw_body(step, model, &self.image);
        let images = body
            .as_ref()
            .map(|b| payload_size(b, &self.image).1)
            .unwrap_or(0);
        let mut builder = match step.method {
            "GET" => self.client.get(&url),
            _ => self.client.post(&url),
        };
        let mut shown_headers = Map::new();
        let auth_name = match provider {
            Provider::Anthropic => {
                builder = builder.header("x-api-key", key.expose()).header(
                    "anthropic-version",
                    os_ai::adapters::anthropic::ANTHROPIC_VERSION,
                );
                shown_headers.insert(
                    "anthropic-version".into(),
                    json!(os_ai::adapters::anthropic::ANTHROPIC_VERSION),
                );
                "x-api-key"
            }
            _ => {
                builder = builder.bearer_auth(key.expose());
                "authorization"
            }
        };
        if let Some(b) = &body {
            builder = builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(b.to_string());
            shown_headers.insert("content-type".into(), json!("application/json"));
        }
        let started = Instant::now();
        let sent = builder.send().await;
        let mut rec = base_record(step, model, &url, worst);
        rec["request"] = json!({
            "headers": shown_headers,
            "auth_header": format!("{auth_name} (nicht aufgezeichnet)"),
            "body": body.as_ref().map(record::reduce),
        });
        let (status, parsed) = match sent {
            Ok(response) => {
                let status = response.status().as_u16();
                rec["response_headers"] = record::response_headers(response.headers());
                let bytes = response.bytes().await.unwrap_or_default();
                let parsed: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                    Value::String(String::from_utf8_lossy(&bytes).into_owned())
                });
                (Some(status), parsed)
            }
            Err(e) => {
                rec["transport_error"] = json!(e.to_string());
                (None, Value::Null)
            }
        };
        rec["status"] = json!(status);
        rec["duration_ms"] = json!(started.elapsed().as_millis() as u64);
        let ok = status.is_some_and(|s| (200..300).contains(&s));
        rec["response_body"] = self.post_process(state, step, ok, &parsed);
        let usage = parsed.get("usage").cloned();
        let (cost, basis) = match usage.as_ref().and_then(record::tokens_from_usage) {
            Some((i, o)) => (plan::cost_usd(rates, i, o, images), "usage"),
            None if step.billable => (worst, "worst_case"),
            None => (0.0, "free"),
        };
        rec["usage"] = json!(usage);
        self.finish(
            state,
            step,
            &mut rec,
            cost,
            basis,
            status
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Transportfehler".into()),
        )
    }

    fn finish(
        &mut self,
        state: &ProviderState,
        step: &Step,
        rec: &mut Value,
        cost: f64,
        basis: &str,
        outcome: String,
    ) -> Value {
        self.ledger.charge(state.provider, 1, cost);
        let running = self.ledger.spent(state.provider).usd;
        rec["cost_usd"] = json!(cost);
        rec["cost_basis"] = json!(basis);
        rec["running_cost_usd"] = json!(running);
        self.say(&format!(
            "[{}] {:<7} {} {} → {} | ~{:.4} USD ({}) | Summe {:.4} USD",
            state.provider.name(),
            step.via.label(),
            step.method,
            step.id,
            outcome,
            cost,
            basis,
            running
        ));
        rec.clone()
    }

    /// Antwortkörper reduzieren; aus der rohen Modellliste die Modellwahl ableiten.
    fn post_process(
        &self,
        state: &mut ProviderState,
        step: &Step,
        ok: bool,
        body: &Value,
    ) -> Value {
        let provider = state.provider;
        let wanted = self.config.options.wanted_model(provider);
        if step.id.ends_with(".raw.models") {
            if ok {
                match provider {
                    Provider::Anthropic => {
                        let c = record::choose_anthropic(body, wanted);
                        state.selection = Selection {
                            text: Some(c.clone()),
                            structured: Some(c.clone()),
                            vision: Some(c),
                        };
                    }
                    Provider::OpenAi => {
                        let ids: Vec<String> = record::model_entries(body)
                            .iter()
                            .filter_map(|e| e.get("id").and_then(Value::as_str).map(str::to_string))
                            .collect();
                        state.facts.insert(
                            "gpt_6_luna_listed".into(),
                            json!(ids.iter().any(|i| i == SPEC_DECISIONS_MODEL)),
                        );
                        state.facts.insert(
                            "ids_containing_luna".into(),
                            json!(ids
                                .iter()
                                .filter(|i| i.contains("luna"))
                                .collect::<Vec<_>>()),
                        );
                        let c = record::choose_openai(body, wanted);
                        state.selection = Selection {
                            text: Some(c.clone()),
                            structured: Some(c.clone()),
                            vision: Some(c),
                        };
                    }
                    Provider::OpenRouter => {
                        state.selection = Selection {
                            text: Some(record::choose_openrouter(body, wanted, ModelNeed::Text)),
                            structured: Some(record::choose_openrouter(
                                body,
                                wanted,
                                ModelNeed::Structured,
                            )),
                            vision: Some(record::choose_openrouter(
                                body,
                                wanted,
                                ModelNeed::Vision,
                            )),
                        };
                        let chosen: Vec<Value> =
                            [ModelNeed::Text, ModelNeed::Structured, ModelNeed::Vision]
                                .iter()
                                .filter_map(|n| state.selection.get(*n).ok())
                                .filter_map(|c| {
                                    record::model_entries(body).into_iter().find(|e| {
                                        e.get("id").and_then(Value::as_str) == Some(c.id.as_str())
                                    })
                                })
                                .map(|e| record::reduce(&e))
                                .collect();
                        state
                            .facts
                            .insert("selected_model_entries".into(), json!(chosen));
                        state.facts.insert(
                            "eligible_models".into(),
                            json!(record::router_models(body).len()),
                        );
                    }
                }
                state
                    .facts
                    .insert("selection_source".into(), json!(step.id));
            }
            return record::summarize_models(body, provider != Provider::OpenRouter);
        }
        if step.id == "openrouter.raw.key" {
            let mut body = body.clone();
            if let Some(label) = body.get_mut("data").and_then(|d| d.get_mut("label")) {
                *label = json!("(ausgelassen: kann einen maskierten Schlüssel enthalten)");
            }
            return record::reduce(&body);
        }
        record::reduce(body)
    }

    async fn adapter_step(
        &mut self,
        state: &mut ProviderState,
        step: &Step,
        adapters: &Adapters,
        model: &str,
        rates: &Rates,
        worst: f64,
    ) -> Value {
        let mut rec = base_record(
            step,
            model,
            &format!(
                "{}{}",
                self.config.endpoints.base(state.provider),
                step.path
            ),
            worst,
        );
        let started = Instant::now();
        let kind = step.id.rsplit('.').next().unwrap_or("");
        let (result, outcome, cost, basis) = match (adapters, kind) {
            (Adapters::Anthropic(a), "models") => {
                let r = a.list_models().await;
                let outcome = if r.is_ok() { "Ok" } else { "Fehler" };
                let v = match r {
                    Ok(models) => {
                        let wanted = state
                            .selection
                            .get(ModelNeed::Text)
                            .ok()
                            .map(|c| c.id.clone());
                        json!({
                            "ok": true,
                            "count": models.len(),
                            "examples": models.iter().take(record::MODEL_EXAMPLES).map(|m| format!("{m:?}")).collect::<Vec<_>>(),
                            "selected_model_as_parsed": wanted.and_then(|w| models.iter().find(|m| m.id == w)).map(|m| format!("{m:?}")),
                            "models_without_capabilities": models.iter().filter(|m| !m.capabilities_known).count(),
                        })
                    }
                    Err(f) => json!({"ok": false, "failure": format!("{f:?}")}),
                };
                (v, outcome.to_string(), 0.0, "free")
            }
            (Adapters::OpenAi { responses, .. }, "models") => {
                let r = responses.list_models().await;
                let outcome = if r.is_ok() { "Ok" } else { "Fehler" };
                let v = match r {
                    Ok(models) => json!({
                        "ok": true,
                        "count": models.len(),
                        "examples": models.iter().take(record::MODEL_EXAMPLES).collect::<Vec<_>>(),
                        "gpt_6_luna_listed": models.iter().any(|m| m.id == SPEC_DECISIONS_MODEL),
                    }),
                    Err(f) => json!({"ok": false, "failure": format!("{f:?}")}),
                };
                (v, outcome.to_string(), 0.0, "free")
            }
            (Adapters::OpenRouter(a), "models") => {
                let r = a.list_models().await;
                let outcome = if r.is_ok() { "Ok" } else { "Fehler" };
                let v = match r {
                    Ok(models) => {
                        let chosen: Vec<String> =
                            [ModelNeed::Text, ModelNeed::Structured, ModelNeed::Vision]
                                .iter()
                                .filter_map(|n| state.selection.get(*n).ok().map(|c| c.id.clone()))
                                .collect();
                        json!({
                            "ok": true,
                            "count": models.len(),
                            "examples": models.iter().take(record::MODEL_EXAMPLES).map(|m| format!("{m:?}")).collect::<Vec<_>>(),
                            "selected_models_as_parsed": models.iter().filter(|m| chosen.contains(&m.id)).map(|m| format!("{m:?}")).collect::<Vec<_>>(),
                            "prices_unknown": models.iter().filter(|m| !m.prices_known).count(),
                        })
                    }
                    Err(f) => json!({"ok": false, "failure": format!("{f:?}")}),
                };
                (v, outcome.to_string(), 0.0, "free")
            }
            (Adapters::OpenRouter(a), "key") => {
                let r = a.key_balance().await;
                let outcome = if r.is_ok() { "Ok" } else { "Fehler" };
                let v = match r {
                    Ok(b) => {
                        json!({"ok": true, "balance": format!("{b:?}"), "remaining": format!("{:?}", b.remaining())})
                    }
                    Err(f) => json!({"ok": false, "failure": format!("{f:?}")}),
                };
                (v, outcome.to_string(), 0.0, "free")
            }
            _ => match adapter_request(step, model, &self.image) {
                Some(request) => {
                    let images = serde_json::to_value(&request)
                        .map(|v| payload_size(&v, &self.image).1)
                        .unwrap_or(0);
                    let r = adapters.provider_for(step).execute(request).await;
                    match r {
                        Ok(response) => {
                            let (cost, basis) = response_cost(&response, rates, images, worst);
                            let refusal = response.refusal;
                            let v = json!({"ok": true, "response": record::reduce(&serde_json::to_value(&response).unwrap_or(Value::Null))});
                            (
                                v,
                                if refusal {
                                    "Ok (Refusal)".into()
                                } else {
                                    "Ok".into()
                                },
                                cost,
                                basis,
                            )
                        }
                        Err(e) => {
                            let v = json!({"ok": false, "error": serde_json::to_value(&e).unwrap_or(Value::Null), "status_text": e.to_string()});
                            (v, format!("Fehler: {e}"), worst, "worst_case")
                        }
                    }
                }
                None => (
                    json!({"ok": false, "error": "kein Adapteraufruf für diesen Schritt"}),
                    "nicht ausführbar".into(),
                    0.0,
                    "free",
                ),
            },
        };
        rec["duration_ms"] = json!(started.elapsed().as_millis() as u64);
        rec["adapter_result"] = result;
        self.finish(state, step, &mut rec, cost, basis, outcome)
    }
}

fn response_cost(
    response: &Response,
    rates: &Rates,
    images: u32,
    worst: f64,
) -> (f64, &'static str) {
    let u = &response.usage;
    match u.input_tokens {
        Some(input) => {
            // Vorsichtig: Cache-Felder zusätzlich zählen, auch wenn der Adapter sie schon in input_tokens führt.
            let i = input + u.cached_input_tokens.unwrap_or(0) + u.cache_write_tokens.unwrap_or(0);
            (
                plan::cost_usd(rates, i, u.output_tokens.unwrap_or(0), images),
                "usage",
            )
        }
        None => (worst, "worst_case"),
    }
}

fn base_record(step: &Step, model: &str, url: &str, worst: f64) -> Value {
    json!({
        "step": step.id,
        "via": step.via,
        "method": step.method,
        "url": url,
        "purpose": step.purpose,
        "model": if model.is_empty() { Value::Null } else { json!(model) },
        "cost_cap_usd": worst,
        "executed": true,
    })
}

fn skipped(step: &Step, reason: &str) -> Value {
    json!({
        "step": step.id,
        "via": step.via,
        "method": step.method,
        "purpose": step.purpose,
        "executed": false,
        "skip_reason": reason,
    })
}

fn write_pretty(path: &std::path::Path, value: &Value) -> io::Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    std::fs::write(path, format!("{text}\n"))
}

/// `summary.json` aus allen vorhandenen Anbieterdateien in `dir` (auch aus getrennten `--only`-Läufen);
/// `last_run` beschreibt den Lauf, der die Datei zuletzt geschrieben hat.
pub fn compose_summary(dir: &std::path::Path, last_run: Option<Value>) -> io::Result<Value> {
    let mut providers = Map::new();
    let (mut requests, mut cost) = (0u64, 0.0f64);
    for provider in Provider::ALL {
        let path = dir.join(format!("{}.json", provider.name()));
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let doc: Value = serde_json::from_str(&text).map_err(io::Error::other)?;
        let totals = doc.get("totals").cloned().unwrap_or(Value::Null);
        let n = totals.get("requests").and_then(Value::as_u64).unwrap_or(0);
        let usd = totals
            .get("estimated_cost_usd")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        requests += n;
        cost += usd;
        providers.insert(
            provider.name().into(),
            json!({
                "generated_at_unix": doc.get("generated_at_unix"),
                "models_only": doc.get("models_only"),
                "key": doc.get("key"),
                "selection": doc.get("selection").map(|s| {
                    ["text", "structured", "vision"].iter()
                        .map(|k| (k.to_string(), s.get(k).and_then(|c| c.get("model")).cloned().unwrap_or(Value::Null)))
                        .collect::<Map<_, _>>()
                }),
                "requests": n,
                "estimated_cost_usd": usd,
                "aborted": totals.get("aborted"),
            }),
        );
    }
    Ok(json!({
        "probe": "M4-Q00",
        "composed_from": "alle Anbieterdateien in results/ (auch aus getrennten Läufen); Zeitpunkte je Anbieter unter providers.*.generated_at_unix",
        "caps": {
            "provider_usd": plan::PROVIDER_CAP_USD,
            "total_usd": plan::TOTAL_CAP_USD,
            "max_requests": Provider::ALL.iter().map(|p| (p.name().to_string(), json!(p.max_requests()))).collect::<Map<_, _>>(),
        },
        "price_basis": "Anthropic/OpenAI: vorsichtige Annahme 30/150 USD je 1 Mio. Tokens (Obergrenze, keine Abrechnung); OpenRouter: Preise aus der Modellliste",
        "providers": providers,
        "total_requests": requests,
        "total_estimated_cost_usd": cost,
        "last_run": last_run,
    }))
}

/// Ohne Netz und ohne Schlüsselbund: vorhandene Anbieterdateien erneut bereinigen ([`record::sanitize`]) und
/// `summary.json` aus ihnen neu zusammensetzen. Gibt die Zahl der bearbeiteten Anbieterdateien zurück.
pub fn rebuild_results(dir: &std::path::Path) -> io::Result<usize> {
    let mut n = 0;
    for provider in Provider::ALL {
        let path = dir.join(format!("{}.json", provider.name()));
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let doc: Value = serde_json::from_str(&text).map_err(io::Error::other)?;
        write_pretty(&path, &record::sanitize(&doc))?;
        n += 1;
    }
    let summary = compose_summary(dir, None)?;
    write_pretty(&dir.join("summary.json"), &record::sanitize(&summary))?;
    Ok(n)
}
