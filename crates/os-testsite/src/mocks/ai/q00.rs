//! Mock-Erweiterungen nach der Live-Probe Q00 (08 Konventionen, Nachtrag 24.11.2026): Reasoning-Tokens, Abbruch an der
//! Ausgabegrenze, OpenRouter-Ist-Kosten, Decisions-Zählung je Frage, Abschaltdaten und Request-IDs. Die Antwortformen
//! folgen den bereinigten Ergebnissen der Probe (`docs/plan/M4-acceptance/live/probe-1/probe/results/*.json`).
use axum::{
    body::Body,
    http::header::{CONTENT_LENGTH, CONTENT_TYPE},
    response::Response,
};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// Text des `thinking`-Blocks bzw. von `message.reasoning` (darf nie in Ergebnissen landen, AI-001).
pub const REASONING_TEXT: &str = "MOCK-REASONING-TEXT";
/// Verschlüsselter Reasoning-Inhalt (OpenAI `encrypted_content`, OpenRouter `reasoning.encrypted`).
pub const ENCRYPTED: &str = "MOCK-ENCRYPTED";
/// Signatur des `thinking`-Blocks.
pub const SIGNATURE: &str = "MOCK-SIGNATURE";
/// Grenze, wenn `output-limit` ohne gesendete Grenze erzwungen wird (gleich dem Ollama-Wert `eval_count = 16`).
pub const FORCED_LIMIT: u64 = 16;
/// Kennung des Kontos in der OpenRouter-Fehlerform „unbekanntes Modell“.
pub const MOCK_USER: &str = "mock-user";

/// `n` aus dem Szenario `reasoning=<n>`, sonst 0.
pub fn reasoning_tokens(scenario: &str) -> u64 {
    scenario
        .strip_prefix("reasoning=")
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// Gesendete Ausgabegrenze: `max_tokens` (Anthropic, OpenRouter) bzw. `max_output_tokens` (OpenAI Responses).
pub fn sent_limit(provider: &str, path: &str, input: &Value) -> Option<u64> {
    match provider {
        "anthropic" | "openrouter" => input.get("max_tokens").and_then(Value::as_u64),
        "openai" if path.ends_with("/responses") => {
            input.get("max_output_tokens").and_then(Value::as_u64)
        }
        _ => None,
    }
}

/// Abbruch an der Ausgabegrenze: (Ausgabe-Tokens = Grenze, Reasoning-Tokens).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Abort {
    pub limit: u64,
    pub reasoning: u64,
}

/// Entscheidet über die Abbruchform: Szenario `output-limit` erzwingt sie (Reasoning = Grenze); sonst greift sie, wenn
/// ⌈Zeichen / 4⌉ + n die gesendete Grenze übersteigt.
pub fn abort(scenario: &str, limit: Option<u64>, answer_tokens: u64, n: u64) -> Option<Abort> {
    if scenario == "output-limit" {
        let limit = limit.unwrap_or(FORCED_LIMIT);
        return Some(Abort {
            limit,
            reasoning: limit,
        });
    }
    let limit = limit.filter(|limit| answer_tokens + n > *limit)?;
    Some(Abort {
        limit,
        reasoning: n.min(limit),
    })
}

/// Text auf die Zeichen von `tokens` Tokens gekürzt (4 Zeichen je Token).
pub fn truncate(text: &str, tokens: u64) -> String {
    let chars = usize::try_from(tokens.saturating_mul(4)).unwrap_or(usize::MAX);
    text.chars().take(chars).collect()
}

/// Preis je Token (USD-Zeichenkette) in Piko-USD; nicht lesbar → 0.
fn pico(value: Option<&Value>) -> u128 {
    let text = match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => return 0,
    };
    let (int, frac) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let mut frac: String = frac.chars().take(12).collect();
    while frac.len() < 12 {
        frac.push('0');
    }
    let int: u128 = int.parse().unwrap_or(0);
    let frac: u128 = frac.parse().unwrap_or(0);
    int * 1_000_000_000_000 + frac
}

/// OpenRouter-Ist-Kosten in Piko-USD: Tokenpreise der Modellliste (inkl. des zutreffenden `overrides`-Eintrags mit dem
/// größten `min_prompt_tokens` ≤ Eingabe-Tokens) × gemeldete Tokens + `pricing.request` + `pricing.image` × Bilder.
pub fn openrouter_cost_pico(
    catalog: &Value,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    images: u64,
) -> u128 {
    let entry = catalog["data"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["id"] == model);
    let Some(pricing) = entry.map(|m| &m["pricing"]) else {
        return 0;
    };
    let mut prompt = pico(pricing.get("prompt"));
    let mut completion = pico(pricing.get("completion"));
    let applicable = pricing["overrides"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| {
            o["min_prompt_tokens"]
                .as_u64()
                .is_some_and(|min| input_tokens >= min)
        })
        .max_by_key(|o| o["min_prompt_tokens"].as_u64());
    if let Some(o) = applicable {
        if o.get("prompt").is_some() {
            prompt = pico(o.get("prompt"));
        }
        if o.get("completion").is_some() {
            completion = pico(o.get("completion"));
        }
    }
    prompt * u128::from(input_tokens)
        + completion * u128::from(output_tokens)
        + pico(pricing.get("request"))
        + pico(pricing.get("image")) * u128::from(images)
}

/// Piko-USD als JSON-Zahl in USD.
pub fn usd(pico: u128) -> Value {
    json!(pico as f64 / 1e12)
}

/// `usage.cost`, `usage.cost_details` und `usage.is_byok` nach Szenario (`no-cost`, `cost-mismatch`, `byok`).
pub fn apply_openrouter_cost(usage: &mut Value, scenario: &str, computed_pico: u128) {
    match scenario {
        "no-cost" => {}
        "byok" => {
            usage["cost"] = json!(0);
            usage["cost_details"] = json!({"upstream_inference_cost": usd(computed_pico)});
            usage["is_byok"] = json!(true);
        }
        _ => {
            let reported = if scenario == "cost-mismatch" {
                computed_pico * 2
            } else {
                computed_pico
            };
            usage["cost"] = usd(reported);
            usage["cost_details"] = json!({"upstream_inference_cost": usd(reported)});
            usage["is_byok"] = json!(false);
        }
    }
}

/// Zahl der Bilder in einer Chat-Completions-Nachrichtenliste.
pub fn image_parts(input: &Value) -> u64 {
    fn walk(v: &Value) -> u64 {
        match v {
            Value::Array(a) => a.iter().map(walk).sum(),
            Value::Object(o) => {
                if o.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| matches!(t, "image" | "input_image" | "image_url"))
                {
                    return 1;
                }
                o.values().map(walk).sum()
            }
            _ => 0,
        }
    }
    walk(input.get("messages").unwrap_or(&Value::Null))
}

fn text_chars(v: &Value) -> usize {
    match v {
        Value::String(s) => s.chars().count(),
        Value::Number(n) => n.to_string().chars().count(),
        _ => 0,
    }
}

/// Eingabe-Tokens der Decisions API (Nachtrag Q00-M1): Σ über die Fragen (⌈Z_E / 4⌉ + 85 × B + ⌈Z_q / 4⌉ + 100) mit
/// Z_E = Zeichen aller Textteile von `input`, B = Bilder, Z_q = `name` + `instructions` + je `choices`-Eintrag `value`
/// und `description` + je `levels`-Eintrag `label` und `description` (AI-012, Nachtrag).
pub fn decisions_input_tokens(input: &Value) -> u64 {
    fn walk(v: &Value) -> (usize, u64) {
        match v {
            Value::String(s) => (s.chars().count(), 0),
            Value::Array(a) => a
                .iter()
                .map(walk)
                .fold((0, 0), |(c, i), (d, j)| (c + d, i + j)),
            Value::Object(o) => {
                if o.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| matches!(t, "input_image" | "image"))
                {
                    return (0, 1);
                }
                ["content", "text"]
                    .iter()
                    .filter_map(|k| o.get(*k))
                    .map(walk)
                    .fold((0, 0), |(c, i), (d, j)| (c + d, i + j))
            }
            _ => (0, 0),
        }
    }
    let (input_chars, images) = walk(input.get("input").unwrap_or(&Value::Null));
    let per_input = u64::try_from(input_chars.div_ceil(4)).unwrap_or(u64::MAX) + 85 * images;
    input["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|q| {
            let mut chars = text_chars(&q["name"]) + text_chars(&q["instructions"]);
            for c in q["choices"].as_array().into_iter().flatten() {
                chars += text_chars(&c["value"]) + text_chars(&c["description"]);
            }
            for l in q["levels"].as_array().into_iter().flatten() {
                chars += text_chars(&l["label"]) + text_chars(&l["description"]);
            }
            per_input + u64::try_from(chars.div_ceil(4)).unwrap_or(u64::MAX) + 100
        })
        .sum()
}

/// Kalenderdatum (UTC) `days` Tage nach heute als `YYYY-MM-DD`.
pub fn date_in_days(days: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let day = i64::try_from(now / 86_400).unwrap_or(0) + days;
    // Umrechnung Tage seit 1970-01-01 → Datum (proleptischer gregorianischer Kalender).
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Request-ID-Header je Anbieter (Q00-N2, N6) und die passenden Körperfelder: Anthropic `request-id: req_mock_<n>`,
/// Antwort-`id` `msg_mock_<n>`, bei Fehlern `request_id` im Körper; OpenAI `x-request-id: req_mock_<n>`; OpenRouter
/// `x-generation-id: gen-mock-<n>` und gleiche Antwort-`id`; Ollama keine.
pub async fn finish(provider: &str, seq: usize, response: Response) -> Response {
    let (mut parts, body) = response.into_parts();
    let header = match provider {
        "anthropic" => Some(("request-id", format!("req_mock_{seq}"))),
        "openai" => Some(("x-request-id", format!("req_mock_{seq}"))),
        "openrouter" => Some(("x-generation-id", format!("gen-mock-{seq}"))),
        _ => None,
    };
    if let Some((name, value)) = header {
        if let Ok(value) = value.parse() {
            parts.headers.insert(name, value);
        }
    }
    let is_json = parts
        .headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    if !is_json || !matches!(provider, "anthropic" | "openrouter") {
        return Response::from_parts(parts, body);
    }
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let Ok(mut value) = serde_json::from_slice::<Value>(&bytes) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    let success = parts.status.is_success();
    match provider {
        "anthropic" if success && value["type"] == "message" => {
            value["id"] = json!(format!("msg_mock_{seq}"));
        }
        "anthropic" if !success && value.get("error").is_some() => {
            value["request_id"] = json!(format!("req_mock_{seq}"));
        }
        "openrouter" if success && value["object"] == "chat.completion" => {
            value["id"] = json!(format!("gen-mock-{seq}"));
        }
        _ => return Response::from_parts(parts, Body::from(bytes)),
    }
    parts.headers.remove(CONTENT_LENGTH);
    Response::from_parts(
        parts,
        Body::from(serde_json::to_vec(&value).unwrap_or_default()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-005 [M4]
    #[test]
    fn openrouter_cost_uses_the_largest_applicable_override() {
        let catalog = json!({"data": [{"id": "m", "pricing": {
            "prompt": "0.000001", "completion": "0.000002", "request": "0.001", "image": "0.002",
            "overrides": [{"min_prompt_tokens": 1000, "prompt": "0.000002", "completion": "0.000004"},
                          {"min_prompt_tokens": 5000, "prompt": "0.000003"}]
        }}]});
        let fees = 1_000_000_000 + 2 * 2_000_000_000;
        assert_eq!(
            openrouter_cost_pico(&catalog, "m", 999, 100, 2),
            999 * 1_000_000 + 100 * 2_000_000 + fees
        );
        assert_eq!(
            openrouter_cost_pico(&catalog, "m", 1000, 100, 0),
            1000 * 2_000_000 + 100 * 4_000_000 + 1_000_000_000
        );
        // Fehlende Felder im größeren Eintrag behalten den Grundpreis (completion 0.000002).
        assert_eq!(
            openrouter_cost_pico(&catalog, "m", 5000, 10, 0),
            5000 * 3_000_000 + 10 * 2_000_000 + 1_000_000_000
        );
        assert_eq!(openrouter_cost_pico(&catalog, "unknown", 10, 10, 0), 0);
    }

    // spec: AI-004 [M4]
    #[test]
    fn decisions_tokens_are_counted_per_question() {
        // Prüfwert aus AI-012 (Nachtrag): Eingabe 350 Zeichen, Fragentexte 140 und 210 Zeichen → 464.
        let input = "e".repeat(350);
        let q1 =
            json!({"name": "a".repeat(40), "instructions": "b".repeat(100), "type": "predicate"});
        let q2 = json!({"name": "c".repeat(10), "instructions": "d".repeat(100), "type": "choice",
            "choices": [{"value": "x".repeat(50), "description": "y".repeat(50)}]});
        let body = json!({"input": input, "questions": [q1, q2]});
        assert_eq!(
            decisions_input_tokens(&body),
            (88 + 35 + 100) + (88 + 53 + 100)
        );
        let image = json!({"input": [{"role": "user", "content": [
            {"type": "input_text", "text": "abcd"},
            {"type": "input_image", "image_url": "data:image/png;base64,AAAA"}
        ]}], "questions": [{"name": "q", "type": "predicate"}]});
        assert_eq!(decisions_input_tokens(&image), 1 + 85 + 1 + 100);
    }

    #[test]
    fn dates_and_abort_rules() {
        assert_eq!(date_in_days(0).len(), 10);
        assert!(date_in_days(30) > date_in_days(0));
        assert_eq!(
            abort("output-limit", Some(50), 1, 0),
            Some(Abort {
                limit: 50,
                reasoning: 50
            })
        );
        assert_eq!(abort("ok", Some(50), 10, 40), None);
        assert_eq!(
            abort("reasoning=41", Some(50), 10, 41),
            Some(Abort {
                limit: 50,
                reasoning: 41
            })
        );
        assert_eq!(abort("ok", None, 10_000, 0), None);
        assert_eq!(truncate("abcdefghij", 2), "abcdefgh");
    }
}
