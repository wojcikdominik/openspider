//! Embedding-Anbieter im OpenAI-Format (AI-008): OpenAI (`POST /v1/embeddings`) und OpenRouter
//! (`POST /api/v1/embeddings`, OpenAI-kompatibel). Request `{"model", "input": [Texte], "encoding_format": "float",
//! "dimensions"?}`, Antwort `data[].embedding` mit `index`, `usage.prompt_tokens`.
use std::time::Instant;

use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde_json::{json, Map, Value};

use super::{vector_from_json, EmbedFuture, EmbeddingBatchReply, EmbeddingProvider};
use crate::adapters::anthropic::shared::{
    elapsed_ms, error_message, http_failure, opt_u64, quota_failure, rate_limited, retry_after,
    wall_millis, QuotaLatch, QuotaStop,
};
use crate::adapters::openai_responses::client::OpenAiHttp;
use crate::adapters::openrouter::{OpenRouterConfig, FINAL_403_SOURCES, IN_FLIGHT_BUDGET};
use crate::limits::{AttemptFailure, FinalError};
use crate::provider::{ProviderError, Usage};
use os_model::ai::AiStatus;

/// Höchstzahl Eingaben je Request bei OpenAI (öffentliche API-Referenz „Create embeddings“: ein Array darf höchstens
/// 2048 Einträge haben).
pub const OPENAI_MAX_INPUTS: usize = 2048;

/// Request-Körper im OpenAI-Format.
pub fn request_body(model: &str, texts: &[String], dimensions: Option<u32>) -> Value {
    let mut body = Map::new();
    body.insert("model".into(), json!(model));
    body.insert("input".into(), json!(texts));
    body.insert("encoding_format".into(), json!("float"));
    if let Some(dimensions) = dimensions {
        body.insert("dimensions".into(), json!(dimensions));
    }
    Value::Object(body)
}

/// Liest eine Antwort im OpenAI-Format: Vektoren nach `index` geordnet, `usage.prompt_tokens` als Eingabe-Tokens.
/// `None` bei fehlender oder unvollständiger `data`-Liste.
pub fn parse_response(body: &Value) -> Option<(Vec<Vec<f64>>, Usage)> {
    let data = body.get("data")?.as_array()?;
    let mut entries: Vec<(u64, Vec<f64>)> = Vec::with_capacity(data.len());
    for (position, entry) in data.iter().enumerate() {
        let index = entry
            .get("index")
            .and_then(Value::as_u64)
            .unwrap_or(position as u64);
        entries.push((index, vector_from_json(entry.get("embedding")?)?));
    }
    entries.sort_by_key(|(index, _)| *index);
    let usage = body.get("usage");
    Some((
        entries.into_iter().map(|(_, v)| v).collect(),
        Usage {
            input_tokens: opt_u64(usage.and_then(|u| u.get("prompt_tokens"))),
            ..Usage::default()
        },
    ))
}

/// Endgültiger Fehler eines nicht gesendeten Eintrags.
fn final_error(error: ProviderError, status: AiStatus, error_code: Option<String>) -> FinalError {
    FinalError {
        message: status.to_string(),
        error,
        status,
        http_status: None,
        error_code,
    }
}

fn request_id(body: &Value, headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| body.get("id").and_then(Value::as_str).map(str::to_string))
}

/// OpenAI-Embeddings über den HTTP-Zugang des Kontos (gemeinsamer Schlüssel, gemeinsame Kontingent-Sperre mit den
/// Adaptern AI-003/AI-004; Basis-URL global bzw. EU nach AI-019).
#[derive(Debug, Clone)]
pub struct OpenAiEmbeddings {
    http: OpenAiHttp,
}

impl OpenAiEmbeddings {
    /// Über den HTTP-Zugang eines OpenAI-Kontos.
    pub fn new(http: OpenAiHttp) -> Self {
        Self { http }
    }

    async fn embed_once(
        &self,
        model: &str,
        texts: &[String],
        dimensions: Option<u32>,
    ) -> Result<EmbeddingBatchReply, AttemptFailure> {
        let started = Instant::now();
        let reply = self
            .http
            .post_json("embeddings", &request_body(model, texts, dimensions), None)
            .await?;
        let body: Value = serde_json::from_slice(&reply.body)
            .map_err(|_| AttemptFailure::from(ProviderError::InvalidOutput))?;
        let (vectors, usage) = parse_response(&body)
            .ok_or_else(|| AttemptFailure::from(ProviderError::InvalidOutput))?;
        Ok(EmbeddingBatchReply {
            vectors,
            usage,
            provider_request_id: reply.request_id,
            http_status: reply.status,
            duration_ms: elapsed_ms(started),
        })
    }
}

impl EmbeddingProvider for OpenAiEmbeddings {
    fn provider_id(&self) -> &str {
        crate::adapters::openai_responses::client::PROVIDER_ID
    }

    fn endpoint_url(&self) -> String {
        self.http.config().endpoint("embeddings")
    }

    fn max_batch_size(&self) -> Option<usize> {
        Some(OPENAI_MAX_INPUTS)
    }

    /// Fehlender Schlüssel bzw. gesperrtes Konto: kein Request (AI-003, AI-009).
    fn blocked(&self) -> Option<FinalError> {
        if self.http.config().api_key.is_empty() {
            return Some(final_error(
                ProviderError::MissingApiKey,
                AiStatus::MissingApiKey,
                None,
            ));
        }
        self.http.account().quota_stop().map(|stop| {
            final_error(
                ProviderError::QuotaExceeded,
                AiStatus::QuotaExceeded,
                stop.error_code,
            )
        })
    }

    fn embed<'a>(
        &'a self,
        model: &'a str,
        texts: &'a [String],
        dimensions: Option<u32>,
    ) -> EmbedFuture<'a> {
        Box::pin(self.embed_once(model, texts, dimensions))
    }
}

/// OpenRouter-Embeddings (nur für Modelle mit Ausgabemodalität `embeddings`, AI-005). Fehlerklassen wie der
/// Chat-Adapter: 402 → `Quota exceeded` und Sperre (außer In-Flight-Budget, das wie 429 wiederholt wird), 403 mit
/// endgültigem `limit_source` → `Quota exceeded`, 429 → Rate-Limit.
#[derive(Debug, Clone)]
pub struct OpenRouterEmbeddings {
    client: reqwest::Client,
    config: OpenRouterConfig,
    latch: QuotaLatch,
}

impl OpenRouterEmbeddings {
    /// Mit eigener Kontingent-Sperre.
    pub fn new(client: reqwest::Client, config: OpenRouterConfig) -> Self {
        Self {
            client,
            config,
            latch: QuotaLatch::default(),
        }
    }

    /// Teilt die Kontingent-Sperre mit dem Chat-Adapter desselben Kontos (`OpenRouterAdapter::quota`).
    pub fn with_quota(mut self, latch: QuotaLatch) -> Self {
        self.latch = latch;
        self
    }

    /// Die Kontingent-Sperre.
    pub fn quota(&self) -> &QuotaLatch {
        &self.latch
    }

    fn url(&self) -> String {
        format!("{}/embeddings", self.config.base_url.trim_end_matches('/'))
    }

    fn classify(&self, status: StatusCode, headers: &HeaderMap, body: &Value) -> AttemptFailure {
        let code = status.as_u16();
        let message = error_message(body, status);
        let limit_source = body
            .pointer("/error/metadata/limit_source")
            .and_then(Value::as_str);
        let retry = retry_after(headers, wall_millis(None));
        if code == 402 && limit_source == Some(IN_FLIGHT_BUDGET) {
            return rate_limited(retry, code).with_error_code(IN_FLIGHT_BUDGET);
        }
        let final_quota = match code {
            402 => true,
            403 => limit_source.is_some_and(|s| FINAL_403_SOURCES.contains(&s)),
            _ => false,
        };
        if final_quota {
            let error_code = limit_source.map(str::to_string);
            self.latch.trip(QuotaStop {
                http_status: code,
                error_code: error_code.clone(),
                message,
            });
            return quota_failure(code, error_code);
        }
        let failure = if code == 429 {
            rate_limited(retry, code)
        } else {
            http_failure(code, message, retry)
        };
        match limit_source.or_else(|| body.pointer("/error/type").and_then(Value::as_str)) {
            Some(kind) => failure.with_error_code(kind),
            None => failure,
        }
    }

    async fn embed_once(
        &self,
        model: &str,
        texts: &[String],
        dimensions: Option<u32>,
    ) -> Result<EmbeddingBatchReply, AttemptFailure> {
        if let Some(blocked) = self.latch.blocked_failure() {
            return Err(blocked);
        }
        let started = Instant::now();
        let response = self
            .client
            .post(self.url())
            .bearer_auth(self.config.api_key.expose())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request_body(model, texts, dimensions).to_string())
            .send()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(self.classify(status, &headers, &body));
        }
        let (vectors, usage) = parse_response(&body)
            .ok_or_else(|| AttemptFailure::from(ProviderError::InvalidOutput))?;
        Ok(EmbeddingBatchReply {
            vectors,
            usage,
            provider_request_id: request_id(&body, &headers),
            http_status: status.as_u16(),
            duration_ms: elapsed_ms(started),
        })
    }
}

impl EmbeddingProvider for OpenRouterEmbeddings {
    fn provider_id(&self) -> &str {
        crate::adapters::openrouter::PROVIDER_ID
    }

    fn endpoint_url(&self) -> String {
        self.url()
    }

    /// Gesperrtes Konto (endgültiger Kontingentfehler): kein Request.
    fn blocked(&self) -> Option<FinalError> {
        self.latch.stop().map(|stop| {
            final_error(
                ProviderError::QuotaExceeded,
                AiStatus::QuotaExceeded,
                stop.error_code,
            )
        })
    }

    fn embed<'a>(
        &'a self,
        model: &'a str,
        texts: &'a [String],
        dimensions: Option<u32>,
    ) -> EmbedFuture<'a> {
        Box::pin(self.embed_once(model, texts, dimensions))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_vectors_follow_the_index_field() {
        let body = json!({
            "data": [
                {"index": 1, "embedding": [0.0, 1.0]},
                {"index": 0, "embedding": [1.0, 0.0]}
            ],
            "usage": {"prompt_tokens": 7}
        });
        let (vectors, usage) = parse_response(&body).unwrap();
        assert_eq!(vectors, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
        assert_eq!(usage.input_tokens, Some(7));
        assert_eq!(usage.output_tokens, None);
        assert!(parse_response(&json!({"data": [{"embedding": "x"}]})).is_none());
    }

    #[test]
    fn request_body_sends_dimensions_only_when_set() {
        let texts = vec!["a".to_string()];
        assert!(request_body("m", &texts, None).get("dimensions").is_none());
        assert_eq!(request_body("m", &texts, Some(256))["dimensions"], 256);
    }
}
