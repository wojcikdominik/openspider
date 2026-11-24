//! Aufzeichnung: Header-Filter, Kürzen von Antwortkörpern, Modelllisten auf Feldnamen plus Beispiele, Modellwahl
//! aus der Liste, `usage` und Kosten.

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

fn ids(entries: &[Value]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|e| e.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// Gewähltes Modell mit Regel.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub id: String,
    pub rule: String,
    pub rates: Rates,
}

fn supported(entry: &Value, cap: &str) -> Option<bool> {
    entry
        .get("capabilities")?
        .get(cap)?
        .get("supported")?
        .as_bool()
}

/// Anthropic: die Liste liefert keine Preise (AI-002); „billigstes Modell“ ist daher nicht ablesbar. Regel: aus den
/// Modellen, die `structured_outputs` und `image_input` als unterstützt melden (sonst aus allen), das erste in
/// Listenreihenfolge, dessen ID „haiku“ enthält, sonst das erste. Eine Vorgabe (`--model-anthropic`) muss in der
/// Liste stehen.
pub fn choose_anthropic(body: &Value, wanted: Option<&str>) -> Result<Choice, String> {
    let entries = model_entries(body);
    if let Some(w) = wanted {
        return if ids(&entries).iter().any(|i| i == w) {
            Ok(Choice {
                id: w.into(),
                rule: "Vorgabe --model-anthropic (in der Liste gefunden)".into(),
                rates: CONSERVATIVE,
            })
        } else {
            Err(format!("Vorgabe {w} steht nicht in der Modellliste"))
        };
    }
    let capable: Vec<&Value> = entries
        .iter()
        .filter(|e| {
            supported(e, "structured_outputs") == Some(true)
                && supported(e, "image_input") == Some(true)
        })
        .collect();
    let (pool, pool_note) = if capable.is_empty() {
        (
            entries.iter().collect::<Vec<_>>(),
            "kein Modell meldet structured_outputs und image_input; Auswahl aus allen",
        )
    } else {
        (
            capable,
            "Modelle mit structured_outputs.supported und image_input.supported",
        )
    };
    let id_of = |e: &&Value| e.get("id").and_then(Value::as_str).map(str::to_string);
    let pick = pool
        .iter()
        .find(|e| id_of(e).is_some_and(|i| i.to_ascii_lowercase().contains("haiku")))
        .or_else(|| pool.first())
        .and_then(id_of)
        .ok_or_else(|| "Modellliste leer oder ohne id".to_string())?;
    Ok(Choice {
        id: pick,
        rule: format!(
            "{pool_note}; erstes mit „haiku“ in der ID, sonst erstes (Liste ohne Preise, AI-002)"
        ),
        rates: CONSERVATIVE,
    })
}

const OPENAI_EXCLUDE: [&str; 16] = [
    "embedding",
    "tts",
    "transcribe",
    "audio",
    "realtime",
    "image",
    "search",
    "moderation",
    "whisper",
    "dall-e",
    "babbage",
    "davinci",
    "computer-use",
    "codex",
    "deep-research",
    "instruct",
];

/// OpenAI Responses: Liste ohne Preise und Fähigkeiten (AI-001). Regel: Vorgabe `--model-openai` (muss in der Liste
/// stehen); sonst `gpt-6-luna`, falls gelistet (08 nennt es mit Responses-Preisen, AI-010); sonst unter den
/// `gpt-`-IDs ohne Sonderzwecke die kürzeste mit „nano“, dann mit „mini“.
pub fn choose_openai(body: &Value, wanted: Option<&str>) -> Result<Choice, String> {
    let all = ids(&model_entries(body));
    let choice = |id: &str, rule: &str| Choice {
        id: id.into(),
        rule: rule.into(),
        rates: CONSERVATIVE,
    };
    if let Some(w) = wanted {
        return if all.iter().any(|i| i == w) {
            Ok(choice(w, "Vorgabe --model-openai (in der Liste gefunden)"))
        } else {
            Err(format!("Vorgabe {w} steht nicht in der Modellliste"))
        };
    }
    if all.iter().any(|i| i == crate::plan::SPEC_DECISIONS_MODEL) {
        return Ok(choice(
            crate::plan::SPEC_DECISIONS_MODEL,
            "gpt-6-luna gelistet (08 AI-010 nennt es für responses)",
        ));
    }
    for tier in ["nano", "mini"] {
        let mut pool: Vec<&String> = all
            .iter()
            .filter(|i| i.starts_with("gpt-") && i.contains(tier))
            .filter(|i| !OPENAI_EXCLUDE.iter().any(|x| i.contains(x)))
            .collect();
        pool.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
        if let Some(id) = pool.first() {
            return Ok(choice(
                id,
                &format!(
                    "gpt-6-luna nicht gelistet; kürzeste gpt-ID mit „{tier}“ ohne Sonderzweck"
                ),
            ));
        }
    }
    Err(
        "kein passendes Modell gefunden; bitte --model-openai <id> aus results/openai.json angeben"
            .into(),
    )
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

/// Typische Kosten einer Probe-Anfrage (500 Eingabe-, 128 Ausgabe-Tokens) für die Sortierung.
fn typical(m: &RouterModel, images: u32) -> f64 {
    crate::plan::cost_usd(&m.rates, 500, 128, images)
}

/// Bevorzugtes OpenRouter-Modell: dieselbe Spec-ID wie bei OpenAI (AI-004, AI-010), damit Antworten und Preise
/// zwischen direktem Weg und OpenRouter vergleichbar sind.
pub const OPENROUTER_PREFERRED: &str = "openai/gpt-6-luna";

/// OpenRouter: bevorzugt [`OPENROUTER_PREFERRED`], sonst das billigste wählbare Modell je Fähigkeit (Text; strukturierte Ausgabe laut `supported_parameters`;
/// Bild laut `architecture.input_modalities`).
pub fn choose_openrouter(
    body: &Value,
    wanted: Option<&str>,
    need: crate::plan::ModelNeed,
) -> Result<Choice, String> {
    use crate::plan::ModelNeed;
    let models = router_models(body);
    if let Some(w) = wanted {
        return match models.iter().find(|m| m.id == w) {
            Some(m) => Ok(Choice {
                id: m.id.clone(),
                rule: "Vorgabe --model-openrouter (in der Liste, Preise innerhalb der Obergrenzen)"
                    .into(),
                rates: m.rates,
            }),
            None => Err(format!(
                "Vorgabe {w} steht nicht in der Liste oder liegt über den Preisobergrenzen"
            )),
        };
    }
    let (filter, images, label): (fn(&RouterModel) -> bool, u32, &str) = match need {
        ModelNeed::Structured => (
            |m| m.structured,
            0,
            "mit structured_outputs/response_format",
        ),
        ModelNeed::Vision => (|m| m.vision, 1, "mit Bildeingabe"),
        _ => (|_| true, 0, "Text"),
    };
    // Vergleichbarkeit (Auswertung Q00, 24.11.2026): die in 08 genannte ID über OpenRouter, wenn wählbar und fähig;
    // die billigsten Modelle der Liste waren untypisch (Spezialmodelle) und taugen nicht als Vergleich.
    if let Some(m) = models
        .iter()
        .find(|m| m.id == OPENROUTER_PREFERRED && filter(m))
    {
        return Ok(Choice {
            id: m.id.clone(),
            rule: format!(
                "{OPENROUTER_PREFERRED} (Spec-ID aus 08 über OpenRouter, {label}; Preise aus der Liste)"
            ),
            rates: m.rates,
        });
    }
    models
        .iter()
        .filter(|m| filter(m))
        .min_by(|a, b| {
            typical(a, images)
                .total_cmp(&typical(b, images))
                .then(a.id.cmp(&b.id))
        })
        .map(|m| Choice {
            id: m.id.clone(),
            rule: format!(
                "billigstes Modell {label} (Preise aus der Liste; ohne :free und ohne Preis 0)"
            ),
            rates: m.rates,
        })
        .ok_or_else(|| format!("kein wählbares Modell {label}"))
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
