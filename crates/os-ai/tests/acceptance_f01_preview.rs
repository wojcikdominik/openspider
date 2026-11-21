//! Unabhängige Abnahmetests M4-F01: Schalter „KI (Vorschau)“ und Routing-Sperren in `os_ai::provider::execute`.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-001, AI-020) und `docs/plan/M4.md` (E1, E-2 b:
//! Schalter aus = kein KI-Request, kein KI-Schritt; Standard aus).
// acceptance: independent

use std::io::Write as _;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use os_ai::provider::{
    execute, AiProvider, Capabilities, CapabilitySource, Input, Model, Outcome, ProviderError,
    ProviderFuture, Request, Response, Usage,
};
use os_ai::{AiStatus, PreviewSwitch};
use os_model::settings::AppSettings;
use os_model::CrawlConfig;

/// Anbieter-Attrappe: zählt Aufrufe und öffnet bei jedem `execute` eine echte TCP-Verbindung zu einem lokalen
/// Lauscher. So lässt sich „kein Netzwerk-Request“ von außen am Lauscher prüfen, nicht nur am Zähler.
struct NetProvider {
    id: &'static str,
    calls: Arc<AtomicUsize>,
    model_calls: Arc<AtomicUsize>,
    target: std::net::SocketAddr,
    answer: Result<Response, ProviderError>,
}

impl AiProvider for NetProvider {
    fn provider_id(&self) -> &str {
        self.id
    }
    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        self.model_calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(Vec::new()) })
    }
    fn execute(&self, _request: Request) -> ProviderFuture<'_, Response> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut s) = TcpStream::connect_timeout(&self.target, Duration::from_secs(2)) {
            let _ = s.write_all(b"POST /v1/messages HTTP/1.1\r\nHost: mock.test\r\n\r\n");
        }
        let answer = self.answer.clone();
        Box::pin(async move { answer })
    }
}

struct Harness {
    listener: TcpListener,
    calls: Arc<AtomicUsize>,
    model_calls: Arc<AtomicUsize>,
}

impl Harness {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        Harness {
            listener,
            calls: Arc::new(AtomicUsize::new(0)),
            model_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn provider(&self, id: &'static str, answer: Result<Response, ProviderError>) -> NetProvider {
        NetProvider {
            id,
            calls: self.calls.clone(),
            model_calls: self.model_calls.clone(),
            target: self.listener.local_addr().unwrap(),
            answer,
        }
    }
    /// Anzahl der Verbindungen, die beim Lauscher angekommen sind.
    fn connections(&self) -> usize {
        let mut n = 0;
        while self.listener.accept().is_ok() {
            n += 1;
        }
        n
    }
}

fn caps_all() -> Capabilities {
    Capabilities {
        text: true,
        structured_output: true,
        vision: true,
        embeddings: true,
        decisions: true,
        batch: true,
        prices_via_api: true,
        local: true,
    }
}

fn caps_text_only() -> Capabilities {
    Capabilities {
        text: true,
        structured_output: true,
        ..Capabilities::default()
    }
}

fn model(provider: &str, caps: Capabilities) -> Model {
    Model {
        provider: provider.into(),
        id: "mock-claude".into(),
        endpoint: "messages".into(),
        capabilities: caps,
        capability_source: CapabilitySource::ModelCatalog,
    }
}

fn text_request(model: Model) -> Request {
    Request {
        model,
        required: Capabilities {
            text: true,
            ..Capabilities::default()
        },
        input: vec![Input::Text {
            text: "MOCK-ID:home Startseite".into(),
        }],
        instructions: "gib {\"ok\": true} zurück".into(),
        questions: Vec::new(),
        response_schema: None,
        max_tokens: Some(64),
        temperature: None,
        effort: None,
    }
}

fn ok_response() -> Response {
    Response {
        content: Some(serde_json::json!({"ok": true})),
        usage: Usage {
            input_tokens: Some(10),
            output_tokens: Some(3),
            cached_input_tokens: None,
            cache_write_tokens: None,
        },
        refusal: false,
        provider_request_id: Some("req_1".into()),
        duration_ms: 5,
    }
}

// spec: AI-001
// acceptance: independent
// Plan M4 E1 / E-2 b: Der Schalter „KI (Vorschau)“ ist in den App-Einstellungen standardmäßig aus.
#[test]
fn preview_switch_is_off_by_default() {
    assert!(!PreviewSwitch::default().is_on());
    assert!(!PreviewSwitch::from_settings(&AppSettings::default()).is_on());
    assert!(!AppSettings::default().ai_app.preview_enabled);
    let mut on = AppSettings::default();
    on.ai_app.preview_enabled = true;
    assert!(PreviewSwitch::from_settings(&on).is_on());
}

// spec: AI-001
// acceptance: independent
// Plan M4 E1 / E-2 b: Schalter aus → kein KI-Request (Adapter wird nicht aufgerufen, keine Verbindung).
#[tokio::test]
async fn switch_off_sends_no_request_and_returns_disabled() {
    let h = Harness::new();
    let provider = h.provider("anthropic", Ok(ok_response()));
    let req = text_request(model("anthropic", caps_all()));
    let outcome = execute(PreviewSwitch::OFF, &provider, req).await;
    assert_eq!(outcome, Outcome::Disabled);
    assert_eq!(
        h.calls.load(Ordering::SeqCst),
        0,
        "Adapter darf nicht laufen"
    );
    assert_eq!(h.model_calls.load(Ordering::SeqCst), 0);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(h.connections(), 0, "kein Netzwerk-Request bei Schalter aus");
}

// spec: AI-001
// acceptance: independent
// Schalter aus gilt auch für Anfragen, die das Routing sonst durchließe, und für jedes Ergebnis des Adapters
// (Fehler, Refusal): Es entsteht kein Status, der einen Request voraussetzt.
#[tokio::test]
async fn switch_off_blocks_every_kind_of_request() {
    let h = Harness::new();
    let answers: Vec<Result<Response, ProviderError>> = vec![
        Ok(ok_response()),
        Ok(Response {
            refusal: true,
            ..ok_response()
        }),
        Err(ProviderError::MissingApiKey),
        Err(ProviderError::QuotaExceeded),
        Err(ProviderError::Http {
            status: 500,
            message: "Internal Server Error".into(),
        }),
    ];
    for answer in answers {
        let provider = h.provider("openai", answer);
        let mut img = text_request(model("openai", caps_all()));
        img.input.push(Input::Image {
            media_type: "image/png".into(),
            base64: "iVBORw0KGgo=".into(),
        });
        img.response_schema = Some(serde_json::json!({"type": "object"}));
        for req in [text_request(model("openai", caps_all())), img] {
            assert_eq!(
                execute(PreviewSwitch::OFF, &provider, req).await,
                Outcome::Disabled
            );
        }
    }
    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(h.connections(), 0);
}

// spec: AI-001
// acceptance: independent
// Gegenprobe: Mit Schalter an wird der passende Adapter genau einmal aufgerufen und das gemeinsame Ergebnis
// unverändert durchgereicht (Inhalt, usage, Request-ID, Laufzeit).
#[tokio::test]
async fn switch_on_routes_capable_request_once() {
    let h = Harness::new();
    let provider = h.provider("anthropic", Ok(ok_response()));
    let outcome = execute(
        PreviewSwitch::ON,
        &provider,
        text_request(model("anthropic", caps_all())),
    )
    .await;
    assert_eq!(outcome, Outcome::Completed(ok_response()));
    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(h.connections(), 1);
}

// spec: AI-001
// acceptance: independent
// „Eine Aufgabe mit Bildeingabe wird nie an ein Modell ohne Vision-Fähigkeit gesendet; ist keines verfügbar,
// erhält sie `Skipped (no capable model)` (Mock-Log: 0 Requests).“ Auch dann nicht, wenn die Aufgabe Vision
// nicht ausdrücklich anfordert, das Bild aber in der Eingabe steckt.
#[tokio::test]
async fn image_task_never_reaches_model_without_vision() {
    let h = Harness::new();
    let provider = h.provider("openai", Ok(ok_response()));
    let mut declared = text_request(model("openai", caps_text_only()));
    declared.required.vision = true;
    let mut undeclared = text_request(model("openai", caps_text_only()));
    undeclared.input.push(Input::Image {
        media_type: "image/jpeg".into(),
        base64: "/9j/4AAQ".into(),
    });
    for req in [declared, undeclared] {
        let outcome = execute(PreviewSwitch::ON, &provider, req).await;
        assert_eq!(
            outcome,
            Outcome::Status(AiStatus::SkippedNoCapableModel),
            "Bildaufgabe an Nicht-Vision-Modell"
        );
        if let Outcome::Status(s) = &outcome {
            assert_eq!(s.to_string(), "Skipped (no capable model)");
        }
    }
    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(h.connections(), 0);
}

// spec: AI-001
// acceptance: independent
// „eine Aufgabe wird nur an einen Adapter geroutet, der alle benötigten Fähigkeiten hat“ – je Fähigkeit der
// Matrix einzeln: fehlt genau die benötigte Fähigkeit, gibt es keinen Request.
#[tokio::test]
async fn every_missing_required_capability_blocks_routing() {
    let h = Harness::new();
    let provider = h.provider("openrouter", Ok(ok_response()));
    let single: [fn(&mut Capabilities); 8] = [
        |c| c.text = true,
        |c| c.structured_output = true,
        |c| c.vision = true,
        |c| c.embeddings = true,
        |c| c.decisions = true,
        |c| c.batch = true,
        |c| c.prices_via_api = true,
        |c| c.local = true,
    ];
    for (i, set) in single.iter().enumerate() {
        let mut required = Capabilities::default();
        set(&mut required);
        // Modell mit allen Fähigkeiten außer der geforderten.
        let mut available = caps_all();
        let mut flip = Capabilities::default();
        set(&mut flip);
        available.text &= !flip.text;
        available.structured_output &= !flip.structured_output;
        available.vision &= !flip.vision;
        available.embeddings &= !flip.embeddings;
        available.decisions &= !flip.decisions;
        available.batch &= !flip.batch;
        available.prices_via_api &= !flip.prices_via_api;
        available.local &= !flip.local;
        let mut req = text_request(model("openrouter", available));
        req.required = required;
        assert_eq!(
            execute(PreviewSwitch::ON, &provider, req).await,
            Outcome::Status(AiStatus::SkippedNoCapableModel),
            "Fähigkeit Nr. {i} fehlt, Request darf nicht gesendet werden"
        );
        // Gegenprobe: mit der Fähigkeit geht der Request hinaus.
        let mut ok = text_request(model("openrouter", caps_all()));
        ok.required = required;
        assert!(matches!(
            execute(PreviewSwitch::ON, &provider, ok).await,
            Outcome::Completed(_)
        ));
    }
    assert_eq!(h.calls.load(Ordering::SeqCst), 8);
}

// spec: AI-001
// acceptance: independent
// Strukturierte Ausgabe nach JSON-Schema ist eine Fähigkeit: Ein Request mit Antwortschema geht nicht an ein
// Modell ohne `Structured Output`.
#[tokio::test]
async fn schema_request_needs_structured_output() {
    let h = Harness::new();
    let provider = h.provider("ollama", Ok(ok_response()));
    let mut req = text_request(model(
        "ollama",
        Capabilities {
            text: true,
            local: true,
            ..Capabilities::default()
        },
    ));
    req.response_schema = Some(serde_json::json!({
        "type": "object",
        "properties": {"ok": {"type": "boolean"}},
        "required": ["ok"],
        "additionalProperties": false
    }));
    assert_eq!(
        execute(PreviewSwitch::ON, &provider, req).await,
        Outcome::Status(AiStatus::SkippedNoCapableModel)
    );
    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
}

// spec: AI-020
// acceptance: independent
// Endgültige Anbieterfehler und Refusals erscheinen als Statuswerte der Tabelle aus AI-020.
#[tokio::test]
async fn provider_outcomes_map_to_spec_status_values() {
    let h = Harness::new();
    let cases: Vec<(Result<Response, ProviderError>, &str)> = vec![
        (
            Ok(Response {
                refusal: true,
                content: None,
                ..ok_response()
            }),
            "Refusal",
        ),
        (Err(ProviderError::Refusal), "Refusal"),
        (Err(ProviderError::MissingApiKey), "Error: Missing API key"),
        (
            Err(ProviderError::Timeout { seconds: 30 }),
            "Error: Timeout nach 30 s",
        ),
        (Err(ProviderError::InvalidOutput), "Error: Invalid output"),
        (Err(ProviderError::MissingAnswer), "Error: Missing answer"),
        (Err(ProviderError::BatchExpired), "Error: Batch expired"),
        (Err(ProviderError::QuotaExceeded), "Quota exceeded"),
        (Err(ProviderError::Policy), "Skipped (policy)"),
        (
            Err(ProviderError::Http {
                status: 500,
                message: "Internal Server Error".into(),
            }),
            "Error: 500 Internal Server Error",
        ),
    ];
    for (answer, expected) in cases {
        let provider = h.provider("anthropic", answer.clone());
        let outcome = execute(
            PreviewSwitch::ON,
            &provider,
            text_request(model("anthropic", caps_all())),
        )
        .await;
        match outcome {
            Outcome::Status(s) => assert_eq!(s.to_string(), expected, "für {answer:?}"),
            other => panic!("{answer:?}: erwartet Status {expected:?}, erhalten {other:?}"),
        }
    }
}

// spec: AI-020
// acceptance: independent
// „Error: <HTTP-Code> <Meldung>“ für sonstige endgültige Anbieterfehler: auch Rate-Limit (429) und Überlast
// (529) nach allen Wiederholungen erscheinen mit ihrem HTTP-Code und nicht als `Quota exceeded`
// (vorübergehende Fälle erzeugen diesen Status nicht).
#[tokio::test]
async fn final_rate_limit_and_overload_keep_http_code() {
    let h = Harness::new();
    for (err, code) in [
        (
            ProviderError::RateLimited {
                retry_after_seconds: Some(7),
            },
            "Error: 429 ",
        ),
        (ProviderError::Overloaded, "Error: 529 "),
    ] {
        let provider = h.provider("anthropic", Err(err.clone()));
        let outcome = execute(
            PreviewSwitch::ON,
            &provider,
            text_request(model("anthropic", caps_all())),
        )
        .await;
        match outcome {
            Outcome::Status(s) => {
                let text = s.to_string();
                assert!(text.starts_with(code), "{err:?} → {text}");
                assert!(s.is_error());
            }
            other => panic!("{err:?}: {other:?}"),
        }
    }
}

// spec: AI-001
// acceptance: independent
// Plan M4 E1 / E-2 b: Mit Schalter aus läuft kein KI-Schritt. `PreviewSwitch::restrict` schaltet für den Lauf alle
// KI-Teile des Profils ab (KI-Aufgaben, Embeddings, Analyse-Schritte `embeddings` und `ai_search`), lässt alles
// andere unverändert; mit Schalter an ändert sich nichts.
#[test]
fn restrict_disables_only_ai_parts_of_the_run_copy() {
    let mut profile = CrawlConfig::default();
    profile.ai.enabled = true;
    profile.embeddings.enabled = true;
    profile.analysis.embeddings = true;
    profile.analysis.ai_search = true;
    profile.speed.threads = 7;

    let mut on = profile.clone();
    PreviewSwitch::ON.restrict(&mut on);
    assert_eq!(on, profile, "Schalter an: Profil unverändert");

    let mut run = profile.clone();
    PreviewSwitch::OFF.restrict(&mut run);
    assert!(!run.ai.enabled);
    assert!(!run.embeddings.enabled);
    assert!(!run
        .analysis
        .step_enabled(os_model::enums::AnalysisStep::Embeddings));
    assert!(!run
        .analysis
        .step_enabled(os_model::enums::AnalysisStep::AiSearch));

    // Nur die KI-Teile ändern sich.
    let mut expected = profile.clone();
    expected.ai.enabled = false;
    expected.embeddings.enabled = false;
    expected.analysis.embeddings = false;
    expected.analysis.ai_search = false;
    assert_eq!(run, expected);
    assert_eq!(run.speed.threads, 7);
    assert!(run.analysis.link_score);
}

// spec: AI-001
// acceptance: independent
// Profil mit KI-Aufgaben aus einer Datei (Fixture) und Standard-App-Einstellungen: Der Lauf hat keine KI-Teile.
#[test]
fn profile_fixture_with_ai_tasks_is_restricted_under_default_settings() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/ai-acceptance/profile-ai-full.osconfig");
    let loaded = CrawlConfig::load_file(&path).unwrap();
    let mut run = loaded.config.clone();
    assert!(run.ai.enabled && run.embeddings.enabled);
    PreviewSwitch::from_settings(&AppSettings::default()).restrict(&mut run);
    assert!(!run.ai.enabled);
    assert!(!run.embeddings.enabled);
    assert!(!run.analysis.embeddings);
    assert!(!run.analysis.ai_search);
    // Aufgaben, Fragen und Budget im Profil selbst bleiben erhalten (nur die Laufkopie ist eingeschränkt).
    assert_eq!(run.ai.tasks, loaded.config.ai.tasks);
    assert_eq!(run.ai.budget, loaded.config.ai.budget);
}

// spec: AI-001
// acceptance: independent
// Matrix AI-001: „OpenAI Decisions – Decisions (Text und Bild)“. Ein Decisions-Modell mit genau dieser Zeile der
// Matrix (nur Spalte `Decisions`) nimmt eine Decision-Frage mit Bildeingabe an (Aufgabentyp `decision-vision`,
// AI-011); das Bild darf den Request nicht in `Skipped (no capable model)` umleiten.
#[tokio::test]
async fn decisions_model_accepts_text_and_image_decision_requests() {
    let h = Harness::new();
    let provider = h.provider("openai", Ok(ok_response()));
    let decisions_only = Capabilities {
        decisions: true,
        ..Capabilities::default()
    };
    let mut m = model("openai", decisions_only);
    m.id = "gpt-6-luna".into();
    m.endpoint = "decisions".into();
    let base = Request {
        required: decisions_only,
        questions: vec![serde_json::json!({"name": "Alt Text Matches", "type": "predicate"})],
        ..text_request(m)
    };
    let text_only = base.clone();
    let mut with_image = base;
    with_image.input.push(Input::Image {
        media_type: "image/png".into(),
        base64: "iVBORw0KGgo=".into(),
    });
    for (label, req) in [("Text", text_only), ("Bild", with_image)] {
        let outcome = execute(PreviewSwitch::ON, &provider, req).await;
        assert!(
            matches!(outcome, Outcome::Completed(_)),
            "Decision-Frage mit {label}: {outcome:?}"
        );
    }
    assert_eq!(h.calls.load(Ordering::SeqCst), 2);
}
