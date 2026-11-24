//! Gemeinsame Hilfen der unabhängigen Abnahmetests M4-K07 (AI-018, AI-024).
//!
//! - Ein Test-Adapter, der den vorbereiteten Request als JSON an den ai-mock schickt (die Produktiv-Adapter sind in
//!   diesem Stand noch nicht angebunden). Was im Request-Log des Mocks steht, hat „die Maschine verlassen“.
//! - Ein kleiner Feld-Extraktor für die eigenen Testseiten und die Fixture-Site (die HTML-Aufbereitung gehört nicht
//!   zu `os-ai`; hier zählt nur, was aus den Feldwerten in den Request gelangt).
#![allow(dead_code)]

use std::path::PathBuf;

use os_ai::policy::{FieldValue, FieldValues, InputField};
use os_ai::provider::{
    AiProvider, Capabilities, CapabilitySource, Model, ProviderError, ProviderFuture, Request,
    Response, Usage,
};
use os_testsite::mocks::{ai::AiMock, MockServer};
use serde_json::{json, Value};

pub fn repo_file(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

pub fn read_fixture(rel: &str) -> String {
    let path = repo_file(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

// ---------------------------------------------------------------------------------------------------------------
// ai-mock

pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::fixtures().expect("ai-mock fixtures");
        let server = MockServer::start(mock.registrations())
            .await
            .expect("ai-mock starts");
        Self { mock, server }
    }

    /// Request-Bodies an Anbieter-Routen (ohne Steueraufrufe).
    pub fn bodies(&self) -> Vec<String> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| !r.path.starts_with("/__mock"))
            .map(|r| r.body)
            .collect()
    }

    pub fn count(&self) -> usize {
        self.bodies().len()
    }

    pub fn count_for(&self, provider: &str) -> usize {
        let prefix = format!("/{provider}/");
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.path.starts_with(&prefix))
            .count()
    }

    /// Volltext des gesamten Request-Logs (alle Felder aller Einträge) für die Volltextsuche.
    pub fn full_log(&self) -> String {
        serde_json::to_string(&self.mock.requests()).expect("log serialises")
    }

    pub fn provider(&self, id: &'static str) -> ForwardingProvider {
        ForwardingProvider::new(id, &self.server)
    }
}

/// Test-Adapter: sendet `instructions` und `input` des Requests unverändert als JSON an den ai-mock.
pub struct ForwardingProvider {
    id: &'static str,
    url: String,
    client: reqwest::Client,
}

impl ForwardingProvider {
    pub fn new(id: &'static str, server: &MockServer) -> Self {
        let url = match id {
            "anthropic" => format!("{}/v1/messages", server.base_url("anthropic")),
            "openrouter" => format!("{}/api/v1/chat/completions", server.base_url("openrouter")),
            "ollama" => format!("{}/api/chat", server.base_url("ollama")),
            _ => format!("{}/v1/responses", server.base_url("openai")),
        };
        Self {
            id,
            url,
            client: reqwest::Client::builder()
                .no_proxy()
                .build()
                .expect("client"),
        }
    }

    pub fn model(&self, model_id: &str) -> Model {
        Model {
            provider: self.id.to_string(),
            id: model_id.to_string(),
            endpoint: self.url.clone(),
            capabilities: Capabilities {
                text: true,
                structured_output: true,
                vision: true,
                ..Capabilities::default()
            },
            capability_source: CapabilitySource::ModelCatalog,
        }
    }

    pub fn request(&self, model_id: &str) -> Request {
        Request {
            model: self.model(model_id),
            required: Capabilities {
                text: true,
                ..Capabilities::default()
            },
            input: Vec::new(),
            instructions: "Abnahme K07".into(),
            questions: Vec::new(),
            response_schema: None,
            max_tokens: None,
            temperature: None,
            effort: None,
        }
    }
}

impl AiProvider for ForwardingProvider {
    fn provider_id(&self) -> &str {
        self.id
    }

    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async move { Ok(vec![self.model("k07-model")]) })
    }

    fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
        Box::pin(async move {
            let body = json!({
                "model": request.model.id,
                "instructions": request.instructions,
                "input": request.input,
            });
            let resp = self
                .client
                .post(&self.url)
                .header("content-type", "application/json")
                .body(body.to_string())
                .send()
                .await
                .map_err(|e| ProviderError::Transport {
                    message: e.to_string(),
                })?;
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            if status >= 400 {
                return Err(ProviderError::Http {
                    status,
                    message: text,
                });
            }
            Ok(Response {
                content: serde_json::from_str::<Value>(&text).ok(),
                usage: Usage::default(),
                refusal: false,
                provider_request_id: None,
                duration_ms: 0,
            })
        })
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Feld-Extraktion aus den Testseiten

fn between<'a>(s: &'a str, start: &str, end: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(start) {
        let after = &rest[i + start.len()..];
        // Attribute des Start-Tags überspringen, falls `start` mit `<tag` endet.
        let after = if start.ends_with('>') {
            after
        } else {
            match after.find('>') {
                Some(j) => &after[j + 1..],
                None => break,
            }
        };
        match after.find(end) {
            Some(j) => {
                out.push(&after[..j]);
                rest = &after[j + end.len()..];
            }
            None => break,
        }
    }
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let i = tag.find(&key)? + key.len();
    let j = tag[i..].find('"')?;
    Some(&tag[i..i + j])
}

/// Feldwerte einer Seite: `url`, `title`, `meta_description`, `h1`, `h2` (alle H2, mit `\n` verbunden),
/// `page_text` (alle `<p>` im Inhalt, mit `\n` verbunden), `alt` und `image` (erstes Bild), `image_context`
/// (`<figcaption>`).
pub fn extract_fields(url: &str, html: &str) -> FieldValues {
    let mut v = FieldValues::new().with_text(InputField::Url, url);
    if let Some(t) = between(html, "<title>", "</title>").first() {
        v = v.with_text(InputField::Title, strip_tags(t));
    }
    if let Some(i) = html.find("<meta name=\"description\"") {
        let tag = &html[i..i + html[i..].find('>').unwrap()];
        if let Some(c) = attr(tag, "content") {
            v = v.with_text(InputField::MetaDescription, c);
        }
    }
    if let Some(t) = between(html, "<h1", "</h1>").first() {
        v = v.with_text(InputField::H1, strip_tags(t));
    }
    let h2: Vec<String> = between(html, "<h2", "</h2>")
        .into_iter()
        .map(strip_tags)
        .collect();
    if !h2.is_empty() {
        v = v.with_text(InputField::H2, h2.join("\n"));
    }
    let main = between(html, "<main>", "</main>")
        .first()
        .copied()
        .unwrap_or(html);
    let paragraphs: Vec<String> = between(main, "<p", "</p>")
        .into_iter()
        .map(strip_tags)
        .collect();
    if !paragraphs.is_empty() {
        v = v.with_text(InputField::PageText, paragraphs.join("\n"));
    }
    if let Some(i) = html.find("<img ") {
        let tag = &html[i..i + html[i..].find('>').unwrap()];
        if let Some(alt) = attr(tag, "alt") {
            v = v.with_text(InputField::Alt, alt);
        }
        if let Some(src) = attr(tag, "src") {
            if let Some(rest) = src.strip_prefix("data:") {
                let (media_type, base64) = rest.split_once(";base64,").expect("base64 data URL");
                v.insert(
                    InputField::Image,
                    FieldValue::Image {
                        media_type: media_type.to_string(),
                        base64: base64.to_string(),
                    },
                );
            }
        }
    }
    if let Some(t) = between(html, "<figcaption", "</figcaption>").first() {
        v = v.with_text(InputField::ImageContext, strip_tags(t));
    }
    v
}

/// Zeitpunkt der Einwilligung in den Tests: 2026-11-24T09:30:00Z.
pub const GIVEN_AT_MS: i64 = 1_795_512_600_000;
