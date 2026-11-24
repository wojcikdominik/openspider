//! Trockenlauf (Plan) und Live-Lauf (Ausführung, Aufzeichnung, Ergebnisdateien) der Probe Q00b. Nur Rohanfragen.

use crate::budget::Ledger;
use crate::cli::{Mode, Options};
use crate::plan::{
    self, planning_rates, worst_case_usd, Endpoints, ModelNeed, Provider, Rates, Step,
};
use crate::record;
use crate::requests::{body, payload_chars};
use crate::secret::{ApiSecret, KeyLookup, KeySource, Scrubber, APP_SERVICE, PROBE_SERVICE};
use serde_json::{json, Map, Value};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// `anthropic-version` wie in Probe 1 bzw. dem Adapter.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Platzhalter für das Modell ohne `types.disabled` im Trockenlauf.
pub const NON_DISABLEABLE_PLACEHOLDER: &str = "<modell-ohne-types.disabled-aus-der-liste>";

/// Eine Zeile des Plans.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanLine {
    pub provider: Provider,
    pub step: &'static str,
    pub source: plan::FormSource,
    pub method: &'static str,
    pub url: String,
    pub model: &'static str,
    pub purpose: &'static str,
    pub cost_cap_usd: f64,
}

fn model_for_plan(step: &Step) -> &'static str {
    match step.model {
        ModelNeed::None => "",
        ModelNeed::Primary => step.provider.primary_model(),
        ModelNeed::NonDisableable => NON_DISABLEABLE_PLACEHOLDER,
    }
}

fn step_cap(step: &Step, model: &str, rates: &Rates) -> f64 {
    let chars = body(step, model).as_ref().map(payload_chars).unwrap_or(0);
    worst_case_usd(step, chars, rates)
}

/// Der Plan für die gewählten Anbieter (ohne Netzwerk, ohne Schlüsselbund).
pub fn plan_lines(options: &Options, endpoints: &Endpoints) -> Vec<PlanLine> {
    let mut out = Vec::new();
    for provider in options.providers() {
        for step in plan::steps(provider) {
            if options.models_only && step.billable() {
                continue;
            }
            let model = model_for_plan(step);
            out.push(PlanLine {
                provider,
                step: step.id,
                source: step.source,
                method: step.method,
                url: format!("{}{}", endpoints.base(provider), step.path),
                model,
                purpose: step.purpose,
                cost_cap_usd: step_cap(step, model, &planning_rates(provider)),
            });
        }
    }
    out
}

/// Gibt den Plan aus (Trockenlauf).
pub fn print_plan(options: &Options, endpoints: &Endpoints, out: &mut dyn Write) -> io::Result<()> {
    writeln!(out, "OpenSpider Live-Probe M4-Q00b – Trockenlauf: kein Netzwerkzugriff, kein Schlüsselbundzugriff")?;
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
        "Obergrenzen: Eingabe ⌈Zeichen/2⌉+64 Tokens, volle Ausgabegrenze ({} Tokens je Schema-Anfrage); Preise vorsichtig {:.0}/{:.0} USD je 1 Mio. Tokens, OpenRouter zusätzlich {:.2} USD Gebühr je Request.",
        plan::SCHEMA_MAX_TOKENS,
        plan::CONSERVATIVE.input_per_token * 1e6,
        plan::CONSERVATIVE.output_per_token * 1e6,
        plan::OPENROUTER_FEE_CEILING
    )?;
    if options.models_only {
        writeln!(out, "--models-only: nur kostenlose Modelllisten.")?;
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
                "  {:>2}. {:<11} {:<4} {:<44} {:<22} ≤ {:.4} USD  {}  [{}]",
                i + 1,
                l.source.label(),
                l.method,
                l.url,
                if l.model.is_empty() { "-" } else { l.model },
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
    /// Nur für Tests: kleinere Anbietergrenze (wird auf höchstens die Konstante begrenzt).
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

/// Modelle eines Anbieters nach der Modellliste.
#[derive(Debug, Default)]
struct Models {
    primary: Option<Result<(String, Rates), String>>,
    non_disableable: Option<Result<String, String>>,
}

impl Models {
    fn get(&self, need: ModelNeed) -> Result<(String, Option<Rates>), String> {
        let missing = || "Modellliste nicht geladen".to_string();
        match need {
            ModelNeed::None => Ok((String::new(), None)),
            ModelNeed::Primary => match &self.primary {
                Some(Ok((id, rates))) => Ok((id.clone(), Some(*rates))),
                Some(Err(e)) => Err(e.clone()),
                None => Err(missing()),
            },
            ModelNeed::NonDisableable => match &self.non_disableable {
                Some(Ok(id)) => Ok((id.clone(), None)),
                Some(Err(e)) => Err(e.clone()),
                None => Err(missing()),
            },
        }
    }
}

/// Lauf über alle gewählten Anbieter.
pub struct Probe<'a> {
    config: &'a RunConfig,
    client: reqwest::Client,
    ledger: Ledger,
    scrubber: Scrubber,
    out: &'a mut (dyn Write + Send),
}

struct ProviderState {
    provider: Provider,
    records: Vec<Value>,
    aborted: Option<String>,
    models: Models,
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
            "OpenSpider Live-Probe M4-Q00b – LIVE ({}), Gesamtgrenze {:.2} USD, je Anbieter {:.2} USD",
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
            "caps": {"provider_usd": self.ledger.provider_cap(), "total_usd": self.ledger.total_cap()},
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
        write_pretty(&self.config.results_dir.join(name), &clean)
    }

    async fn run_provider(&mut self, provider: Provider, keys: &dyn KeySource) -> Value {
        let base = self.config.endpoints.base(provider).to_string();
        let mut state = ProviderState {
            provider,
            records: Vec::new(),
            aborted: None,
            models: Models::default(),
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
        for step in plan::steps(provider) {
            if self.config.options.models_only && step.billable() {
                state.records.push(skipped(step, "--models-only"));
                continue;
            }
            if let Some(reason) = &state.aborted {
                state.records.push(skipped(step, reason));
                continue;
            }
            let Some(key) = key.as_ref() else { continue };
            let (model, rates) = match state.models.get(step.model) {
                Ok((m, r)) => (m, r.unwrap_or_else(|| planning_rates(provider))),
                Err(e) => {
                    state
                        .records
                        .push(skipped(step, &format!("kein Modell: {e}")));
                    continue;
                }
            };
            let worst = step_cap(step, &model, &rates);
            if let Err(reason) = self.ledger.admit(provider, worst) {
                self.say(&format!("[{}] Abbruch: {reason}", provider.name()));
                state.records.push(skipped(step, &reason));
                state.aborted = Some(reason);
                continue;
            }
            let record = self
                .raw_step(&mut state, step, key, &base, &model, &rates, worst)
                .await;
            state.records.push(record);
            if let Some(reason) = self.ledger.reached(provider) {
                self.say(&format!("[{}] Abbruch: {reason}", provider.name()));
                state.aborted = Some(reason);
            }
        }
        let spent = self.ledger.spent(provider);
        json!({
            "probe": "M4-Q00b",
            "provider": provider.name(),
            "base_url": base,
            "generated_at_unix": now_unix(),
            "models_only": self.config.options.models_only,
            "key": key_info,
            "price_basis": match provider {
                Provider::OpenRouter => "Preise aus der OpenRouter-Modellliste (openai/gpt-6-luna); usage.cost zusätzlich aufgezeichnet",
                _ => "vorsichtige Annahme 30 USD (Eingabe) / 150 USD (Ausgabe) je 1 Mio. Tokens – Obergrenze, keine Abrechnung",
            },
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
        let payload = body(step, model);
        let mut builder = match step.method {
            "GET" => self.client.get(&url),
            _ => self.client.post(&url),
        };
        let mut shown_headers = Map::new();
        let auth_name = match provider {
            Provider::Anthropic => {
                builder = builder
                    .header("x-api-key", key.expose())
                    .header("anthropic-version", ANTHROPIC_VERSION);
                shown_headers.insert("anthropic-version".into(), json!(ANTHROPIC_VERSION));
                "x-api-key"
            }
            _ => {
                builder = builder.bearer_auth(key.expose());
                "authorization"
            }
        };
        if let Some(b) = &payload {
            builder = builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(b.to_string());
            shown_headers.insert("content-type".into(), json!("application/json"));
        }
        let started = Instant::now();
        let sent = builder.send().await;
        let mut rec = json!({
            "step": step.id,
            "form_source": step.source,
            "method": step.method,
            "url": url,
            "purpose": step.purpose,
            "model": if model.is_empty() { Value::Null } else { json!(model) },
            "cost_cap_usd": worst,
            "executed": true,
            "request": {
                "headers": shown_headers,
                "auth_header": format!("{auth_name} (nicht aufgezeichnet)"),
                "body": payload.as_ref().map(record::reduce),
            },
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
        if step.billable() {
            rec["observation"] = record::observe(&parsed);
        }
        rec["response_body"] = self.post_process(state, step, ok, &parsed);
        let usage = parsed.get("usage").cloned();
        let (cost, basis) = match usage.as_ref().and_then(record::tokens_from_usage) {
            Some((i, o)) => (plan::cost_usd(rates, i, o), "usage"),
            None if step.billable() => (worst, "worst_case"),
            None => (0.0, "free"),
        };
        rec["usage"] = json!(usage);
        self.ledger.charge(provider, 1, cost);
        let running = self.ledger.spent(provider).usd;
        rec["cost_usd"] = json!(cost);
        rec["cost_basis"] = json!(basis);
        rec["running_cost_usd"] = json!(running);
        let obs = rec.get("observation").cloned().unwrap_or(Value::Null);
        self.say(&format!(
            "[{}] {} {} → {} | Antwort {} | Reasoning-Tokens {} | ~{:.4} USD ({}) | Summe {:.4} USD",
            provider.name(),
            step.method,
            step.id,
            status.map(|s| s.to_string()).unwrap_or_else(|| "Transportfehler".into()),
            obs.get("answer_present").map(Value::to_string).unwrap_or_else(|| "-".into()),
            obs.get("reasoning_tokens").map(Value::to_string).unwrap_or_else(|| "-".into()),
            cost,
            basis,
            running
        ));
        rec
    }

    /// Antwortkörper reduzieren; aus der Modellliste die Modelle und Fakten ableiten.
    fn post_process(
        &self,
        state: &mut ProviderState,
        step: &Step,
        ok: bool,
        body: &Value,
    ) -> Value {
        let provider = state.provider;
        if !step.id.ends_with(".models") {
            return record::reduce(body);
        }
        let primary = provider.primary_model();
        let entries = record::model_entries(body);
        let entry = entries
            .iter()
            .find(|e| e.get("id").and_then(Value::as_str) == Some(primary))
            .cloned();
        state.facts.insert("primary_model".into(), json!(primary));
        state
            .facts
            .insert("primary_listed".into(), json!(ok && entry.is_some()));
        if ok {
            state.models.primary = Some(match (provider, &entry) {
                (_, None) => Err(format!("{primary} steht nicht in der Modellliste")),
                (Provider::OpenRouter, Some(_)) => {
                    match record::router_models(body)
                        .into_iter()
                        .find(|m| m.id == primary)
                    {
                        Some(m) => Ok((m.id, m.rates)),
                        None => Err(format!(
                            "{primary}: Preise unlesbar oder über den Obergrenzen"
                        )),
                    }
                }
                (_, Some(_)) => Ok((primary.to_string(), plan::CONSERVATIVE)),
            });
        }
        match provider {
            Provider::Anthropic => {
                let caps: Vec<Value> = entries
                    .iter()
                    .filter_map(record::thinking_caps)
                    .map(|c| json!({"id": c.id, "thinking": c.thinking, "disabled": c.disabled, "adaptive": c.adaptive, "effort": c.effort, "effort_low": c.effort_low}))
                    .collect();
                state.facts.insert("thinking_caps".into(), json!(caps));
                if ok {
                    state.models.non_disableable = Some(
                        record::non_disableable_anthropic(body)
                            .map(|c| c.id)
                            .ok_or_else(|| {
                                "kein Modell mit thinking ohne types.disabled in der Liste"
                                    .to_string()
                            }),
                    );
                }
            }
            Provider::OpenRouter => {
                if let Some(e) = &entry {
                    state.facts.insert(
                        "primary_reasoning".into(),
                        e.get("reasoning").cloned().unwrap_or(Value::Null),
                    );
                    state.facts.insert(
                        "primary_supported_parameters".into(),
                        e.get("supported_parameters")
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                    state.facts.insert(
                        "primary_pricing".into(),
                        e.get("pricing").cloned().unwrap_or(Value::Null),
                    );
                }
            }
            Provider::OpenAi => {
                if let Some(e) = &entry {
                    state
                        .facts
                        .insert("primary_entry".into(), record::reduce(e));
                }
            }
        }
        record::summarize_models(body, provider != Provider::OpenRouter)
    }
}

fn skipped(step: &Step, reason: &str) -> Value {
    json!({
        "step": step.id,
        "form_source": step.source,
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
                "requests": n,
                "estimated_cost_usd": usd,
                "aborted": totals.get("aborted"),
            }),
        );
    }
    Ok(json!({
        "probe": "M4-Q00b",
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
