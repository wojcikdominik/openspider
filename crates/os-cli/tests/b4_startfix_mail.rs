//! B4-STARTFIX: Abschlussmail bei einem Crawl, der regulär als „Fehlgeschlagen“ endet (CRAWL-128, Start-URL nicht erreichbar),
//! für die Kommandozeile (globale Einstellung) und den Planer (Aufgabe, mit und ohne Auto-Vergleich). Der Lauf behält Exit 0
//! (OUT-182), die Mail gilt aber als Fehlschlag (OUT-281): keine Erfolgsmail, `notify_on_failure` entscheidet. Gegen den
//! SMTP-Mock der Testsite.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::Env;
use os_analyze::analysis::CancelToken;
use os_cli::exit::Exit;
use os_cli::reporter::Reporter;
use os_cli::run_task::{comparison_mail::ComparisonMailHooks, run_in, DefaultHooks, RunCtx};
use os_cli::run_task::{TaskHooks, TaskRun};
use os_cli::signals::Signals;
use os_fetch::MemorySecretStore;
use os_model::SystemClock;
use os_sched::mail::{Account, AccountStore, Auth, Tls};
use os_sched::tasks::{CrawlSpec, Execution, Notify, Output, Schedule, Source, Task, TaskStore};
use os_testsite::mocks::smtp::{Config, SmtpMock};

/// SMTP-Mock ohne Anmeldung: Das Konto braucht dann kein Kennwort im Schlüsselbund (auch nicht im Kindprozess der CLI).
async fn smtp() -> SmtpMock {
    SmtpMock::start(Config {
        username: String::new(),
        ..Config::default()
    })
    .await
    .unwrap()
}

fn save_account(settings: &std::path::Path, mock: &SmtpMock) {
    AccountStore::new(settings)
        .save(
            &Account {
                id: "fixture".into(),
                label: "Mock".into(),
                host: "localhost".into(),
                port: mock.addr.port(),
                tls: Tls::None,
                username: String::new(),
                from: "crawler@site.test".into(),
                auth: Auth::Password,
                allow_insecure: true,
                client_id: None,
            },
            None,
            &MemorySecretStore::new(),
        )
        .unwrap();
}

fn closed_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    format!(
        "http://127.0.0.1:{}/",
        listener.local_addr().unwrap().port()
    )
}

// ---------------------------------------------------------------------------------------------------
// Kommandozeile: globale Einstellung „E-Mail bei Crawl-Ende“

fn cli_env(mock: &SmtpMock, on_success: bool, on_failure: bool) -> Env {
    let env = Env::new();
    let mut settings = os_model::settings::AppSettings::load(env.home.path()).unwrap();
    settings.network.proxy.mode = os_model::ProxyMode::None;
    settings.accounts.notify_account = Some("fixture".into());
    settings.accounts.notify_to = vec!["recipient@site.test".into()];
    settings.accounts.notify_on_success = on_success;
    settings.accounts.notify_on_failure = on_failure;
    settings.save(env.home.path()).unwrap();
    save_account(env.home.path(), mock);
    env
}

fn crawl_args(url: &str) -> Vec<String> {
    ["--crawl", url, "--set", "robots.retries=0"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

// spec: OUT-281
// spec: CRAWL-128
// spec: OUT-182
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_an_unreachable_start_url_sends_the_failure_mail_with_exit_0() {
    let mock = smtp().await;
    let env = cli_env(&mock, false, true);
    let run = env.run_async(crawl_args(&closed_url())).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let messages = mock.messages();
    assert_eq!(messages.len(), 1, "{}", run.dump());
    assert!(
        messages[0].data.contains("[FAILED]"),
        "{}",
        messages[0].data
    );
    assert!(
        messages[0].data.contains("nicht erreichbar"),
        "{}",
        messages[0].data
    );
}

// Gegenprobe: Nur „bei Erfolg“ an – der fehlgeschlagene Lauf sendet keine Erfolgsmail. Ein per robots.txt gesperrter Start ist
// kein Fehlschlag (CRAWL-128) und erhält sie.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_an_unreachable_start_url_sends_no_success_mail() {
    let mock = smtp().await;
    let env = cli_env(&mock, true, false);
    let run = env.run_async(crawl_args(&closed_url())).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(mock.messages().is_empty(), "{:?}", mock.messages());

    let site = common::site("ui/fx-ui-robots-start").await;
    let env = cli_env(&mock, true, false);
    let run = env
        .run_async(crawl_args(&format!("{}/", site.base_url())))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let messages = mock.messages();
    assert_eq!(messages.len(), 1, "{}", run.dump());
    assert!(
        !messages[0].data.contains("[FAILED]"),
        "{}",
        messages[0].data
    );
}

// ---------------------------------------------------------------------------------------------------
// Planer: Aufgabe mit eigenen Empfängern

struct Planner {
    home: tempfile::TempDir,
    secrets: Arc<MemorySecretStore>,
}

impl Planner {
    fn new(mock: &SmtpMock) -> Planner {
        let planner = Planner {
            home: tempfile::tempdir().unwrap(),
            secrets: Arc::new(MemorySecretStore::new()),
        };
        // Die Aufgabenprüfung (OUT-168) verlangt den Schlüsselbund-Eintrag des Kontos; der Mock fragt keine Anmeldung ab.
        planner.secrets.set("mail/fixture", "unused");
        let mut settings = os_model::settings::AppSettings::default();
        settings.network.proxy.mode = os_model::ProxyMode::None;
        settings.save(&planner.ctx().paths.settings).unwrap();
        save_account(&planner.ctx().paths.settings, mock);
        planner
    }

    fn ctx(&self) -> RunCtx {
        RunCtx {
            paths: os_store::paths::app_paths_with_home(Some(self.home.path().as_os_str())),
            clock: Arc::new(SystemClock),
            secrets: self.secrets.clone(),
            run_id: None,
            manual: true,
            log_path: None,
            export_dir: self.home.path().join("exports"),
            json_progress: false,
            heartbeat: Duration::from_millis(200),
            poll: Duration::from_millis(50),
            timeout_scale: 1.0,
        }
    }

    async fn run(&self, task: Task, hooks: &dyn TaskHooks) -> TaskRun {
        TaskStore::in_settings(&self.ctx().paths.settings)
            .save(task.clone())
            .unwrap();
        run_in(
            &self.ctx(),
            &task.id,
            hooks,
            Signals::manual(CancelToken::new()),
            Reporter::for_task(false),
        )
        .await
    }

    fn comparison_hooks(&self, mock: &SmtpMock) -> ComparisonMailHooks {
        ComparisonMailHooks {
            crawls: self.ctx().paths.crawls,
            sender: Arc::new(
                os_sched::mail::completion::SmtpCompletionSender::loopback(
                    mock.addr.port(),
                    "sender@site.test",
                )
                .unwrap(),
            ),
        }
    }
}

fn task(url: &str, auto_compare: bool, on_success: bool, on_failure: bool) -> Task {
    Task {
        id: "nightly".into(),
        name: "Nightly".into(),
        project: "Shop".into(),
        description: String::new(),
        enabled: true,
        schedule: Schedule::Daily {
            time: "03:00".into(),
            from: None,
        },
        crawl: CrawlSpec {
            source: Source::Spider { url: url.into() },
            config: None,
            auth_profile: None,
            overrides: [("threads", "1"), ("robots.retries", "0")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        },
        execution: Execution {
            auto_compare,
            ..Execution::default()
        },
        apis: Vec::new(),
        output: Output::default(),
        exports: Vec::new(),
        export_options: None,
        preset: None,
        custom_summary: None,
        notify: Notify {
            on_success,
            on_failure,
            account: Some("fixture".into()),
            to: vec!["recipient@site.test".into()],
        },
    }
}

/// Der Lauf bleibt ein gültiges Ergebnis mit Warnung (OUT-182): Exit 0, Historie „Completed with warnings“.
fn assert_valid_result(run: &TaskRun) {
    assert_eq!(run.exit, Exit::Ok, "{}", run.summary);
    let record = run.record.as_ref().unwrap();
    assert_eq!(
        record.result,
        os_sched::history::RunResult::CompletedWithWarnings,
        "{record:?}"
    );
}

// Aufgabe ohne Auto-Vergleich (eingebaute Abschlussmail): „bei Erfolg“ aus, „bei Fehlschlag“ an.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_an_unreachable_start_url_sends_the_failure_mail() {
    let mock = smtp().await;
    let planner = Planner::new(&mock);
    let run = planner
        .run(task(&closed_url(), false, false, true), &DefaultHooks)
        .await;
    assert_valid_result(&run);
    let messages = mock.messages();
    assert_eq!(messages.len(), 1, "{}", run.summary);
    assert!(
        messages[0].data.contains("[FAILED]"),
        "{}",
        messages[0].data
    );
}

// Gegenprobe ohne Auto-Vergleich: nur „bei Erfolg“ an – keine Mail für den fehlgeschlagenen Crawl.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_an_unreachable_start_url_sends_no_success_mail() {
    let mock = smtp().await;
    let planner = Planner::new(&mock);
    let run = planner
        .run(task(&closed_url(), false, true, false), &DefaultHooks)
        .await;
    assert_valid_result(&run);
    assert!(mock.messages().is_empty(), "{:?}", mock.messages());
}

// Aufgabe mit Auto-Vergleich (Abschlussmail mit Änderungstabelle, OUT-152): Ein als `Failed` beendeter Crawl lässt sich nicht
// vergleichen; der Lauf endet deshalb selbst als „Failed“ und die Mail folgt `on_failure`, mit Betreff-Präfix `[FAILED]`.
// spec: OUT-281
// spec: OUT-152
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_with_auto_compare_treats_a_failed_crawl_as_failure() {
    let mock = smtp().await;
    let planner = Planner::new(&mock);
    let hooks = planner.comparison_hooks(&mock);
    let failed = |run: &TaskRun| {
        assert_eq!(
            run.record.as_ref().unwrap().result,
            os_sched::history::RunResult::Failed,
            "{}",
            run.summary
        );
    };
    let run = planner
        .run(task(&closed_url(), true, true, false), &hooks)
        .await;
    failed(&run);
    assert!(mock.messages().is_empty(), "{:?}", mock.messages());

    let run = planner
        .run(task(&closed_url(), true, false, true), &hooks)
        .await;
    failed(&run);
    let messages = mock.messages();
    assert_eq!(messages.len(), 1, "{}", run.summary);
    assert!(
        messages[0]
            .data
            .contains("Subject: [FAILED] Nightly: Failed"),
        "{}",
        messages[0].data
    );
    // Fehlermeldung des Laufs statt Änderungstabelle.
    assert!(
        messages[0].data.contains("Auto-Vergleich fehlgeschlagen"),
        "{}",
        messages[0].data
    );
}
