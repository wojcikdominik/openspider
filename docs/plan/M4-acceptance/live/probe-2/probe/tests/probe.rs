//! Tests der Probe Q00b ohne echte Anbieter und ohne Schlüsselbund: Trockenlauf, Modusschalter, Schlüsselschutz und
//! Bereinigung, Kostenbremse, keine Wiederholungen, Beobachtungen zu V-A6/V-A8 – gegen einen lokalen Testserver
//! (127.0.0.1). Der Testschlüssel kommt nur über die Testquelle [`TestKeys`] herein.

mod support;

use probe::cli::{parse_args, Mode, Options};
use probe::plan::{self, Endpoints, Provider};
use probe::record::{self, VALUE_NOT_RECORDED};
use probe::runner::{plan_lines, print_plan, Probe, RunConfig};
use probe::secret::{ApiSecret, KeyLookup, KeySource, Scrubber};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use support::{Behavior, Server, NON_DISABLEABLE};

const TEST_KEY: &str = "sk-PROBE2-TEST-0123456789-SECRETVALUE-QRST";

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

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("q00b-probe-{}-{name}", std::process::id()));
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

fn record_of<'a>(file: &'a Value, step: &str) -> &'a Value {
    file["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["step"] == step)
        .unwrap_or_else(|| panic!("{step} fehlt"))
}

fn sent_body(server: &Server, prefix: &str, index: usize) -> Value {
    server
        .requests_for(prefix)
        .into_iter()
        .filter(|r| r.method == "POST")
        .nth(index)
        .expect("POST")
        .json()
}

// spec: AI-002, AI-003, AI-004, AI-005 (Nachträge 24.11.2026; V-A6, V-A8), AI-009
#[tokio::test]
async fn full_run_sends_each_planned_form_once_and_records_observations() {
    let server = Server::start(Behavior::new(TEST_KEY)).await;
    let dir = temp_dir("full");
    let (summary, stdout) = run(&server, live_options(), &TestKeys::all(), &dir, None).await;

    let planned = plan::STEPS.len();
    assert_eq!(server.requests().len(), planned, "Serverlog");
    assert_eq!(summary.requests, planned);
    for provider in Provider::ALL {
        let file = read_json(&dir, &format!("{}.json", provider.name()));
        let records = file["requests"].as_array().unwrap();
        assert_eq!(records.len(), plan::steps(provider).count());
        assert!(
            records.iter().all(|r| r["executed"] == true),
            "{records:#?}"
        );
        assert!(file["totals"]["aborted"].is_null());
        for r in records {
            let headers = r["request"]["headers"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            assert!(headers.keys().all(|k| !record::is_auth_header(k)));
        }
    }

    // Gesendete Formen (genau wie adapters/anthropic/reasoning.rs bzw. die Alternativen).
    let a: Vec<Value> = (0..7)
        .map(|i| sent_body(&server, "/anthropic", i))
        .collect();
    assert!(a[0].get("thinking").is_none() && a[0]["output_config"].get("effort").is_none());
    assert_eq!(a[1]["thinking"], serde_json::json!({"type": "disabled"}));
    assert!(a[1]["output_config"].get("effort").is_none());
    assert_eq!(a[2]["thinking"], serde_json::json!({"type": "adaptive"}));
    assert_eq!(a[2]["output_config"]["effort"], "low");
    assert_eq!(a[6]["model"], NON_DISABLEABLE);
    let o: Vec<Value> = (0..7).map(|i| sent_body(&server, "/openai", i)).collect();
    assert!(o[..5].iter().all(|b| b["store"] == false));
    assert_eq!(o[1]["reasoning"]["effort"], "none");
    assert!(o[5].get("store").is_none() && o[5].get("reasoning").is_none());
    assert_eq!(o[6]["store"], false);
    let r = sent_body(&server, "/openrouter", 1);
    assert_eq!(r["reasoning"]["effort"], "none");
    assert_eq!(r["provider"]["require_parameters"], true);

    // Beobachtungen.
    let fa = read_json(&dir, "anthropic.json");
    assert_eq!(
        record_of(&fa, "anthropic.off")["observation"]["reasoning_tokens"],
        0
    );
    assert_eq!(
        record_of(&fa, "anthropic.low")["observation"]["reasoning_tokens"],
        12
    );
    assert_eq!(
        record_of(&fa, "anthropic.off")["observation"]["answer_is_json"],
        true
    );
    let nd = record_of(&fa, "anthropic.off_not_disableable");
    assert_eq!(nd["status"], 400);
    assert_eq!(nd["observation"]["error_type"], "invalid_request_error");
    assert_eq!(nd["observation"]["answer_present"], false);
    assert_eq!(fa["facts"]["primary_listed"], true);
    let fo = read_json(&dir, "openai.json");
    let def = record_of(&fo, "openai.default");
    assert_eq!(def["observation"]["store"], false);
    assert_eq!(def["observation"]["prompt_cache_retention"], "24h");
    assert_eq!(
        record_of(&fo, "openai.off")["observation"]["reasoning_tokens"],
        0
    );
    let inv = record_of(&fo, "openai.invalid_effort");
    assert_eq!(inv["status"], 400);
    assert_eq!(inv["observation"]["error_code"], "unsupported_value");
    assert_eq!(inv["cost_basis"], "worst_case");
    assert!(record_of(&fo, "openai.decisions_store_false")["observation"]["store"].is_null());
    let fr = read_json(&dir, "openrouter.json");
    assert_eq!(
        record_of(&fr, "openrouter.off")["observation"]["reasoning_tokens"],
        0
    );
    assert_eq!(
        record_of(&fr, "openrouter.default")["observation"]["reasoning_tokens"],
        25
    );
    assert_eq!(fr["facts"]["primary_reasoning"]["mandatory"], false);

    // Schlüssel und Kennungen erscheinen nirgends (Server gibt sie in Headern und Körpern zurück).
    let files = all_files_text(&dir);
    for haystack in [&stdout, &files] {
        assert!(!haystack.contains(TEST_KEY));
        assert!(!haystack.contains("QRST"));
    }
    for needle in [
        "SECRETACCOUNT",
        "SECRETORG",
        "acmecorp",
        "SIGBLOB",
        "ENCBLOB",
        "req_test",
        "xreq_test",
        "req_body_test",
        "msg_test",
        "resp_test",
        "gen-test",
    ] {
        assert!(!files.contains(needle), "{needle}");
    }
    assert!(files.contains("[REDACTED]"));
    let text = record_of(&fa, "anthropic.default");
    assert_eq!(text["response_headers"]["x-echo-key"], VALUE_NOT_RECORDED);
    assert_eq!(
        text["response_headers"]["anthropic-ratelimit-requests-remaining"],
        "49"
    );
    let summary_file = read_json(&dir, "summary.json");
    assert_eq!(summary_file["total_requests"], planned);
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-013 (Kostenbremse der Probe)
#[tokio::test]
async fn cost_brake_stops_a_provider_after_an_expensive_answer() {
    let mut behavior = Behavior::new(TEST_KEY);
    behavior.input_tokens = 1_000_000;
    let server = Server::start(behavior).await;
    let dir = temp_dir("brake");
    let options = Options {
        only: Some(Provider::Anthropic),
        ..live_options()
    };
    let (summary, stdout) = run(&server, options, &TestKeys::all(), &dir, None).await;
    assert_eq!(server.requests().len(), 2, "Liste + erste Schema-Anfrage");
    assert_eq!(summary.requests, 2);
    let file = read_json(&dir, "anthropic.json");
    assert!(file["totals"]["aborted"]
        .as_str()
        .unwrap()
        .contains("Kostenbremse"));
    assert!(stdout.contains("Abbruch"));
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-013 (Gesamtgrenze nur nach unten)
#[tokio::test]
async fn lowered_total_cap_allows_only_free_requests() {
    let server = Server::start(Behavior::new(TEST_KEY)).await;
    let dir = temp_dir("total");
    let options = Options {
        max_total_usd: 0.000_001,
        ..live_options()
    };
    let (summary, _) = run(&server, options, &TestKeys::all(), &dir, None).await;
    assert!(server.requests().iter().all(|r| r.method == "GET"));
    assert_eq!(summary.cost_usd, 0.0);
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-017 (Probe: kein automatischer Retry)
#[tokio::test]
async fn errors_are_recorded_without_retry() {
    for status in [429u16, 500] {
        let mut behavior = Behavior::new(TEST_KEY);
        behavior.fail_posts_with = Some(status);
        let server = Server::start(behavior).await;
        let dir = temp_dir(&format!("retry-{status}"));
        let (summary, _) = run(&server, live_options(), &TestKeys::all(), &dir, None).await;
        assert_eq!(
            server.requests().len(),
            plan::STEPS.len(),
            "Status {status}"
        );
        assert_eq!(summary.requests, plan::STEPS.len());
        let fa = read_json(&dir, "anthropic.json");
        assert_eq!(record_of(&fa, "anthropic.off")["status"], status);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// spec: AI-009 (fehlender Schlüssel → Anbieter übersprungen; OpenRouter ist optional)
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
    let file = read_json(&dir, "openrouter.json");
    assert_eq!(file["key"]["found"], false);
    assert!(stdout.contains("[openrouter] übersprungen: kein Schlüssel"));
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: AI-002, AI-005 (nur Listen)
#[tokio::test]
async fn models_only_sends_only_free_requests() {
    let server = Server::start(Behavior::new(TEST_KEY)).await;
    let dir = temp_dir("models");
    let options = Options {
        models_only: true,
        ..live_options()
    };
    let (summary, _) = run(&server, options, &TestKeys::all(), &dir, None).await;
    assert_eq!(server.requests().len(), 3);
    assert!(server.requests().iter().all(|r| r.method == "GET"));
    assert_eq!(summary.cost_usd, 0.0);
    let fa = read_json(&dir, "anthropic.json");
    let caps = fa["facts"]["thinking_caps"].as_array().unwrap();
    assert_eq!(caps.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

// spec: V-A6, V-A8 (Plan); Trockenlauf ohne Netzwerk und Schlüsselbund
#[test]
fn dry_run_prints_the_full_plan_within_the_caps() {
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
        "https://api.openai.com/v1/responses",
        "https://api.openai.com/v1/decisions",
        "https://openrouter.ai/api/v1/chat/completions",
        "claude-haiku-5-5",
        "gpt-6-luna",
        "openai/gpt-6-luna",
    ] {
        assert!(text.contains(needle), "{needle}");
    }
    let lines = plan_lines(&options, &Endpoints::default());
    let total: f64 = lines.iter().map(|l| l.cost_cap_usd).sum();
    assert!(total <= plan::TOTAL_CAP_USD, "gesamt {total}");
    for provider in Provider::ALL {
        let mine: Vec<_> = lines.iter().filter(|l| l.provider == provider).collect();
        assert!(mine.len() <= provider.max_requests());
        let sum: f64 = mine.iter().map(|l| l.cost_cap_usd).sum();
        // Der volle Plan passt unter die Anbietergrenze: die Bremse schneidet keinen Schritt ab.
        assert!(sum <= plan::PROVIDER_CAP_USD, "{provider:?} {sum}");
    }
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
    assert_eq!(
        parse_args(&args(&[]), Some("1")).unwrap().mode,
        Mode::DryRun { note: None }
    );
    assert!(parse_args(&args(&["--max-total-usd", "1.5"]), None).is_err());
    assert_eq!(
        parse_args(&args(&["--max-total-usd", "0.5"]), None)
            .unwrap()
            .max_total_usd,
        0.5
    );
    assert!(parse_args(&args(&["--model-anthropic", "x"]), None).is_err());
    assert!(parse_args(&args(&["--base-url", "x"]), None).is_err());
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
}

// spec: AI-009; summary.json aus getrennten --only-Läufen
#[tokio::test]
async fn separate_only_runs_are_composed_into_one_summary() {
    let server = Server::start(Behavior::new(TEST_KEY)).await;
    let dir = temp_dir("compose");
    for provider in [Provider::Anthropic, Provider::OpenAi] {
        let options = Options {
            only: Some(provider),
            ..live_options()
        };
        run(&server, options, &TestKeys::all(), &dir, None).await;
    }
    let summary = read_json(&dir, "summary.json");
    assert_eq!(summary["providers"].as_object().unwrap().len(), 2);
    let expected = plan::steps(Provider::Anthropic).count() + plan::steps(Provider::OpenAi).count();
    assert_eq!(summary["total_requests"], expected);
    let _ = std::fs::remove_dir_all(&dir);
}
