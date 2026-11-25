//! Aufzeichnung: Header-Filter, Kürzen von Antwortkörpern, Modelllisten auf Feldnamen plus Beispiele, Modelle aus
//! der Liste, `usage`, Beobachtungen zu Reasoning und Speicherung, Bereinigung (unverändert aus Probe 1 übernommen:
//! `sanitize`, Header-Regeln).

use crate::plan::{Rates, CONSERVATIVE, OPENROUTER_FEE_CEILING};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Höchstlänge aufgezeichneter Zeichenketten.
pub const MAX_TEXT: usize = 400;
/// Höchstzahl aufgezeichneter Listeneinträge.
pub const MAX_ITEMS: usize = 20;
/// Beispiel-Einträge je Modellliste.
pub const MODEL_EXAMPLES: usize = 3;

/// Request-Header, die nie aufgezeichnet werden (nicht einmal mit Namen im Header-Block).
pub fn is_auth_header(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "authorization" || n == "x-api-key" || n == "proxy-authorization" || n == "cookie"
}

/// Antwort-Header, deren Wert aufgezeichnet wird (Rate-Limits, Request-IDs, unkritische Formatangaben); von allen
/// anderen nur der Name.
pub fn header_value_recorded(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("ratelimit")
        || n.contains("request-id")
        || n == "retry-after"
        || n == "content-type"
        || n == "openai-processing-ms"
        || n == "openai-version"
}

/// Marker für Header, deren Wert nicht aufgezeichnet wird.
pub const VALUE_NOT_RECORDED: &str = "(Wert nicht aufgezeichnet)";

/// Antwort-Header als Objekt Name → Wert bzw. Marker.
pub fn response_headers(headers: &reqwest::header::HeaderMap) -> Value {
    let mut map = BTreeMap::new();
    for (name, value) in headers {
        let name = name.as_str().to_ascii_lowercase();
        let shown = if header_value_recorded(&name) {
            value
                .to_str()
                .map(str::to_string)
                .unwrap_or_else(|_| "(nicht lesbar)".into())
        } else {
            VALUE_NOT_RECORDED.into()
        };
        map.entry(name)
            .and_modify(|v: &mut String| {
                v.push_str(", ");
                v.push_str(&shown);
            })
            .or_insert(shown);
    }
    json!(map)
}

fn truncate(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max).collect();
        format!("{head}…[gekürzt, {count} Zeichen]")
    }
}

/// Kürzt lange Zeichenketten und Listen (für aufgezeichnete Anfrage- und Antwortkörper).
pub fn reduce(value: &Value) -> Value {
    reduce_with(value, MAX_TEXT)
}

fn reduce_with(value: &Value, max_text: usize) -> Value {
    match value {
        Value::String(s) => Value::String(truncate(s, max_text)),
        Value::Array(items) => {
            let mut out: Vec<Value> = items
                .iter()
                .take(MAX_ITEMS)
                .map(|v| reduce_with(v, max_text))
                .collect();
            if items.len() > MAX_ITEMS {
                out.push(json!(format!(
                    "…[+{} weitere Einträge]",
                    items.len() - MAX_ITEMS
                )));
            }
            Value::Array(out)
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), reduce_with(v, max_text)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn collect_paths(
    value: &Value,
    prefix: &str,
    depth: usize,
    out: &mut BTreeMap<String, BTreeSet<&'static str>>,
) {
    if let Value::Object(map) = value {
        for (k, v) in map {
            let path = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            out.entry(path.clone()).or_default().insert(type_name(v));
            if depth < 5 {
                collect_paths(v, &path, depth + 1, out);
            }
        }
    }
}

/// Einträge einer Modellliste (`data` bzw. die Liste selbst).
pub fn model_entries(body: &Value) -> Vec<Value> {
    body.get("data")
        .or(Some(body))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Modellliste reduziert: Feldpfade (mit JSON-Typen und Häufigkeit), Felder außerhalb von `data`, wenige
/// Beispiel-Einträge, optional alle IDs.
pub fn summarize_models(body: &Value, include_ids: bool) -> Value {
    let all = model_entries(body);
    let is_custom = |e: &Value| {
        e.get("id")
            .and_then(Value::as_str)
            .is_some_and(is_custom_model)
    };
    let custom = all.iter().filter(|e| is_custom(e)).count();
    let entries: Vec<Value> = all.into_iter().filter(|e| !is_custom(e)).collect();
    let mut paths: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in &entries {
        let mut own = BTreeMap::new();
        collect_paths(entry, "", 0, &mut own);
        for (path, types) in own {
            *counts.entry(path.clone()).or_default() += 1;
            paths.entry(path).or_default().extend(types);
        }
    }
    let field_paths: Map<String, Value> = paths
        .into_iter()
        .map(|(path, types)| {
            let n = counts.get(&path).copied().unwrap_or(0);
            (
                path,
                json!({"types": types.into_iter().collect::<Vec<_>>(), "entries": n}),
            )
        })
        .collect();
    let top_level: Map<String, Value> = body
        .as_object()
        .map(|m| {
            m.iter()
                .filter(|(k, _)| k.as_str() != "data")
                .map(|(k, v)| (k.clone(), reduce(v)))
                .collect()
        })
        .unwrap_or_default();
    let mut out = json!({
        "count": entries.len() + custom,
        "custom_models_omitted": custom,
        "top_level_fields": top_level,
        "entry_field_paths": field_paths,
        "examples": entries.iter().take(MODEL_EXAMPLES).map(|e| reduce_with(e, 160)).collect::<Vec<_>>(),
    });
    if include_ids {
        out["ids"] = json!(entries
            .iter()
            .filter_map(|e| e.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>());
    }
    out
}

/// Preis-Feld von OpenRouter (USD als Zeichenkette oder Zahl); fehlend = 0, unlesbar/negativ = `None`.
fn price(entry: &Value, field: &str) -> Option<f64> {
    match entry.get("pricing").and_then(|p| p.get(field)) {
        None | Some(Value::Null) => Some(0.0),
        Some(Value::String(s)) => s.parse::<f64>().ok().filter(|v| *v >= 0.0),
        Some(Value::Number(n)) => n.as_f64().filter(|v| *v >= 0.0),
        Some(_) => None,
    }
}

fn strings_at<'a>(entry: &'a Value, path: &[&str]) -> Vec<&'a str> {
    let mut node = Some(entry);
    for p in path {
        node = node.and_then(|n| n.get(*p));
    }
    node.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// OpenRouter-Modell mit gelesenen Preisen.
#[derive(Debug, Clone, PartialEq)]
pub struct RouterModel {
    pub id: String,
    pub rates: Rates,
    pub structured: bool,
    pub vision: bool,
}

/// Wählbare OpenRouter-Modelle: Preise lesbar und nicht 0 (kostenlose Varianten werden ausgelassen, weil sie mit
/// `data_collection: deny` meist keinen Endpunkt haben), Tokenpreise höchstens die vorsichtige Annahme, Gebühren
/// höchstens [`OPENROUTER_FEE_CEILING`], Textausgabe.
pub fn router_models(body: &Value) -> Vec<RouterModel> {
    model_entries(body)
        .iter()
        .filter_map(|e| {
            let id = e.get("id")?.as_str()?.to_string();
            let rates = Rates {
                input_per_token: price(e, "prompt")?,
                output_per_token: price(e, "completion")?,
                per_request: price(e, "request")?,
                per_image: price(e, "image")?,
            };
            let outputs = strings_at(e, &["architecture", "output_modalities"]);
            let params = strings_at(e, &["supported_parameters"]);
            let eligible = !id.ends_with(":free")
                && rates.input_per_token + rates.output_per_token > 0.0
                && rates.input_per_token <= CONSERVATIVE.input_per_token
                && rates.output_per_token <= CONSERVATIVE.output_per_token
                && rates.per_request <= OPENROUTER_FEE_CEILING
                && rates.per_image <= OPENROUTER_FEE_CEILING
                && (outputs.is_empty() || outputs.contains(&"text"));
            eligible.then(|| RouterModel {
                structured: params.contains(&"structured_outputs")
                    || params.contains(&"response_format"),
                vision: strings_at(e, &["architecture", "input_modalities"]).contains(&"image"),
                id,
                rates,
            })
        })
        .collect()
}

/// Tokenzahlen aus einem `usage`-Objekt (Anthropic: `input_tokens` + Cache-Felder, `output_tokens`; OpenAI:
/// `input_tokens`/`output_tokens`; OpenRouter: `prompt_tokens`/`completion_tokens`). `None`, wenn keine
/// Eingabe-Tokenzahl lesbar ist.
pub fn tokens_from_usage(usage: &Value) -> Option<(u64, u64)> {
    let n = |k: &str| usage.get(k).and_then(Value::as_u64);
    let input = n("input_tokens").or_else(|| n("prompt_tokens"))?;
    let cache =
        n("cache_read_input_tokens").unwrap_or(0) + n("cache_creation_input_tokens").unwrap_or(0);
    let output = n("output_tokens")
        .or_else(|| n("completion_tokens"))
        .unwrap_or(0);
    Some((input + cache, output))
}

/// Ersatztext für Konto-, Organisations-, Projekt- und Nutzerkennungen.
pub const OMITTED_ACCOUNT: &str = "(ausgelassen: Kontokennung)";
/// Ersatztext für kundeneigene Modelle (Fine-Tunes), deren ID Organisation oder Zweck verrät.
pub const OMITTED_CUSTOM_MODEL: &str = "(ausgelassen: kundeneigenes Modell)";

/// Felder, deren Wert (sofern nicht `null`) eine Konto-, Organisations-, Projekt- oder Nutzerkennung ist.
fn is_account_field(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    [
        "user_id",
        "workspace_id",
        "organization_id",
        "organisation_id",
        "project_id",
        "account_id",
        "org_id",
        "owner_id",
        "customer_id",
        "team_id",
    ]
    .iter()
    .any(|suffix| k == *suffix || k.ends_with(&format!("_{suffix}")))
        || matches!(
            k.as_str(),
            "email" | "user" | "safety_identifier" | "expires_at" | "organization" | "project"
        )
}

/// Felder mit Anbieter-Request- bzw. Antwort-IDs (Header und Körper). Bei `id` nur, wenn der Wert wie eine
/// Anbieter-ID aussieht (Modell-IDs bleiben).
fn is_request_id_field(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k == "request_id"
        || k == "provider_request_id"
        || k.contains("request-id")
        || k.contains("generation-id")
}

const PROVIDER_ID_PREFIXES: [&str; 9] = [
    "msg_",
    "req_",
    "resp_",
    "rs_",
    "gen-",
    "chatcmpl-",
    "dec_",
    "call_",
    "fc_",
];

fn looks_like_provider_id(text: &str) -> bool {
    PROVIDER_ID_PREFIXES.iter().any(|p| text.starts_with(p))
}

/// Formhinweis statt Wert: Präfix bleiben (`req_…`), der Rest entfällt.
pub fn id_hint(text: &str) -> String {
    match PROVIDER_ID_PREFIXES.iter().find(|p| text.starts_with(*p)) {
        Some(p) => format!("{p}…(ausgelassen: Anbieter-ID)"),
        None if text == VALUE_NOT_RECORDED => text.to_string(),
        None => "(ausgelassen: Anbieter-ID)".into(),
    }
}

/// Undurchsichtige Blobs (signierte bzw. verschlüsselte Reasoning-Inhalte): verraten nichts Prüfbares, sind aber an
/// das Konto gebunden.
fn is_opaque_blob(key: &str, parent: &Map<String, Value>) -> bool {
    let k = key.to_ascii_lowercase();
    k == "signature"
        || k == "encrypted_content"
        || (k == "data"
            && parent
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|t| t.contains("encrypted")))
}

/// Kundeneigenes Modell (Fine-Tune) an der ID erkennbar.
pub fn is_custom_model(id: &str) -> bool {
    id.starts_with("ft:") || id.contains(":ft-") || id.contains(":ft:")
}

/// Entfernt Kennungen aus einem aufgezeichneten Dokument (Antwortkörper, Header, Adapterergebnisse): Konto- und
/// Nutzerkennungen, Anbieter-Request-IDs (nur Präfix bleibt), signierte/verschlüsselte Blobs, IDs kundeneigener
/// Modelle. Idempotent.
pub fn sanitize(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let clean = match v {
                        Value::Null => Value::Null,
                        Value::String(s) if s.starts_with("(ausgelassen") => v.clone(),
                        _ if is_account_field(k) && !v.is_object() && !v.is_array() => {
                            json!(OMITTED_ACCOUNT)
                        }
                        Value::String(s) if is_opaque_blob(k, map) => json!(format!(
                            "(ausgelassen: undurchsichtiger Blob, {} Zeichen aufgezeichnet)",
                            s.chars().count()
                        )),
                        Value::String(s) if is_request_id_field(k) => json!(id_hint(s)),
                        Value::String(s) if k == "id" && looks_like_provider_id(s) => {
                            json!(id_hint(s))
                        }
                        Value::String(s) if is_custom_model(s) => json!(OMITTED_CUSTOM_MODEL),
                        _ => sanitize(v),
                    };
                    (k.clone(), clean)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(sanitize).collect()),
        Value::String(s) if is_custom_model(s) => json!(OMITTED_CUSTOM_MODEL),
        other => other.clone(),
    }
}

fn supported_at(entry: &Value, path: &[&str]) -> Option<bool> {
    let mut node = entry.get("capabilities")?;
    for p in path {
        node = node.get(*p)?;
    }
    node.get("supported")?.as_bool()
}

/// Anthropic-Modell aus der Liste mit seinen Thinking-Fähigkeiten (AI-002, Nachtrag 24.11.2026).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingCaps {
    pub id: String,
    pub thinking: Option<bool>,
    pub disabled: Option<bool>,
    pub adaptive: Option<bool>,
    pub effort: Option<bool>,
    pub effort_low: Option<bool>,
}

/// Thinking-Fähigkeiten eines Listeneintrags.
pub fn thinking_caps(entry: &Value) -> Option<ThinkingCaps> {
    Some(ThinkingCaps {
        id: entry.get("id")?.as_str()?.to_string(),
        thinking: supported_at(entry, &["thinking"]),
        disabled: supported_at(entry, &["thinking", "types", "disabled"]),
        adaptive: supported_at(entry, &["thinking", "types", "adaptive"]),
        effort: supported_at(entry, &["effort"]),
        effort_low: supported_at(entry, &["effort", "low"]),
    })
}

/// Anthropic: ein Modell, bei dem Thinking laut Liste **nicht** abschaltbar ist (`thinking.supported = true`,
/// `types.disabled.supported` nicht `true`); erstes in Listenreihenfolge.
pub fn non_disableable_anthropic(body: &Value) -> Option<ThinkingCaps> {
    model_entries(body)
        .iter()
        .filter_map(thinking_caps)
        .find(|c| c.thinking == Some(true) && c.disabled != Some(true))
}

/// Was je Anfrage für V-A6/V-A8 festgehalten wird (aus dem Antwortkörper, anbieterübergreifend).
pub fn observe(body: &Value) -> Value {
    let usage = body.get("usage").cloned().unwrap_or(Value::Null);
    let n = |path: &[&str]| {
        let mut node = Some(&usage);
        for p in path {
            node = node.and_then(|v| v.get(*p));
        }
        node.and_then(Value::as_u64)
    };
    let reasoning_tokens = n(&["output_tokens_details", "thinking_tokens"])
        .or_else(|| n(&["output_tokens_details", "reasoning_tokens"]))
        .or_else(|| n(&["completion_tokens_details", "reasoning_tokens"]));
    let output_tokens = n(&["output_tokens"]).or_else(|| n(&["completion_tokens"]));
    let answer = answer_text(body);
    let answer_is_json = answer
        .as_deref()
        .is_some_and(|t| serde_json::from_str::<Value>(t.trim()).is_ok());
    // Decisions antworten mit `answers[]` statt Text (Auswertung Q00b: vorher als „keine Antwort“ gezählt).
    let decisions_answers = body
        .get("answers")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());
    json!({
        "answer_present": decisions_answers || answer.as_deref().is_some_and(|t| !t.trim().is_empty()),
        "answer_is_json": answer_is_json,
        "reasoning_tokens": reasoning_tokens,
        "output_tokens": output_tokens,
        "stop": body.get("stop_reason").cloned()
            .or_else(|| body.get("status").cloned())
            .or_else(|| body.pointer("/choices/0/finish_reason").cloned()),
        "incomplete_reason": body.pointer("/incomplete_details/reason").cloned(),
        "thinking_blocks": body.get("content").and_then(Value::as_array)
            .map(|c| c.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("thinking")).count()),
        "store": body.get("store").cloned(),
        "prompt_cache_retention": body.get("prompt_cache_retention").cloned(),
        "reasoning_echo": body.get("reasoning").cloned(),
        "error_type": body.pointer("/error/type").cloned(),
        "error_code": body.pointer("/error/code").cloned(),
        "error_message": body.pointer("/error/message").cloned(),
        "decisions_answer_types": body.get("answers").and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.get("type").cloned()).collect::<Vec<_>>()),
    })
}

/// Antworttext: Anthropic `content[type=text]`, OpenAI `output[].content[type=output_text]`, OpenRouter
/// `choices[0].message.content`.
pub fn answer_text(body: &Value) -> Option<String> {
    if let Some(blocks) = body.get("content").and_then(Value::as_array) {
        let t: String = blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect();
        return Some(t);
    }
    if let Some(items) = body.get("output").and_then(Value::as_array) {
        let t: String = items
            .iter()
            .filter_map(|i| i.get("content").and_then(Value::as_array))
            .flatten()
            .filter(|c| c.get("type").and_then(Value::as_str) == Some("output_text"))
            .filter_map(|c| c.get("text").and_then(Value::as_str))
            .collect();
        return Some(t);
    }
    body.pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
}
