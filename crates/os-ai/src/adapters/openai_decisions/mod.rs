//! Adapter OpenAI Decisions API (AI-004, öffentliche Beta, Modell `gpt-6-luna`): `POST <basis>/decisions` mit
//! `model`, `input` (Text oder Nutzer-Nachricht mit Teilen `input_text`/`input_image`, Bilder nur als
//! Base64-Data-URL) und `questions` (höchstens 10 je Request, OS-Festlegung bis zur GA). Antworten je Frage über
//! `name`; Typ `refusal` wirkt nur auf seine Frage, eine fehlende Frage ergibt `Error: Missing answer`, ein unbekannter
//! `type` `Error: Invalid output` für diese Frage, unbekannte Zusatzfelder werden ignoriert. `<Q> Confidence` ist die
//! höchste Einzelwahrscheinlichkeit; das gelieferte `confidence` bleibt roh erhalten, eine Abweichung über 0,01 wird
//! als `confidence_mismatch` vermerkt. Ohne `usage` bleiben die Tokenfelder `null` und die Kosten kommen aus der
//! Schätzung (`Price Source = estimate`). Ausgabe-Tokens kosten nichts ([`decisions_billing_rates`]).
//!
//! Einstieg für die Decision-Engine (M4-D01): [`OpenAiDecisions::decide`] bzw.
//! [`OpenAiDecisions::decide_with_images`]. Konto, Kontingent und Rate-Limit-Auswertung teilt der Adapter mit
//! [`crate::adapters::openai_responses`].
pub mod answer;
pub mod question;

use std::collections::HashSet;
use std::sync::Arc;

use os_model::ai::AiStatus;
use serde_json::{json, Value};

use crate::adapters::openai_responses::image::{
    from_base64, prepare_image, ImageError, ImageSource, PreparedImage,
};
use crate::adapters::openai_responses::{
    list_models, parse_usage, CapabilityLookup, OpenAiHttp, RequestMeta, PROVIDER_ID,
};
use crate::cost::{tokens, Rates, RequestOutcome};
use crate::limits::{AttemptFailure, Limiter, RequestSpec};
use crate::provider::{
    AiProvider, Capabilities, Input, Model, ProviderError, ProviderFuture, Request, Response, Usage,
};

pub use answer::{
    evaluate, level_index, match_answers, ChoiceProbability, DecisionAnswer, LevelProbability,
    QuestionResult, CONFIDENCE_TOLERANCE,
};
pub use question::{
    bundle, ChoiceOption, MaxQuestionsPerRequest, Question, QuestionKind, ScoreLevel,
};

/// Endpunktname im Modellkatalog und Protokoll.
pub const ENDPOINT: &str = "decisions";
/// Standardmodell der Beta (AI-004).
pub const DEFAULT_MODEL: &str = "gpt-6-luna";

/// Fähigkeiten laut AI-001: Decisions (Text und Bild).
pub const DECISIONS_CAPABILITIES: Capabilities = Capabilities {
    text: false,
    structured_output: false,
    vision: false,
    embeddings: false,
    decisions: true,
    batch: false,
    prices_via_api: false,
    local: false,
};

/// Teil der Eingabe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputPart {
    Text(String),
    Image(PreparedImage),
}

/// Eingabe eines Requests (für alle Fragen eines Requests gleich).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DecisionInput {
    parts: Vec<InputPart>,
}

impl DecisionInput {
    /// Nur Text.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            parts: vec![InputPart::Text(text.into())],
        }
    }
    pub fn from_parts(parts: Vec<InputPart>) -> Self {
        Self { parts }
    }
    /// Text plus Bilder; Bilder, die keine Data-URL sind, werden geladen und kodiert. Ein Bild über 20 MB ergibt
    /// [`ImageError::TooLarge`] (`Skipped (image too large)`).
    pub async fn with_images(
        client: &reqwest::Client,
        text: Option<String>,
        images: Vec<ImageSource>,
    ) -> Result<Self, ImageError> {
        let mut parts: Vec<InputPart> = text.into_iter().map(InputPart::Text).collect();
        for source in images {
            parts.push(InputPart::Image(prepare_image(client, source).await?));
        }
        Ok(Self { parts })
    }
    pub fn parts(&self) -> &[InputPart] {
        &self.parts
    }
    pub fn image_count(&self) -> u32 {
        let n = self
            .parts
            .iter()
            .filter(|p| matches!(p, InputPart::Image(_)))
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }
    /// Zeichen aller Textteile.
    pub fn text_chars(&self) -> u64 {
        self.parts
            .iter()
            .map(|p| match p {
                InputPart::Text(t) => tokens::char_count(t),
                InputPart::Image(_) => 0,
            })
            .sum()
    }
    /// Feld `input`: reiner Text als Zeichenkette, sonst eine Nutzer-Nachricht mit Teilen.
    pub fn to_json(&self) -> Value {
        if let [InputPart::Text(text)] = self.parts.as_slice() {
            return json!(text);
        }
        let content: Vec<Value> = self
            .parts
            .iter()
            .map(|p| match p {
                InputPart::Text(text) => json!({"type": "input_text", "text": text}),
                InputPart::Image(image) => {
                    json!({"type": "input_image", "image_url": image.data_url()})
                }
            })
            .collect();
        json!([{"role": "user", "content": content}])
    }
}

/// Request-Body für `POST /decisions`.
pub fn decisions_body(model: &str, input: &DecisionInput, questions: &[Question]) -> Value {
    json!({
        "model": model,
        "input": input.to_json(),
        "questions": questions,
    })
}

/// Geschätzte Eingabe-Tokens eines Requests je Frage (AI-012, Nachtrag 24.11.2026, Q00-M1): Σ über die Fragen
/// [⌈Z_E / 3,5⌉ + b × B + ⌈Z_q / 3,5⌉ + G], G aus `provider-limits.json` (M4-K10, freigegeben 26.11.2026). Gilt für
/// TPM-Begrenzung (AI-017) und Kosten ohne `usage` (`Price Source = estimate`).
pub fn estimate_input_tokens(
    input: &DecisionInput,
    questions: &[Question],
    image_token_estimate: u64,
) -> u64 {
    let question_chars: Vec<u64> = questions.iter().map(Question::text_chars).collect();
    crate::cost::decisions_input_tokens(
        input.text_chars(),
        u64::from(input.image_count()),
        image_token_estimate,
        &question_chars,
        crate::cost::decisions::bundled_question_overhead(),
    )
}

/// Abrechnungssätze der Decisions API (AI-010, Nachtrag 25.11.2026 „Preise der Decisions API“; Korrektur M4-K10,
/// Befund B1-1, freigegeben 26.11.2026): Ausgabe-Tokens kosten nichts; Eingabe, Cache-Lesen, Cache-Schreiben,
/// Langkontext-Schwelle und -Faktoren, Gebühren und Regionalzuschlag gelten wie in der Preistabelle (der Eintrag
/// `decisions` trägt die Cache-Preise und Langkontext-Faktoren des `responses`-Eintrags, kein Batch).
pub fn decisions_billing_rates(rates: &Rates) -> Rates {
    Rates {
        output_pico: 0,
        ..rates.clone()
    }
}

/// Optionen eines Laufs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecideOptions {
    /// Modell (Standard `gpt-6-luna`).
    pub model: String,
    /// Höchstzahl Fragen je Request (1–10, Standard 10).
    pub max_questions: MaxQuestionsPerRequest,
    /// Bild-Schätzwert der Preistabelle (`Rates::image_token_estimate`) für die Token-Schätzung.
    pub image_token_estimate: u64,
}

impl Default for DecideOptions {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            max_questions: MaxQuestionsPerRequest::default(),
            image_token_estimate: 0,
        }
    }
}

/// Protokolldaten eines Requests (eine Zeile `ai_requests`).
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionsRequestRecord {
    /// Namen der Fragen dieses Requests in Reihenfolge.
    pub questions: Vec<String>,
    /// Status der Protokollzeile: `Success` bei lesbarer Antwort (Ergebnisse je Frage stehen in
    /// [`DecisionsRun::results`]), `Error: Invalid output` bei unlesbarer Antwort, sonst der Endstatus des Requests.
    pub status: AiStatus,
    pub meta: RequestMeta,
    /// Irgendeine Antwort dieses Requests mit `confidence`-Abweichung über 0,01.
    pub confidence_mismatch: bool,
    /// Schätzung vor dem Senden (TPM, Kosten ohne `usage`).
    pub estimated_input_tokens: u64,
    pub images: u32,
}

impl DecisionsRequestRecord {
    /// Eingabe für [`crate::cost::request_record`] mit [`decisions_billing_rates`]; ohne `usage` mit der Schätzung
    /// (`Price Source = estimate`), Ausgabe-Schätzung 0.
    pub fn request_outcome(&self) -> RequestOutcome {
        let mut outcome = self.meta.request_outcome(
            self.status.clone(),
            self.estimated_input_tokens,
            0,
            self.images,
        );
        outcome.confidence_mismatch = self.confidence_mismatch;
        outcome
    }
}

/// Ergebnis eines Laufs über alle Fragen einer Eingabe.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DecisionsRun {
    /// Ergebnis je Frage in der Reihenfolge der übergebenen Fragen.
    pub results: Vec<(String, QuestionResult)>,
    /// Gesendete Requests in Reihenfolge.
    pub requests: Vec<DecisionsRequestRecord>,
}

impl DecisionsRun {
    /// Ergebnis der ersten Frage mit diesem Namen.
    pub fn result(&self, name: &str) -> Option<&QuestionResult> {
        self.results.iter().find(|(n, _)| n == name).map(|(_, r)| r)
    }
}

/// Antwort eines Requests.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionsReply {
    /// Ergebnis je Frage des Requests.
    pub results: Vec<QuestionResult>,
    pub usage: Option<Usage>,
    pub provider_request_id: Option<String>,
    pub http_status: u16,
    /// `false`, wenn die Antwort kein JSON mit einer Liste `answers` war (alle Fragen `Error: Invalid output`).
    pub readable: bool,
}

/// Liest eine Antwort von `POST /decisions` für die Fragen `questions`.
pub fn parse_decisions(
    body: &[u8],
    questions: &[Question],
    request_id: Option<String>,
    http_status: u16,
) -> DecisionsReply {
    let parsed: Option<Value> = serde_json::from_slice(body).ok();
    let answers = parsed
        .as_ref()
        .and_then(|v| v.get("answers"))
        .and_then(Value::as_array);
    let (results, readable) = match answers {
        Some(answers) => (match_answers(questions, answers), true),
        None => (
            questions
                .iter()
                .map(|_| QuestionResult::InvalidOutput {
                    detail: "response has no answers list".into(),
                })
                .collect(),
            false,
        ),
    };
    DecisionsReply {
        results,
        usage: parsed.as_ref().and_then(parse_usage),
        provider_request_id: request_id.or_else(|| {
            parsed
                .as_ref()
                .and_then(|v| v["id"].as_str().map(str::to_string))
        }),
        http_status,
        readable,
    }
}

/// Adapter OpenAI Decisions.
#[derive(Clone)]
pub struct OpenAiDecisions {
    http: OpenAiHttp,
    capabilities: CapabilityLookup,
}

impl std::fmt::Debug for OpenAiDecisions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiDecisions")
            .field("http", &self.http)
            .finish_non_exhaustive()
    }
}

impl OpenAiDecisions {
    /// Ohne Katalog meldet [`AiProvider::models`] jedes Modell ohne Fähigkeiten (konservativ); M4-K04 setzt den
    /// Katalog mit [`OpenAiDecisions::with_capabilities`].
    pub fn new(http: OpenAiHttp) -> Self {
        Self {
            http,
            capabilities: Arc::new(|_| None),
        }
    }
    pub fn with_capabilities(mut self, lookup: CapabilityLookup) -> Self {
        self.capabilities = lookup;
        self
    }
    pub fn http(&self) -> &OpenAiHttp {
        &self.http
    }

    /// Ein Request ohne Wiederholung mit höchstens [`MaxQuestionsPerRequest::LIMIT`] Fragen.
    pub async fn send(
        &self,
        model: &str,
        input: &DecisionInput,
        questions: &[Question],
        observer: Option<&Limiter>,
    ) -> Result<DecisionsReply, AttemptFailure> {
        debug_assert!(questions.len() <= usize::from(MaxQuestionsPerRequest::LIMIT));
        let body = decisions_body(model, input, questions);
        let reply = self.http.post_json(ENDPOINT, &body, observer).await?;
        Ok(parse_decisions(
            &reply.body,
            questions,
            reply.request_id,
            reply.status,
        ))
    }

    /// Stellt alle Fragen zu einer Eingabe: bündelt sie zu Requests mit höchstens `options.max_questions` Fragen
    /// (jede Frage genau einmal) und führt jeden Request über `limiter` aus (Wiederholungen, Timeout und Rate-Limits
    /// gelten je Request, also für alle seine Fragen). Ein doppelter Fragenname wird nicht gesendet und erhält
    /// `Error: Invalid output`.
    pub async fn decide(
        &self,
        limiter: &Limiter,
        input: &DecisionInput,
        questions: &[Question],
        options: &DecideOptions,
    ) -> DecisionsRun {
        let mut results: Vec<Option<QuestionResult>> = vec![None; questions.len()];
        let mut seen = HashSet::new();
        let mut unique = Vec::new();
        for (i, q) in questions.iter().enumerate() {
            if seen.insert(q.name.as_str()) {
                unique.push(i);
            } else {
                results[i] = Some(QuestionResult::InvalidOutput {
                    detail: format!("duplicate question name {:?}", q.name),
                });
            }
        }
        let mut requests = Vec::new();
        for indices in unique.chunks(options.max_questions.get()) {
            let chunk: Vec<Question> = indices.iter().map(|&i| questions[i].clone()).collect();
            let estimated = estimate_input_tokens(input, &chunk, options.image_token_estimate);
            let spec = RequestSpec::new(PROVIDER_ID, options.model.clone(), estimated);
            let exec = limiter
                .run(&spec, |_| {
                    self.send(&options.model, input, &chunk, Some(limiter))
                })
                .await;
            let meta = RequestMeta::from_execution(
                &exec,
                exec.result.as_ref().ok().map(|r| {
                    (
                        r.http_status,
                        r.usage.clone(),
                        r.provider_request_id.clone(),
                    )
                }),
            );
            let (chunk_results, status) = match exec.result {
                Ok(reply) => {
                    let status = if reply.readable {
                        AiStatus::Success
                    } else {
                        AiStatus::InvalidOutput
                    };
                    (reply.results, status)
                }
                Err(error) => (
                    chunk
                        .iter()
                        .map(|_| QuestionResult::NotAnswered {
                            status: error.status.clone(),
                        })
                        .collect(),
                    error.status,
                ),
            };
            let confidence_mismatch = chunk_results
                .iter()
                .filter_map(QuestionResult::answer)
                .any(DecisionAnswer::confidence_mismatch);
            for (&i, result) in indices.iter().zip(chunk_results) {
                results[i] = Some(result);
            }
            requests.push(DecisionsRequestRecord {
                questions: chunk.iter().map(|q| q.name.clone()).collect(),
                status,
                meta,
                confidence_mismatch,
                estimated_input_tokens: estimated,
                images: input.image_count(),
            });
        }
        DecisionsRun {
            results: questions
                .iter()
                .zip(results)
                .map(|(q, r)| (q.name.clone(), r.unwrap_or(QuestionResult::MissingAnswer)))
                .collect(),
            requests,
        }
    }

    /// Wie [`OpenAiDecisions::decide`], bereitet aber vorher die Bilder auf. Ist ein Bild zu groß bzw. nicht nutzbar,
    /// geht kein Request hinaus und alle Fragen erhalten den Status aus [`ImageError::status`] (z. B.
    /// `Skipped (image too large)`).
    pub async fn decide_with_images(
        &self,
        limiter: &Limiter,
        text: Option<String>,
        images: Vec<ImageSource>,
        questions: &[Question],
        options: &DecideOptions,
    ) -> DecisionsRun {
        match DecisionInput::with_images(self.http.client(), text, images).await {
            Ok(input) => self.decide(limiter, &input, questions, options).await,
            Err(error) => {
                let status = error.status();
                DecisionsRun {
                    results: questions
                        .iter()
                        .map(|q| {
                            (
                                q.name.clone(),
                                QuestionResult::NotAnswered {
                                    status: status.clone(),
                                },
                            )
                        })
                        .collect(),
                    requests: Vec::new(),
                }
            }
        }
    }

    /// Modellliste `GET <basis>/models`; Fähigkeiten aus dem Katalog.
    pub async fn list_models(&self) -> Result<Vec<Model>, AttemptFailure> {
        list_models(&self.http, ENDPOINT, &self.capabilities).await
    }
}

/// Summe der `usage` mehrerer Requests; fehlt sie bei einem, ist die Summe unbekannt (`None`).
fn combine_usage(parts: Vec<Option<Usage>>) -> Option<Usage> {
    let add = |x: Option<u64>, y: Option<u64>| Some(x? + y?);
    let mut parts = parts.into_iter();
    let first = parts.next()??;
    parts.try_fold(first, |a, b| {
        let b = b?;
        Some(Usage {
            input_tokens: add(a.input_tokens, b.input_tokens),
            output_tokens: add(a.output_tokens, b.output_tokens),
            cached_input_tokens: add(a.cached_input_tokens, b.cached_input_tokens),
            cache_write_tokens: add(a.cache_write_tokens, b.cache_write_tokens),
            reasoning_tokens: add(a.reasoning_tokens, b.reasoning_tokens),
        })
    })
}

impl AiProvider for OpenAiDecisions {
    fn provider_id(&self) -> &str {
        PROVIDER_ID
    }

    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async move { self.list_models().await.map_err(|f| f.error) })
    }

    /// Ein Versuch je Request (ohne Limiter): `request.questions` sind Fragen im Format von [`Question`], gebündelt zu
    /// höchstens 10 je Request. `content` ist `{"answers": [{"name", "status", "answer" | "refusal_reason" |
    /// "detail"}]}`; `refusal` ist gesetzt, wenn alle Fragen verweigert wurden. Eine nicht lesbare Fragendefinition
    /// ergibt `Error: Invalid output` ohne Request.
    fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
        Box::pin(async move {
            let started = std::time::Instant::now();
            let questions: Vec<Question> = request
                .questions
                .iter()
                .map(|q| serde_json::from_value(q.clone()))
                .collect::<Result<_, _>>()
                .map_err(|_| ProviderError::InvalidOutput)?;
            let mut parts = Vec::new();
            let mut image_error = None;
            for part in &request.input {
                match part {
                    Input::Text { text } => parts.push(InputPart::Text(text.clone())),
                    Input::Image { media_type, base64 } => match from_base64(media_type, base64) {
                        Ok(image) => parts.push(InputPart::Image(image)),
                        Err(e) => image_error = Some(e.status()),
                    },
                }
            }
            let mut results = Vec::new();
            let mut usages = Vec::new();
            let mut request_id = None;
            if let Some(status) = image_error {
                results = questions
                    .iter()
                    .map(|_| QuestionResult::NotAnswered {
                        status: status.clone(),
                    })
                    .collect();
            } else {
                let input = DecisionInput::from_parts(parts);
                for chunk in bundle(&questions, MaxQuestionsPerRequest::default()) {
                    let reply = self
                        .send(&request.model.id, &input, chunk, self.http.limiter())
                        .await
                        .map_err(|f| f.error)?;
                    usages.push(reply.usage);
                    request_id = request_id.or(reply.provider_request_id);
                    results.extend(reply.results);
                }
            }
            let refusal = !results.is_empty()
                && results
                    .iter()
                    .all(|r| matches!(r, QuestionResult::Refusal { .. }));
            let answers: Vec<Value> = questions
                .iter()
                .zip(&results)
                .map(|(q, r)| r.to_json(&q.name))
                .collect();
            Ok(Response {
                content: Some(json!({"answers": answers})),
                usage: combine_usage(usages).unwrap_or_default(),
                refusal,
                provider_request_id: request_id,
                duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            })
        })
    }
}
