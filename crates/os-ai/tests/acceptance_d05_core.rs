//! Unabhängige Abnahme M4-D05 (os-ai): Laufrahmen `os_ai::run` von außen über `run_decisions_blocking`.
//!
//! Erwartungen ausschließlich aus 08 AI-020 (Statustabelle mit Nachtrag 26.11.2026 `Skipped (input unavailable)`;
//! „Kontingentsperre je KI-Lauf“, Nachtrag 26.11.2026, Auflage E8/K4-29; Nachtrag K4-13 „Pause und Fortsetzen“,
//! „Gestoppt (endgültig)“, Besitz in jedem Ende-Pfad; Nachtrag 27.11.2026 `confirmed_at`) und AI-012. Der Code wurde nur
//! gelesen, um die öffentliche Schnittstelle aufzurufen. Gegen ai-mock, Testschlüssel nur im Arbeitsspeicher.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

#[path = "../../os-store/tests/acceptance_k11_support/mod.rs"]
mod k11;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use os_ai::decisions::exec::{RunEnd, Selection};
use os_ai::keys::AiKeyStore;
use os_ai::policy::{ConsentGate, ConsentKey, FieldSet};
use os_ai::run::{
    controls, run_decisions_blocking, DecisionsJob, FileConsentStore, JobConfirmation, JobError,
    JobOutcome, RunEnv,
};
use os_fetch::secrets::MemorySecretStore;
use os_model::ai::hook::{InputSource, PageAiFields};
use os_model::ai::AiStatus;
use os_model::budget::MicroUsd;
use os_model::config::{AiAppSettings, ProviderEndpoint};
use os_model::{CrawlConfig, CrawlState, SystemClock};
use os_store::{CrawlFile, OpenMode};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use proptest::prelude::*;
use serde_json::{json, Value};

const TEST_KEY: &str = "sk-TEST-D05-core-0000000000";
const QUESTION: &str = "d05-core";

// ---------------------------------------------------------------------------------------------------------------
// (i) Statuswerte

/// Tabelle aus 08 AI-020 samt Nachträgen 24.11. und 26.11.2026.
const TABLE: [&str; 25] = [
    "Success",
    "Success (cached)",
    "Refusal",
    "Pending",
    "Pending (batch)",
    "Pending (confirmation)",
    "Pending (consent)",
    "Skipped (scope)",
    "Skipped (segment)",
    "Skipped (empty input)",
    "Skipped (policy)",
    "Skipped (no capable model)",
    "Skipped (image too large)",
    "Skipped (unsupported format)",
    "Skipped (no declared language)",
    "Skipped (price unknown)",
    "Skipped (input unavailable)",
    "Budget exceeded",
    "Quota exceeded",
    "Error: Missing API key",
    "Error: Invalid output",
    "Error: Missing answer",
    "Error: Batch expired",
    "Error: Connection failed",
    "Error: Output limit reached",
];

fn in_table(s: &str) -> bool {
    if TABLE.contains(&s) {
        return true;
    }
    if let Some(n) = s
        .strip_prefix("Error: Timeout nach ")
        .and_then(|r| r.strip_suffix(" s"))
    {
        return n.parse::<u64>().is_ok();
    }
    s.strip_prefix("Error: ")
        .and_then(|r| r.split_once(' ').map(|(c, _)| c).or(Some(r)))
        .is_some_and(|c| c.len() == 3 && c.parse::<u16>().is_ok())
}

fn unit_statuses() -> Vec<AiStatus> {
    vec![
        AiStatus::Success,
        AiStatus::SuccessCached,
        AiStatus::Refusal,
        AiStatus::Pending,
        AiStatus::PendingBatch,
        AiStatus::PendingConfirmation,
        AiStatus::PendingConsent,
        AiStatus::SkippedScope,
        AiStatus::SkippedSegment,
        AiStatus::SkippedEmptyInput,
        AiStatus::SkippedPolicy,
        AiStatus::SkippedNoCapableModel,
        AiStatus::SkippedImageTooLarge,
        AiStatus::SkippedUnsupportedFormat,
        AiStatus::SkippedNoDeclaredLanguage,
        AiStatus::SkippedInputUnavailable,
        AiStatus::BudgetExceeded,
        AiStatus::QuotaExceeded,
        AiStatus::MissingApiKey,
        AiStatus::InvalidOutput,
        AiStatus::MissingAnswer,
        AiStatus::BatchExpired,
        AiStatus::ConnectionFailed,
        AiStatus::OutputLimitReached,
    ]
}

fn any_status() -> impl Strategy<Value = AiStatus> {
    prop_oneof![
        proptest::sample::select(unit_statuses()),
        (1u64..=600).prop_map(|seconds| AiStatus::Timeout { seconds }),
        (400u16..=599, "[A-Za-z][A-Za-z ]{0,24}")
            .prop_map(|(code, message)| AiStatus::HttpError { code, message }),
    ]
}

proptest! {
    // spec: AI-020 [M4]
    // acceptance: independent
    // Kriterium (i): „Jede Statusspalte … enthält nur Werte aus der Tabelle (Property-Test …)“ – der neue Wert eingeschlossen.
    #[test]
    fn i_every_status_value_is_from_the_table(status in any_status()) {
        let text = status.to_string();
        prop_assert!(in_table(&text), "{text}");
        // Kanonischer Text = Exportwert (serde) für die festen Werte.
        if !matches!(status, AiStatus::Timeout { .. } | AiStatus::HttpError { .. }) {
            prop_assert_eq!(serde_json::to_value(&status).unwrap(), json!(text));
            let back: AiStatus = serde_json::from_value(json!(text)).unwrap();
            prop_assert_eq!(back, status);
        }
    }
}

// spec: AI-020 [M4]
// acceptance: independent
// Nachtrag 26.11.2026: `Skipped (input unavailable)` ist additiv, zählt zu den übersprungenen Einträgen, nicht zum Filter
// „Errors“ („`Skipped (…)` und `Refusal` zählen nicht dazu“).
#[test]
fn i_skipped_input_unavailable_is_a_skipped_value_not_an_error() {
    let s = AiStatus::SkippedInputUnavailable;
    assert_eq!(s.to_string(), "Skipped (input unavailable)");
    assert_eq!(
        serde_json::to_value(&s).unwrap(),
        json!("Skipped (input unavailable)")
    );
    assert!(!s.is_error());
    for u in unit_statuses() {
        let t = u.to_string();
        if t.starts_with("Skipped (") || t == "Refusal" {
            assert!(!u.is_error(), "{t} zählt nicht zu „Errors“");
        }
        if t.starts_with("Error: ") || t == "Quota exceeded" || t == "Budget exceeded" {
            assert!(u.is_error(), "{t} zählt zu „Errors“");
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Laufrahmen gegen ai-mock

struct Mock {
    mock: AiMock,
    server: MockServer,
    client: reqwest::Client,
}

impl Mock {
    async fn start() -> Mock {
        let mock = AiMock::fixtures().unwrap();
        let server = MockServer::start(mock.registrations()).await.unwrap();
        Mock {
            mock,
            server,
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
        }
    }

    async fn control(&self, path: &str, body: Value) -> Value {
        let url = format!("{}{path}", self.server.base_url("__mock"));
        let resp = self.client.post(&url).json(&body).send().await.unwrap();
        assert!(resp.status().is_success(), "{url}: {}", resp.status());
        resp.json().await.unwrap_or(Value::Null)
    }

    async fn hold(&self, count: usize) {
        self.control(
            "/scenario",
            json!({"provider": "openai", "scenario": "hold", "count": count}),
        )
        .await;
    }

    async fn release(&self) {
        self.control("/release", json!({"provider": "openai"}))
            .await;
    }

    fn decisions(&self) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.method == "POST" && r.path.ends_with("/v1/decisions"))
            .collect()
    }

    /// Decisions-Requests, deren Body den Host `host` nennt.
    fn sent_for(&self, host: &str) -> usize {
        self.decisions()
            .iter()
            .filter(|r| r.body.contains(host))
            .count()
    }

    async fn wait_held(&self, n: usize) {
        let start = Instant::now();
        while self.decisions().iter().filter(|r| r.held).count() < n {
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "kein gehaltener Request"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

fn settings(mock: &Mock, threshold: u64) -> AiAppSettings {
    let mut s = AiAppSettings {
        preview_enabled: true,
        confirmation_threshold: MicroUsd(threshold),
        ..AiAppSettings::default()
    };
    s.provider_endpoints.insert(
        "openai".into(),
        ProviderEndpoint {
            base_url: Some(format!("{}/v1", mock.server.base_url("openai"))),
            concurrent_requests: 1,
            ..ProviderEndpoint::default()
        },
    );
    s
}

fn config() -> CrawlConfig {
    let mut c = CrawlConfig::default();
    c.ai.enabled = true;
    c.ai.questions = vec![json!({
        "id": QUESTION,
        "version": "1.0.0",
        "name": "D05 Kern",
        "type": "predicate",
        "instructions": "Abnahmefrage D05: trifft die Aussage auf die Seite zu?",
        "subject": "page",
        "inputs": ["url", "page_text"],
    })];
    c.ai.active_questions = vec![QUESTION.into()];
    c
}

/// Umgebung eines KI-Laufs (eigene Einwilligung über alle Felder, Testschlüssel nur im Arbeitsspeicher).
fn env(mock: &Mock, dir: &Path, threshold: u64) -> RunEnv {
    let secrets = Arc::new(MemorySecretStore::new());
    secrets.set("ai.openai", TEST_KEY);
    let consent =
        ConsentGate::with_store(Arc::new(FileConsentStore::new(dir.join("ai-consent.json"))))
            .unwrap();
    let fields = FieldSet::from_ids([
        "url",
        "title",
        "meta_description",
        "h1",
        "h2",
        "page_text",
        "word_count",
        "html_lang",
        "hreflang_self",
        "structured_data_types",
    ])
    .unwrap();
    consent
        .grant(ConsentKey::new("openai", "decision", fields), 1, None)
        .unwrap();
    RunEnv::from_settings(
        &settings(mock, threshold),
        &config(),
        &AiKeyStore::new(secrets),
        consent,
        Arc::new(SystemClock),
        "app",
    )
    .expect("RunEnv")
}

/// Abgeschlossener Crawl mit `n` HTML-Seiten (Status 200, indexierbar) unter `https://<host>/p<i>`.
fn crawl_file(
    dir: &Path,
    seed: u64,
    host: &str,
    n: i64,
    marker: &str,
) -> (PathBuf, Vec<PageAiFields>) {
    let (_, path) = k11::new_crawl(&dir.join(host), seed, CrawlState::Completed);
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let mut pages = Vec::new();
    let origin = format!("https://{host}");
    for i in 1..=n {
        let url = format!("https://{host}/p{i}");
        f.conn()
            .execute(
                "INSERT INTO urls(id, url, address, origin, is_internal, state, discovered_via, folder_depth, url_length, \
                 status_code, content_class, indexability) VALUES (?1, ?2, ?2, ?3, 1, 'done', 'link', 0, 30, 200, 'HTML', 'Indexable')",
                (i, url.as_str(), origin.as_str()),
            )
            .unwrap();
        pages.push(PageAiFields {
            url_id: i,
            url: url.clone(),
            content_class: "html".into(),
            status_code: Some(200),
            indexable: true,
            title: Some(format!("Seite {i}")),
            page_text: Some(format!("Inhalt der Seite {i} auf {host}. {marker}")),
            source: InputSource::Stored,
            ..PageAiFields::default()
        });
    }
    (path, pages)
}

fn job(path: &Path, pages: &[PageAiFields], confirmation: JobConfirmation) -> DecisionsJob {
    DecisionsJob {
        crawl_path: path.to_path_buf(),
        config: config(),
        questions: Vec::new(),
        pages: pages.to_vec(),
        confirmation,
        selection: Selection::Open,
    }
}

fn spawn_run(
    env: RunEnv,
    job: DecisionsJob,
) -> tokio::task::JoinHandle<Result<JobOutcome, JobError>> {
    tokio::task::spawn_blocking(move || run_decisions_blocking(env, job))
}

fn count(outcome: &JobOutcome, status: &str) -> u64 {
    outcome
        .statuses
        .get(QUESTION)
        .and_then(|m| m.get(status))
        .copied()
        .unwrap_or(0)
}

fn owner_pid(path: &Path) -> String {
    k11::get(path, "owner_pid").unwrap_or_default()
}

fn q_i64(path: &Path, sql: &str) -> i64 {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .conn()
        .query_row(sql, [], |r| r.get::<_, Option<i64>>(0))
        .unwrap()
        .unwrap_or(0)
}

// spec: AI-020 [M4]
// acceptance: independent
// Kriterium (n), Auflage E8 (K4-29): „Die Sperre eines Anbieters nach einer Kontingent-Antwort gilt je KI-Lauf. Weitere
// Einträge dieses Anbieters im selben Lauf erhalten `Quota exceeded` ohne Request …; ein neuer Lauf versucht es wieder.“
// Zwei gleichzeitige Läufe (B wartet auf einen gehaltenen Request, während A die Kontingent-Antwort erhält): A sendet genau
// einen Request, B sendet danach alle seine Einträge; ein danach gestarteter Lauf C versucht es wieder.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn n_quota_lock_is_per_ai_run() {
    let mock = Mock::start().await;
    let dir = k11::TempDir::new("d05-quota");
    let (a, a_pages) = crawl_file(
        dir.path(),
        51,
        "quota-a.test",
        5,
        "MOCK-SCENARIO:insufficient-quota",
    );
    let (b, b_pages) = crawl_file(dir.path(), 52, "quota-b.test", 5, "");
    let (c, c_pages) = crawl_file(
        dir.path(),
        53,
        "quota-c.test",
        5,
        "MOCK-SCENARIO:insufficient-quota",
    );

    mock.hold(1).await;
    let run_b = spawn_run(
        env(&mock, dir.path(), 1_000_000),
        job(&b, &b_pages, JobConfirmation::Confirmed),
    );
    mock.wait_held(1).await;
    assert_eq!(
        mock.sent_for("quota-b.test"),
        1,
        "B wartet auf seinen ersten Request"
    );

    let out_a = spawn_run(
        env(&mock, dir.path(), 1_000_000),
        job(&a, &a_pages, JobConfirmation::Confirmed),
    )
    .await
    .unwrap()
    .expect("Lauf A");
    assert_eq!(
        mock.sent_for("quota-a.test"),
        1,
        "A: ein Request, danach gesperrt"
    );
    assert_eq!(count(&out_a, "Quota exceeded"), 5, "{:?}", out_a.statuses);
    assert!(!run_b.is_finished(), "B läuft gleichzeitig");

    mock.release().await;
    let out_b = run_b.await.unwrap().expect("Lauf B");
    assert_eq!(mock.sent_for("quota-b.test"), 5, "B sendet weiter");
    assert_eq!(count(&out_b, "Success"), 5, "{:?}", out_b.statuses);

    let out_c = spawn_run(
        env(&mock, dir.path(), 1_000_000),
        job(&c, &c_pages, JobConfirmation::Confirmed),
    )
    .await
    .unwrap()
    .expect("Lauf C");
    assert_eq!(
        mock.sent_for("quota-c.test"),
        1,
        "neuer Lauf versucht es wieder"
    );
    assert_eq!(count(&out_c, "Quota exceeded"), 5);
}

#[derive(Debug, Clone, Copy)]
enum Action {
    Pause,
    Stop,
    Abort,
}

// spec: AI-020 [M4]
// acceptance: independent
// Kriterium (h) für die Pfade Pause, Stopp und Abbruch, mit `hold`: Pause und Stopp – kein neuer Request, der laufende wird
// abgewartet und übernommen, offene Einträge bleiben `Pending`; Abbruch – der laufende Request wird verworfen (Eintrag
// `Pending`, keine Protokollzeile). In jedem Fall trägt der Lauf vorher den Besitz und gibt ihn am Ende frei.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h_owner_is_released_when_a_run_is_paused_stopped_or_aborted() {
    for (i, action) in [Action::Pause, Action::Stop, Action::Abort]
        .into_iter()
        .enumerate()
    {
        let mock = Mock::start().await;
        let dir = k11::TempDir::new("d05-end");
        let host = format!("end-{i}.test");
        let (path, pages) = crawl_file(dir.path(), 60 + i as u64, &host, 5, "");
        mock.hold(1).await;
        let run = spawn_run(
            env(&mock, dir.path(), 1_000_000),
            job(&path, &pages, JobConfirmation::Confirmed),
        );
        mock.wait_held(1).await;
        assert_eq!(
            owner_pid(&path),
            std::process::id().to_string(),
            "{action:?}: Besitz"
        );
        match action {
            Action::Pause => controls().pause(&path),
            Action::Stop => controls().stop(&path),
            Action::Abort => controls().abort(&path),
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        if !matches!(action, Action::Abort) {
            mock.release().await;
        }
        let out = tokio::time::timeout(Duration::from_secs(30), run)
            .await
            .expect("Lauf endet")
            .unwrap()
            .expect("Ergebnis");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            mock.decisions().len(),
            1,
            "{action:?}: kein weiterer Request"
        );
        let expected_end = match action {
            Action::Pause => RunEnd::Paused,
            Action::Stop => RunEnd::Stopped,
            Action::Abort => RunEnd::Aborted,
        };
        assert_eq!(out.end, expected_end);
        let logged = q_i64(&path, "SELECT COUNT(*) FROM ai_requests");
        match action {
            Action::Abort => {
                assert_eq!(count(&out, "Success"), 0);
                assert_eq!(logged, 0, "abgebrochener Request ohne Protokollzeile");
            }
            _ => {
                assert_eq!(
                    count(&out, "Success"),
                    1,
                    "{action:?}: laufender Request übernommen"
                );
                assert_eq!(logged, 1);
            }
        }
        let open = q_i64(
            &path,
            &format!("SELECT COUNT(*) FROM ai_answers WHERE question_id = '{QUESTION}' AND status = 'Pending'"),
        );
        let answered = q_i64(
            &path,
            &format!("SELECT COUNT(*) FROM ai_answers WHERE question_id = '{QUESTION}' AND status <> 'Pending'"),
        );
        assert_eq!(
            answered + open,
            5,
            "{action:?}: übrige Einträge offen ({:?})",
            out.statuses
        );
        assert_eq!(owner_pid(&path), "", "{action:?}: owner_pid leer");
    }
}

// spec: AI-020 [M4]
// spec: AI-012 [M4]
// acceptance: independent
// Kriterium (l), Nachtrag 27.11.2026: Die erteilte Bestätigung steht in `ai_runs.confirmed_at`; Fortsetzen nach einer Pause
// – mit neuer Umgebung wie nach einem Neustart – setzt dieselbe Zeile fort und übernimmt die Bestätigung (Schwelle 1 µUSD:
// ohne sie bliebe alles `Pending (confirmation)`); ein neuer Lauf legt eine neue Zeile an, für die neu bestätigt wird.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l_resume_continues_the_same_run_row_with_its_confirmation() {
    let mock = Mock::start().await;
    let dir = k11::TempDir::new("d05-confirm");
    let (path, pages) = crawl_file(dir.path(), 70, "confirm.test", 5, "");
    mock.hold(1).await;
    let run = spawn_run(
        env(&mock, dir.path(), 1),
        job(&path, &pages, JobConfirmation::Confirmed),
    );
    mock.wait_held(1).await;
    controls().pause(&path);
    tokio::time::sleep(Duration::from_millis(200)).await;
    mock.release().await;
    let first = run.await.unwrap().expect("erster Lauf");
    assert_eq!(first.end, RunEnd::Paused);
    assert_eq!(q_i64(&path, "SELECT COUNT(*) FROM ai_runs"), 1);
    assert_eq!(
        q_i64(
            &path,
            "SELECT COUNT(*) FROM ai_runs WHERE confirmed_at IS NOT NULL AND ended_at IS NULL"
        ),
        1
    );
    assert_eq!(mock.decisions().len(), 1);

    let resumed = spawn_run(
        env(&mock, dir.path(), 1),
        job(&path, &pages, JobConfirmation::Resume),
    )
    .await
    .unwrap()
    .expect("Fortsetzen");
    assert_eq!(resumed.run_id, first.run_id, "dieselbe Laufzeile");
    assert_eq!(resumed.end, RunEnd::Completed, "kein neuer Voranschlag");
    assert_eq!(count(&resumed, "Success"), 5, "{:?}", resumed.statuses);
    assert_eq!(count(&resumed, "Pending (confirmation)"), 0);
    assert_eq!(mock.decisions().len(), 5, "genau die offenen Einträge");
    assert_eq!(q_i64(&path, "SELECT COUNT(*) FROM ai_runs"), 1);

    let unconfirmed = spawn_run(
        env(&mock, dir.path(), 1),
        job(&path, &pages, JobConfirmation::Unconfirmed),
    )
    .await
    .unwrap()
    .expect("neuer Lauf ohne Bestätigung");
    assert_ne!(unconfirmed.run_id, first.run_id, "neuer Lauf = neue Zeile");
    let confirmed = spawn_run(
        env(&mock, dir.path(), 1),
        job(&path, &pages, JobConfirmation::Confirmed),
    )
    .await
    .unwrap()
    .expect("neuer Lauf mit Bestätigung");
    assert_eq!(q_i64(&path, "SELECT COUNT(*) FROM ai_runs"), 3);
    assert_eq!(
        q_i64(
            &path,
            &format!(
                "SELECT COUNT(*) FROM ai_runs WHERE id = {} AND confirmed_at IS NULL",
                unconfirmed.run_id
            )
        ),
        1,
        "ohne Bestätigung leer"
    );
    assert_eq!(
        q_i64(
            &path,
            &format!(
                "SELECT COUNT(*) FROM ai_runs WHERE id = {} AND confirmed_at IS NOT NULL",
                confirmed.run_id
            )
        ),
        1,
        "neu bestätigt"
    );
}
