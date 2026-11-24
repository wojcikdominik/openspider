//! Tests der Probe ohne echte Anbieter und ohne Schlüsselbund: Trockenlauf, Modusschalter, Schlüsselschutz,
//! Kostenbremse, keine Wiederholungen – gegen einen lokalen Testserver (127.0.0.1). Der Testschlüssel kommt nur
//! über die Testquelle [`TestKeys`] herein.

mod support;

use probe::cli::{parse_args, Mode, Options};
use probe::plan::{self, Endpoints, Provider};
use probe::record::{self, VALUE_NOT_RECORDED};
use probe::runner::{plan_lines, print_plan, Probe, RunConfig};
use probe::secret::{ApiSecret, KeyLookup, KeySource, Scrubber};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use support::{Behavior, Server};

const TEST_KEY: &str = "sk-PROBE-TEST-0123456789-SECRETVALUE-XYZW";
const TEST_KEY_TAIL: &str = "XYZW";

/// Nur im Test vorhandene Schlüsselquelle.
struct TestKeys(HashMap<Provider, &'static str>);

impl TestKeys {
    fn all() -> Self {
        Self(Provider::ALL.iter().map(|p| (*p, TEST_KEY)).collect())
    }
    fn without(provider: Provider) -> Self {
        let mut k = Self::all();
        k.0.remove(&provider);
        k
    }
}

impl KeySource for TestKeys {
    fn lookup(&self, provider: Provider) -> KeyLookup {
        match self.0.get(&provider) {
            Some(v) => KeyLookup::Found {
                secret: ApiSecret::new(*v),
                service: "Test",
            },
            None => KeyLookup::Missing {
                note: "Test: kein Eintrag".into(),
            },
        }
    }
}

/// Schlüsselquelle, die bei jedem Zugriff abbricht (Trockenlauf darf sie nie fragen).
struct PanickingKeys;
impl KeySource for PanickingKeys {
    fn lookup(&self, _: Provider) -> KeyLookup {
        panic!("Schlüsselquelle im Trockenlauf befragt")
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("q00-probe-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn live_options() -> Options {
    Options {
        mode: Mode::Live,
        ..Options::default()
    }
}

async fn run(
    server: &Server,
    options: Options,
    keys: &dyn KeySource,
    dir: &Path,
    provider_cap: Option<f64>,
) -> (probe::runner::Summary, String) {
    let mut config = RunConfig::new(options, dir.to_path_buf());
    config.endpoints = server.endpoints();
    config.provider_cap_usd = provider_cap;
    let mut out: Vec<u8> = Vec::new();
    let summary = Probe::new(&config, &mut out)
        .expect("client")
        .run(keys)
        .await
        .expect("run");
    (summary, String::from_utf8(out).expect("utf8"))
}

fn read_json(dir: &Path, name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join(name)).expect("datei")).expect("json")
}

fn all_files_text(dir: &Path) -> String {
    let mut text = String::new();
    for entry in std::fs::read_dir(dir).expect("dir") {
        text.push_str(&std::fs::read_to_string(entry.expect("entry").path()).expect("lesen"));
    }
    text
}

// spec: AI-009 (Schlüssel nie in Ausgaben), AI-017 (keine Wiederholung im Probe-Lauf), AI-002/003/004/005 (Plan)
#[tokio::test]
async fn full_run_executes_every_planned_step_once_and_never_leaks_the_key() {
    let server = Server::start(Behavior::new(TEST_KEY)).await;
    let dir = temp_dir("full");
    let (summary, stdout) = run(&server, live_options(), &TestKeys::all(), &dir, None).await;

    // Jede geplante Anfrage genau einmal: Serverlog = Plan = Buchungen.
    let planned = plan::STEPS.len();
    assert_eq!(server.requests().len(), planned, "Serverlog");
    assert_eq!(summary.requests, planned, "gebuchte Anfragen");
    for provider in Provider::ALL {
        let file = read_json(&dir, &format!("{}.json", provider.name()));
        let records = file["requests"].as_array().expect("requests");
        assert_eq!(records.len(), plan::steps(provider).count());
        assert!(
            records.iter().all(|r| r["executed"] == true),
            "{provider:?}: {records:#?}"
        );
        assert!(file["totals"]["aborted"].is_null());
        // Keine Authentifizierungs-Header in den aufgezeichneten Anfragen.
        for r in records {
            let headers = r["request"]["headers"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            assert!(headers.keys().all(|k| !record::is_auth_header(k)), "{r}");
        }
    }

    // Der Schlüssel ging wirklich hinaus …
    let anthropic = server.requests_for("/anthropic");
    assert!(anthropic
        .iter()
        .all(|r| r.header("x-api-key") == Some(TEST_KEY)));
    let bearer = format!("Bearer {TEST_KEY}");
    assert!(server
        .requests_for("/openai")
        .iter()
        .all(|r| r.header("authorization") == Some(bearer.as_str())));
    assert!(server
        .requests_for("/openrouter")
        .iter()
        .all(|r| r.header("authorization") == Some(bearer.as_str())));

    // … erscheint aber weder in der Konsole noch in einer Ergebnisdatei (auch nicht maskiert mit seinen letzten
    // Zeichen), obwohl der Server ihn in Header und Fehlermeldungen zurückgibt.
    let files = all_files_text(&dir);
    for haystack in [&stdout, &files] {
        assert!(!haystack.contains(TEST_KEY));
        assert!(!haystack.contains(TEST_KEY_TAIL));
        assert!(!haystack.contains("SECRETVALUE"));
    }
    assert!(
        files.contains("[REDACTED]"),
        "Fehlermeldung mit Schlüssel wurde geschwärzt"
    );

    // Keine Konto-/Nutzerkennungen, Request-IDs, undurchsichtigen Blobs oder kundeneigenen Modelle (Auswertung Q00).
    for needle in [
        "SECRETACCOUNT",
        "SECRETWORKSPACE",
        "acmecorp",
        "SIGBLOB",
        "ENCBLOB",
        "req_test",
        "xreq_test",
        "msg_test",
        "gen-test",
        "resp_test",
        "2026-12-01T16:45",
    ] {
        assert!(!files.contains(needle), "{needle} in Ergebnisdateien");
    }
    assert!(files.contains(record::OMITTED_ACCOUNT));

    // Header: Rate-Limit- und Request-ID-Werte ja, andere nur mit Namen.
    let a = read_json(&dir, "anthropic.json");
    let text = a["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["step"] == "anthropic.raw.text")
        .unwrap()
        .clone();
    assert_eq!(
        text["response_headers"]["anthropic-ratelimit-requests-remaining"],
        "49"
    );
    assert_eq!(
        text["response_headers"]["request-id"],
        "req_…(ausgelassen: Anbieter-ID)"
    );
    assert_eq!(
        text["response_body"]["id"],
        "msg_…(ausgelassen: Anbieter-ID)"
    );
    assert_eq!(text["response_headers"]["x-echo-key"], VALUE_NOT_RECORDED);
    assert_eq!(text["status"], 200);
    assert_eq!(text["cost_basis"], "usage");
    assert!(text["usage"]["input_tokens"].is_u64());

    // Modellwahl aus der Liste, gpt-6-luna-Prüfung, OpenRouter ohne :free und ohne Luxusmodell.
    assert_eq!(a["selection"]["text"]["model"], "claude-test-haiku");
    let o = read_json(&dir, "openai.json");
    assert_eq!(o["facts"]["gpt_6_luna_listed"], true);
    assert_eq!(o["selection"]["text"]["model"], "gpt-6-luna");
    let r = read_json(&dir, "openrouter.json");
    assert_eq!(r["selection"]["text"]["model"], "vendor/cheapest-text");
    assert_eq!(r["selection"]["structured"]["model"], "vendor/cheap-vision");
    assert_eq!(r["selection"]["vision"]["model"], "vendor/cheap-vision");

    // Adapter kommen mit den Antworten zurecht.
    let adapter_ok = |file: &Value, step: &str| {
        file["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["step"] == step)
            .map(|x| x["adapter_result"]["ok"] == true)
            .unwrap_or(false)
    };
    for (file, step) in [
        (&a, "anthropic.adapter.structured"),
        (&a, "anthropic.adapter.image"),
        (&o, "openai.adapter.responses_structured"),
        (&o, "openai.adapter.decisions_3q"),
        (&r, "openrouter.adapter.structured"),
        (&r, "openrouter.adapter.key"),
    ] {
        assert!(adapter_ok(file, step), "{step}");
    }
    assert!(!adapter_ok(&a, "anthropic.adapter.unknown_model"));

    let summary_file = read_json(&dir, "summary.json");
    assert_eq!(summary_file["total_requests"], planned);
    let o_models = o["requests"][0]["response_body"].clone();
    assert_eq!(o_models["custom_models_omitted"], 1);
    assert_eq!(o_models["count"], 3);
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-013 (hartes Budget, hier als Kostenbremse der Probe)
#[tokio::test]
async fn cost_brake_stops_a_provider_after_an_expensive_answer() {
    let mut behavior = Behavior::new(TEST_KEY);
    behavior.input_tokens = 1_000_000; // 30 USD nach vorsichtiger Preisannahme
    let server = Server::start(behavior).await;
    let dir = temp_dir("brake");
    let options = Options {
        only: Some(Provider::Anthropic),
        ..live_options()
    };
    let (summary, stdout) = run(&server, options, &TestKeys::all(), &dir, None).await;

    // Modelliste roh + Adapter, count_tokens (ohne usage → Obergrenze), dann die erste Textanfrage mit 1 Mio.
    // Tokens; danach nichts mehr.
    let sent = server.requests();
    assert_eq!(sent.len(), 4, "{sent:#?}");
    assert_eq!(summary.requests, 4);
    let file = read_json(&dir, "anthropic.json");
    let aborted = file["totals"]["aborted"].as_str().expect("abgebrochen");
    assert!(aborted.contains("Kostenbremse"), "{aborted}");
    let records = file["requests"].as_array().unwrap();
    assert_eq!(records.iter().filter(|r| r["executed"] == true).count(), 4);
    assert!(records.iter().skip(4).all(|r| r["executed"] == false));
    assert!(stdout.contains("Abbruch"));
    assert!(
        server.requests_for("/openai").is_empty() && server.requests_for("/openrouter").is_empty()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-013 (Gesamtgrenze nur nach unten)
#[tokio::test]
async fn lowered_total_cap_allows_only_free_requests() {
    let server = Server::start(Behavior::new(TEST_KEY)).await;
    let dir = temp_dir("total");
    let options = Options {
        // kleiner als die Obergrenze jeder kostenpflichtigen Anfrage, auch beim billigsten OpenRouter-Modell
        max_total_usd: 0.000_001,
        ..live_options()
    };
    let (summary, _) = run(&server, options, &TestKeys::all(), &dir, None).await;
    let sent = server.requests();
    assert!(sent.iter().all(|r| r.method == "GET"), "{sent:#?}");
    assert_eq!(summary.cost_usd, 0.0);
    for provider in Provider::ALL {
        let file = read_json(&dir, &format!("{}.json", provider.name()));
        assert!(file["totals"]["aborted"]
            .as_str()
            .unwrap_or("")
            .contains("Gesamtgrenze"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-013 (Anbietergrenze)
#[tokio::test]
async fn provider_cap_is_checked_before_each_request() {
    let server = Server::start(Behavior::new(TEST_KEY)).await;
    let dir = temp_dir("cap");
    let options = Options {
        only: Some(Provider::OpenRouter),
        ..live_options()
    };
    // Anbietergrenze kleiner als die Obergrenze einer einzigen kostenpflichtigen Anfrage.
    let (_, _) = run(&server, options, &TestKeys::all(), &dir, Some(0.000_001)).await;
    assert!(server.requests().iter().all(|r| r.method == "GET"));
    let file = read_json(&dir, "openrouter.json");
    assert!(file["totals"]["aborted"]
        .as_str()
        .unwrap()
        .contains("Kostenbremse"));
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-017 (Probe: bei 429/5xx kein automatischer Retry, nur aufzeichnen)
#[tokio::test]
async fn errors_are_recorded_without_retry() {
    for status in [429u16, 500] {
        let mut behavior = Behavior::new(TEST_KEY);
        behavior.fail_text_with = Some(status);
        let server = Server::start(behavior).await;
        let dir = temp_dir(&format!("retry-{status}"));
        let (summary, _) = run(&server, live_options(), &TestKeys::all(), &dir, None).await;
        assert_eq!(
            server.requests().len(),
            plan::STEPS.len(),
            "Status {status}"
        );
        assert_eq!(summary.requests, plan::STEPS.len());
        let failing = server
            .requests()
            .iter()
            .filter(|r| r.body.contains("pong"))
            .count();
        let text_steps = plan::STEPS
            .iter()
            .filter(|s| {
                s.id.ends_with("text")
                    || s.id.ends_with("unknown_model")
                    || s.id.ends_with("count_tokens")
            })
            .filter(|s| !s.id.contains("decisions"))
            .count();
        assert_eq!(
            failing, text_steps,
            "jede fehlschlagende Anfrage genau einmal"
        );
        let a = read_json(&dir, "anthropic.json");
        let raw_text = a["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["step"] == "anthropic.raw.text")
            .unwrap()
            .clone();
        assert_eq!(raw_text["status"], status);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// spec: AI-009 (fehlender Schlüssel → Anbieter übersprungen)
#[tokio::test]
async fn missing_key_skips_the_provider_without_requests() {
    let server = Server::start(Behavior::new(TEST_KEY)).await;
    let dir = temp_dir("missing");
    let (_, stdout) = run(
        &server,
        live_options(),
        &TestKeys::without(Provider::OpenRouter),
        &dir,
        None,
    )
    .await;
    assert!(server.requests_for("/openrouter").is_empty());
    assert!(!server.requests_for("/anthropic").is_empty());
    let file = read_json(&dir, "openrouter.json");
    assert_eq!(file["key"]["found"], false);
    assert!(file["requests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["executed"] == false));
    assert!(stdout.contains("[openrouter] übersprungen: kein Schlüssel"));
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-005, AI-010 (nur kostenlose Abfragen)
#[tokio::test]
async fn models_only_sends_only_free_requests() {
    let server = Server::start(Behavior::new(TEST_KEY)).await;
    let dir = temp_dir("models");
    let options = Options {
        models_only: true,
        ..live_options()
    };
    let (summary, _) = run(&server, options, &TestKeys::all(), &dir, None).await;
    let free = plan::STEPS.iter().filter(|s| s.models_only).count();
    assert_eq!(server.requests().len(), free);
    assert!(server.requests().iter().all(|r| r.method == "GET"));
    assert_eq!(summary.cost_usd, 0.0);
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-002, AI-003, AI-004, AI-005 (Plan); Trockenlauf ohne Netzwerk und Schlüsselbund
#[test]
fn dry_run_prints_the_full_plan_without_touching_keys() {
    let options = parse_args(&[], None).expect("optionen");
    assert_eq!(options.mode, Mode::DryRun { note: None });
    let mut out = Vec::new();
    print_plan(&options, &Endpoints::default(), &mut out).expect("druck");
    let text = String::from_utf8(out).unwrap();
    for step in plan::STEPS {
        assert!(text.contains(step.id), "{}", step.id);
    }
    for needle in [
        "https://api.anthropic.com/v1/messages",
        "https://api.openai.com/v1/decisions",
        "https://api.openai.com/v1/responses",
        "https://openrouter.ai/api/v1/key",
        "gpt-6-luna",
        "Trockenlauf",
    ] {
        assert!(text.contains(needle), "{needle}");
    }
    assert!(!text.contains("eu."), "keine EU-Endpunkte");
    // Die Schlüsselquelle existiert im Trockenlauf gar nicht als Parameter; zur Sicherheit:
    let _never_asked = PanickingKeys;
    let lines = plan_lines(&options, &Endpoints::default());
    assert_eq!(lines.len(), plan::STEPS.len());
    let total: f64 = lines.iter().map(|l| l.cost_cap_usd).sum();
    assert!(
        total > 0.0 && total < plan::TOTAL_CAP_USD,
        "Obergrenze gesamt {total}"
    );
    for provider in Provider::ALL {
        let n = lines.iter().filter(|l| l.provider == provider).count();
        assert!(n <= provider.max_requests());
        let sum: f64 = lines
            .iter()
            .filter(|l| l.provider == provider)
            .map(|l| l.cost_cap_usd)
            .sum();
        assert!(sum < plan::PROVIDER_CAP_USD, "{provider:?} {sum}");
    }
    // Anfragen über den Adapter nur, wo es einen Adapter gibt, und jede Anfrage mit Zweck.
    assert!(lines.iter().all(|l| !l.purpose.is_empty()));
}

// spec: E-8 b (Live nur mit Umgebungsvariable und --live)
#[test]
fn live_needs_both_env_and_flag() {
    let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        parse_args(&args(&["--live"]), Some("1")).unwrap().mode,
        Mode::Live
    );
    assert!(matches!(
        parse_args(&args(&["--live"]), None).unwrap().mode,
        Mode::DryRun { note: Some(_) }
    ));
    assert!(matches!(
        parse_args(&args(&["--live"]), Some("true")).unwrap().mode,
        Mode::DryRun { note: Some(_) }
    ));
    assert_eq!(
        parse_args(&args(&[]), Some("1")).unwrap().mode,
        Mode::DryRun { note: None }
    );
    assert_eq!(
        parse_args(&args(&["--live", "--dry-run"]), Some("1"))
            .unwrap()
            .mode,
        Mode::DryRun { note: None }
    );
    assert!(parse_args(&args(&["--max-total-usd", "7"]), None).is_err());
    assert!(parse_args(&args(&["--max-total-usd", "0"]), None).is_err());
    assert_eq!(
        parse_args(&args(&["--max-total-usd", "1.5"]), None)
            .unwrap()
            .max_total_usd,
        1.5
    );
    assert_eq!(
        parse_args(&args(&["--only", "openai"]), None).unwrap().only,
        Some(Provider::OpenAi)
    );
    assert!(parse_args(&args(&["--only", "gemini"]), None).is_err());
    assert!(
        parse_args(&args(&["--base-url", "x"]), None).is_err(),
        "keine Option für andere Hosts"
    );
    assert_eq!(
        parse_args(&args(&["--rebuild-results", "--live"]), Some("1"))
            .unwrap()
            .mode,
        Mode::Rebuild
    );
}

// spec: AI-009
#[test]
fn secret_debug_and_scrubber_hide_the_key() {
    let secret = ApiSecret::new(format!("  {TEST_KEY}\n"));
    assert_eq!(format!("{secret:?}"), "ApiSecret([REDACTED])");
    let mut s = Scrubber::default();
    s.add(&secret);
    assert_eq!(s.text(&format!("x {TEST_KEY} y")), "x [REDACTED] y");
    assert_eq!(s.text("sk-...XYZW"), "sk-...[REDACTED]");
    let v = s.json(&serde_json::json!({TEST_KEY: [TEST_KEY]}));
    assert!(!v.to_string().contains(TEST_KEY));
}

// spec: AI-009 (keine Kennungen in Ergebnissen); Auswertung Q00: summary.json aus getrennten --only-Läufen
#[tokio::test]
async fn separate_only_runs_are_composed_into_one_summary() {
    let server = Server::start(Behavior::new(TEST_KEY)).await;
    let dir = temp_dir("compose");
    for provider in [Provider::Anthropic, Provider::OpenRouter] {
        let options = Options {
            only: Some(provider),
            ..live_options()
        };
        run(&server, options, &TestKeys::all(), &dir, None).await;
    }
    let summary = read_json(&dir, "summary.json");
    let providers = summary["providers"].as_object().unwrap();
    assert_eq!(providers.len(), 2, "{summary:#}");
    let expected =
        plan::steps(Provider::Anthropic).count() + plan::steps(Provider::OpenRouter).count();
    assert_eq!(summary["total_requests"], expected);
    assert_eq!(
        summary["last_run"]["requests"],
        plan::steps(Provider::OpenRouter).count()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-009; Auswertung Q00: Bereinigen vorhandener Dateien ohne Netz
#[test]
fn rebuild_sanitizes_existing_files_and_composes_summary() {
    let dir = temp_dir("rebuild");
    std::fs::create_dir_all(&dir).unwrap();
    let doc = serde_json::json!({
        "provider": "openrouter",
        "totals": {"requests": 3, "estimated_cost_usd": 0.5, "aborted": null},
        "requests": [{"response_body": {"id": "gen-123-abc", "user_id": "user_X",
            "data": {"workspace_id": "cdc3-uuid", "creator_user_id": "user_Y", "organization_id": null}},
            "response_headers": {"x-request-id": "2a5f384a-0de2-4644-bb6b-df4483fb4eb1", "x-generation-id": "gen-9"},
            "content": [{"type": "thinking", "signature": "CAQS..."}],
            "ids": ["gpt-6-luna", "davinci:ft-acme-2023"]}]
    });
    std::fs::write(dir.join("openrouter.json"), doc.to_string()).unwrap();
    std::fs::write(
        dir.join("openai.json"),
        serde_json::json!({"totals": {"requests": 2, "estimated_cost_usd": 0.25}}).to_string(),
    )
    .unwrap();
    assert_eq!(probe::runner::rebuild_results(&dir).unwrap(), 2);
    let text = std::fs::read_to_string(dir.join("openrouter.json")).unwrap();
    for needle in [
        "user_X",
        "user_Y",
        "cdc3-uuid",
        "2a5f384a",
        "gen-123",
        "gen-9",
        "CAQS",
        "acme",
    ] {
        assert!(!text.contains(needle), "{needle}");
    }
    let cleaned: Value = serde_json::from_str(&text).unwrap();
    assert!(cleaned["requests"][0]["response_body"]["data"]["organization_id"].is_null());
    assert_eq!(cleaned["requests"][0]["ids"][0], "gpt-6-luna");
    let summary = read_json(&dir, "summary.json");
    assert_eq!(summary["total_requests"], 5);
    assert!((summary["total_estimated_cost_usd"].as_f64().unwrap() - 0.75).abs() < 1e-12);
    // Idempotent.
    probe::runner::rebuild_results(&dir).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("openrouter.json")).unwrap(),
        text
    );
    let _ = std::fs::remove_dir_all(&dir);
}
