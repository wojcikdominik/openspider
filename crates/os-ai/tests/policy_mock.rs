//! AI-018 und AI-024 gegen ai-mock (Mock-Zähler und Volltextsuche im Request-Log). Die Requests sendet ein
//! schlanker Testadapter über HTTP an den Mock (die Produktivadapter liefern K01–K03); geprüft wird der
//! Datenschutz-Wächter `os_ai::policy::guard`, der vor jedem Adapter liegt. Seiten: `tests/fixtures/ai/site/`
//! (`fields.html` mit Feldmarkern, `pii.html`, Kontakt und Impressum).
#[path = "policy_html.rs"]
mod policy_html;
use os_ai::{
    policy::{
        guard::{self, PreparedInput},
        ConsentBook, ConsentGate, DataFlowOverview, FieldSet, FieldValue, FieldValues, InputField,
        ProviderPolicies, RedactionSettings, Redactor, TaskDataFlow,
    },
    provider::{
        AiProvider, Capabilities, CapabilitySource, Input, Model, Outcome, ProviderError,
        ProviderFuture, Request, Response, Usage,
    },
    AiStatus, PreviewSwitch,
};
use os_testsite::mocks::{
    ai::{AiMock, RequestRecord},
    MockServer,
};
use policy_html::{repo, site_page};
use serde_json::{json, Value};

const T0: i64 = 1_796_083_200_000;

fn fixture(name: &str) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(repo(&format!("tests/fixtures/ai/policy/{name}"))).unwrap(),
    )
    .unwrap()
}

/// Markers of `site/fields.html` per field; `word_count` and `page_type` are derived values (word count, result of
/// another question) and get their marker here.
const MARKERS: [(InputField, &str); 14] = [
    (InputField::Url, "MARK-URL"),
    (InputField::Title, "MARK-TITLE"),
    (InputField::MetaDescription, "MARK-META"),
    (InputField::H1, "MARK-H1"),
    (InputField::H2, "MARK-H2"),
    (InputField::PageText, "MARK-TEXT"),
    (InputField::WordCount, "MARK-WORDS"),
    (InputField::HtmlLang, "MARK-LANG"),
    (InputField::HreflangSelf, "MARK-HREFLANG"),
    (InputField::StructuredDataTypes, "MARK-SD"),
    (InputField::PageType, "MARK-PAGETYPE"),
    (InputField::Image, "TUFSSy1JTUFHRQ"),
    (InputField::Alt, "MARK-ALT"),
    (InputField::ImageContext, "MARK-CONTEXT"),
];

fn fields_page() -> FieldValues {
    let mut page = site_page("fields.html");
    page.insert(
        InputField::Url,
        FieldValue::Text("http://ai.test:8080/fields?MARK-URL".into()),
    );
    page.insert(InputField::WordCount, FieldValue::Text("MARK-WORDS".into()));
    page.insert(
        InputField::PageType,
        FieldValue::Text("MARK-PAGETYPE".into()),
    );
    assert_eq!(
        page.fields(),
        FieldSet::new(InputField::ALL),
        "fields.html carries every field"
    );
    page
}

fn flows() -> Vec<TaskDataFlow> {
    serde_json::from_value(fixture("tasks.json")["flows"].clone()).unwrap()
}

/// Minimal HTTP test adapter: sends the request parts unchanged as JSON to the ai-mock route of its provider.
struct MockHttp {
    id: &'static str,
    url: String,
    client: reqwest::Client,
}

impl MockHttp {
    fn new(server: &MockServer, id: &'static str) -> Self {
        let path = match id {
            "openai" => "/v1/responses",
            "ollama" => "/api/chat",
            "anthropic" => "/v1/messages",
            _ => "/api/v1/chat/completions",
        };
        Self {
            id,
            url: format!("{}{path}", server.base_url(id)),
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
        }
    }
}

impl AiProvider for MockHttp {
    fn provider_id(&self) -> &str {
        self.id
    }
    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
        Box::pin(async move {
            let text: Vec<&str> = request
                .input
                .iter()
                .filter_map(|p| match p {
                    Input::Text { text } => Some(text.as_str()),
                    Input::Image { .. } => None,
                })
                .collect();
            let images: Vec<&str> = request
                .input
                .iter()
                .filter_map(|p| match p {
                    Input::Image { base64, .. } => Some(base64.as_str()),
                    Input::Text { .. } => None,
                })
                .collect();
            let body =
                json!({"model": request.model.id, "input": text.join("\n"), "images": images});
            let resp = self
                .client
                .post(&self.url)
                .json(&body)
                .send()
                .await
                .map_err(|e| ProviderError::Transport {
                    message: e.to_string(),
                })?;
            let status = resp.status().as_u16();
            let content: Value = resp.json().await.unwrap_or(Value::Null);
            if status != 200 {
                return Err(ProviderError::Http {
                    status,
                    message: content.to_string(),
                });
            }
            Ok(Response {
                content: Some(content),
                usage: Usage::default(),
                refusal: false,
                provider_request_id: None,
                duration_ms: 0,
            })
        })
    }
}

fn request(provider: &str) -> Request {
    Request {
        model: Model {
            provider: provider.into(),
            id: "mock-model".into(),
            endpoint: "responses".into(),
            capabilities: Capabilities {
                text: true,
                structured_output: true,
                vision: true,
                ..Capabilities::default()
            },
            capability_source: CapabilitySource::ModelCatalog,
            reasoning_levels: Vec::new(),
        },
        required: Capabilities {
            text: true,
            ..Capabilities::default()
        },
        input: Vec::new(),
        instructions: "Policy test".into(),
        questions: Vec::new(),
        response_schema: None,
        max_tokens: None,
        temperature: None,
        effort: None,
        reasoning: Default::default(),
    }
}

async fn start() -> (AiMock, MockServer) {
    let mock = AiMock::fixtures().unwrap();
    let server = MockServer::start(mock.registrations()).await.unwrap();
    (mock, server)
}

fn provider_requests(mock: &AiMock) -> Vec<RequestRecord> {
    mock.requests()
        .into_iter()
        .filter(|r| !r.path.starts_with("/__mock"))
        .collect()
}

async fn send(
    gate: &ConsentGate,
    provider: &MockHttp,
    task: &str,
    input: &PreparedInput,
) -> guard::GuardedOutcome {
    guard::execute(
        PreviewSwitch::ON,
        gate,
        task,
        input,
        provider,
        request(provider.id),
    )
    .await
}

// spec: AI-018
#[tokio::test]
async fn without_consent_no_request_and_pending_consent() {
    let (mock, server) = start().await;
    let openai = MockHttp::new(&server, "openai");
    let page = fields_page();
    let gate = ConsentGate::dialog(ConsentBook::new());
    for flow in flows() {
        let input = PreparedInput::new(&page, &flow.fields, &Redactor::default());
        let out = send(&gate, &openai, &flow.task, &input).await;
        assert_eq!(
            out.outcome,
            Outcome::Status(AiStatus::PendingConsent),
            "{}",
            flow.task
        );
        assert!(!out.sent);
    }
    assert_eq!(provider_requests(&mock).len(), 0, "Mock-Zähler 0");
    // consent for openai/page-type only → exactly that request goes out
    let page_type = flows().into_iter().find(|f| f.task == "page-type").unwrap();
    gate.grant(page_type.consent_key(), T0, None).unwrap();
    let input = PreparedInput::new(&page, &page_type.fields, &Redactor::default());
    let out = send(&gate, &openai, "page-type", &input).await;
    assert!(
        matches!(out.outcome, Outcome::Completed(_)),
        "{:?}",
        out.outcome
    );
    assert_eq!(provider_requests(&mock).len(), 1);
    server.stop().await;
}

// spec: AI-018
#[tokio::test]
async fn adding_page_text_to_title_suggestions_asks_again_before_next_request() {
    let (mock, server) = start().await;
    let openai = MockHttp::new(&server, "openai");
    let page = fields_page();
    let gate = ConsentGate::dialog(ConsentBook::new());
    let policies = ProviderPolicies::embedded();
    let mut title = flows()
        .into_iter()
        .find(|f| f.task == "suggest-title")
        .unwrap();
    gate.grant(title.consent_key(), T0, None).unwrap();
    let input = PreparedInput::new(&page, &title.fields, &Redactor::default());
    assert!(send(&gate, &openai, "suggest-title", &input).await.sent);
    assert_eq!(provider_requests(&mock).len(), 1);
    assert!(!provider_requests(&mock)[0].body.contains("MARK-TEXT"));

    title.fields.insert(InputField::PageText);
    let dialog =
        DataFlowOverview::consent_requests(std::slice::from_ref(&title), &policies, &gate, true);
    assert_eq!(dialog.len(), 1, "dialog appears again");
    assert_eq!(
        dialog[0].added_fields,
        FieldSet::new([InputField::PageText])
    );
    let input = PreparedInput::new(&page, &title.fields, &Redactor::default());
    let out = send(&gate, &openai, "suggest-title", &input).await;
    assert_eq!(out.outcome, Outcome::Status(AiStatus::PendingConsent));
    assert_eq!(
        provider_requests(&mock).len(),
        1,
        "no request with page text before consent"
    );

    gate.grant(title.consent_key(), T0 + 1, Some(dialog[0].row.clone()))
        .unwrap();
    assert!(send(&gate, &openai, "suggest-title", &input).await.sent);
    let log = provider_requests(&mock);
    assert_eq!(log.len(), 2);
    assert!(log[1].body.contains("MARK-TEXT"));
    server.stop().await;
}

// spec: AI-018
#[tokio::test]
async fn shown_fields_are_exactly_the_sent_fields() {
    let (mock, server) = start().await;
    let openai = MockHttp::new(&server, "openai");
    let page = fields_page();
    let gate = ConsentGate::cli(true);
    let policies = ProviderPolicies::embedded();
    let mut sets: Vec<FieldSet> = flows().into_iter().map(|f| f.fields).collect();
    sets.extend(InputField::ALL.into_iter().map(|f| FieldSet::new([f])));
    sets.push(FieldSet::new(InputField::ALL));
    for (i, set) in sets.iter().enumerate() {
        let input = PreparedInput::new(&page, set, &Redactor::default());
        assert!(
            send(&gate, &openai, &format!("task-{i}"), &input)
                .await
                .sent
        );
        let body = &provider_requests(&mock)[i].body;
        // the overview lists exactly this set
        let flow = TaskDataFlow {
            task: format!("task-{i}"),
            provider: "openai".into(),
            model: "mock-model".into(),
            endpoint: server.base_url("openai"),
            region: None,
            fields: set.clone(),
            max_chars: None,
            api: None,
        };
        let row = &DataFlowOverview::build(&[flow], &policies, &gate, true).rows[0];
        assert_eq!(&row.fields_sent, set);
        for (field, marker) in MARKERS {
            assert_eq!(
                body.contains(marker),
                set.contains(field),
                "set {:?}: marker {marker} of field {}",
                set.ids(),
                field.id()
            );
        }
    }
    server.stop().await;
}

// spec: AI-018
#[tokio::test]
async fn revocation_stops_open_entries_of_that_provider_immediately() {
    let (mock, server) = start().await;
    let openai = MockHttp::new(&server, "openai");
    let ollama = MockHttp::new(&server, "ollama");
    let page = fields_page();
    let fields = FieldSet::new([InputField::Url, InputField::Title]);
    let input = PreparedInput::new(&page, &fields, &Redactor::default());
    let gate = ConsentGate::dialog(ConsentBook::new());
    gate.grant(input.consent_key("openai", "page-type"), T0, None)
        .unwrap();
    gate.grant(input.consent_key("ollama", "page-type"), T0, None)
        .unwrap();
    let mut statuses = Vec::new();
    for i in 0..6 {
        if i == 2 {
            gate.revoke_provider("openai").unwrap();
        }
        let out = send(&gate, &openai, "page-type", &input).await;
        statuses.push(match out.outcome {
            Outcome::Completed(_) => "Success".to_string(),
            Outcome::Status(s) => s.to_string(),
            other => format!("{other:?}"),
        });
        assert!(
            send(&gate, &ollama, "page-type", &input).await.sent,
            "other provider runs on"
        );
    }
    assert_eq!(
        statuses,
        [
            "Success",
            "Success",
            "Pending (consent)",
            "Pending (consent)",
            "Pending (consent)",
            "Pending (consent)"
        ]
    );
    let log = provider_requests(&mock);
    assert_eq!(
        log.iter().filter(|r| r.path.starts_with("/openai")).count(),
        2
    );
    assert_eq!(
        log.iter().filter(|r| r.path.starts_with("/ollama")).count(),
        6
    );
    server.stop().await;
}

// spec: AI-018
#[tokio::test]
async fn cli_without_consent_switch_runs_no_ai_task() {
    let (mock, server) = start().await;
    let openai = MockHttp::new(&server, "openai");
    let page = fields_page();
    let gate = ConsentGate::cli(false);
    for flow in flows() {
        let input = PreparedInput::new(&page, &flow.fields, &Redactor::default());
        let out = send(&gate, &openai, &flow.task, &input).await;
        assert_eq!(out.outcome, Outcome::Status(AiStatus::PendingConsent));
    }
    assert!(gate.cli_message().is_some());
    assert_eq!(provider_requests(&mock).len(), 0);
    let gate = ConsentGate::cli(true);
    let input = PreparedInput::new(&page, &flows()[0].fields, &Redactor::default());
    assert!(send(&gate, &openai, "suggest-title", &input).await.sent);
    assert_eq!(provider_requests(&mock).len(), 1);
    server.stop().await;
}

// spec: AI-018
#[tokio::test]
async fn preview_switch_off_sends_nothing_even_with_consent() {
    let (mock, server) = start().await;
    let openai = MockHttp::new(&server, "openai");
    let page = fields_page();
    let input = PreparedInput::new(&page, &page.fields(), &Redactor::default());
    let out = guard::execute(
        PreviewSwitch::OFF,
        &ConsentGate::cli(true),
        "t",
        &input,
        &openai,
        request("openai"),
    )
    .await;
    assert_eq!(out.outcome, Outcome::Disabled);
    assert_eq!(provider_requests(&mock).len(), 0);
    server.stop().await;
}

// spec: AI-024
#[tokio::test]
async fn pii_request_contains_placeholders_not_values() {
    let (mock, server) = start().await;
    let openai = MockHttp::new(&server, "openai");
    let page = site_page("pii.html");
    let input = PreparedInput::new(&page, &page.fields(), &Redactor::default());
    let out = send(&ConsentGate::cli(true), &openai, "page-type", &input).await;
    assert!(out.sent);
    assert_eq!(out.redactions, 3, "Redactions = 3");
    let log = provider_requests(&mock);
    assert_eq!(log.len(), 1);
    for original in [
        "max@example.com",
        "+49 30 1234567",
        "DE89 3704 0044 0532 0130 00",
        "DE89370400440532013000",
    ] {
        assert!(!log[0].body.contains(original), "{original}");
    }
    // article number in IBAN shape with wrong check digits stays (AI-024)
    assert!(log[0].body.contains("DE12 3456 7890 1234 5678 90"));
    for placeholder in ["[EMAIL]", "[PHONE]", "[IBAN]"] {
        assert!(log[0].body.contains(placeholder), "{placeholder}");
    }
    server.stop().await;
}

const SITE_PII: [&str; 7] = [
    "kontakt@example.test",
    "+49 30 123456",
    "legal@example.test",
    "+49 30 654321",
    "DE89 3704 0044 0532 0130 00",
    "DE89370400440532013000",
    "30 123456",
];

// spec: AI-024
#[tokio::test]
async fn contact_and_imprint_requests_contain_no_contact_data_by_default() {
    let (mock, server) = start().await;
    let openai = MockHttp::new(&server, "openai");
    let redactor = Redactor::new(RedactionSettings::default());
    let mut total = 0;
    for file in ["contact.html", "legal.html"] {
        let page = site_page(file);
        // sanity: the fixture really carries contact data in several fields
        assert!(page
            .text(InputField::PageText)
            .unwrap()
            .contains("@example.test"));
        assert!(page
            .text(InputField::MetaDescription)
            .unwrap()
            .contains("+49 30"));
        let input = PreparedInput::new(&page, &page.fields(), &redactor);
        let out = send(&ConsentGate::cli(true), &openai, "page-type", &input).await;
        assert!(out.sent);
        total += out.redactions;
    }
    // contact: mail + phone in meta and text = 4; imprint: mail + phone + IBAN in meta and text = 6
    assert_eq!(total, 10);
    let log = provider_requests(&mock);
    assert_eq!(log.len(), 2);
    let all: String = log.iter().map(|r| r.body.as_str()).collect();
    for value in SITE_PII {
        assert_eq!(all.matches(value).count(), 0, "{value} found in mock log");
    }
    assert!(all.contains("MOCK-ID:contact") && all.contains("MOCK-ID:legal"));
    server.stop().await;
}

// spec: AI-024
#[tokio::test]
async fn redaction_off_sends_originals_and_dialog_shows_off_with_notice() {
    let (mock, server) = start().await;
    let openai = MockHttp::new(&server, "openai");
    let page = site_page("legal.html");
    let input = PreparedInput::new(
        &page,
        &page.fields(),
        &Redactor::new(RedactionSettings::off()),
    );
    let out = send(&ConsentGate::cli(true), &openai, "page-type", &input).await;
    assert_eq!(out.redactions, 0);
    let body = &provider_requests(&mock)[0].body;
    for value in [
        "legal@example.test",
        "+49 30 654321",
        "DE89 3704 0044 0532 0130 00",
    ] {
        assert!(body.contains(value), "{value}");
    }
    let flow = TaskDataFlow {
        task: "page-type".into(),
        provider: "openai".into(),
        model: "mock-model".into(),
        endpoint: server.base_url("openai"),
        region: None,
        fields: page.fields(),
        max_chars: Some(3000),
        api: None,
    };
    let dialog = DataFlowOverview::consent_requests(
        &[flow],
        &ProviderPolicies::embedded(),
        &ConsentGate::dialog(ConsentBook::new()),
        false,
    );
    assert_eq!(dialog[0].row.redaction_cell(), "off");
    assert!(dialog[0].pii_notice);
    server.stop().await;
}
