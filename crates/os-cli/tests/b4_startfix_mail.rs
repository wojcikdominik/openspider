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

/// Mail-Text ohne weiche Zeilenumbrüche der Quoted-Printable-Kodierung (`=` am Zeilenende), damit lange Zeilen am Stück prüfbar sind.
fn unfolded(data: &str) -> String {
    data.replace("=\r\n", "").replace("=\n", "")
}

/// Die Fehler-Mail nennt Fehlerklasse (in Worten und als Schlüssel) und betroffene Adresse (OUT-281, OS-Festlegung 09.11.2026).
fn assert_names_class(data: &str, address: &str) {
    let text = unfolded(data);
    assert!(
        text.contains("Verbindung abgelehnt (connection_refused)"),
        "{text}"
    );
    assert!(text.contains(address), "{text}");
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
    let url = closed_url();
    let run = env.run_async(crawl_args(&url)).await;
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
    // Standard „robots.txt beachten“: Ursache ist die nicht abrufbare robots.txt der Start-URL.
    assert_names_class(&messages[0].data, &format!("{url}robots.txt"));
}

// Exit 3 durch `--fail-on-start-error`: Die Fehler-Mail trägt trotzdem Start-URL, Projekt, Start und Ende (OS-Festlegung
// 09.11.2026) sowie Fehlerklasse und Adresse.
// spec: OUT-281
// spec: OUT-182
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_the_failure_mail_with_exit_3_carries_the_crawl_header() {
    let mock = smtp().await;
    let env = cli_env(&mock, false, true);
    let url = closed_url();
    let mut args = crawl_args(&url);
    args.extend(["--project-name", "Shop", "--fail-on-start-error"].map(String::from));
    let run = env.run_async(args).await;
    assert_eq!(run.code, 3, "{}", run.dump());
    let messages = mock.messages();
    assert_eq!(messages.len(), 1, "{}", run.dump());
    let text = unfolded(&messages[0].data);
    assert!(text.contains("[FAILED]"), "{text}");
    for line in [
        format!("Start-URL: {url}"),
        "Projekt: Shop".to_string(),
        "Start: 20".to_string(),
        "Ende: 20".to_string(),
    ] {
        assert!(text.contains(&line), "{line} fehlt: {text}");
    }
    assert_names_class(&messages[0].data, &format!("{url}robots.txt"));
}

// Globale Mail der Kommandozeile mit `--compare-with-previous`: Der fehlgeschlagene Crawl wird nicht verglichen, die Fehler-Mail
// trägt den Vermerk statt einer Änderungstabelle (OUT-152, OS-Festlegung 09.11.2026).
// spec: OUT-152
// spec: OUT-281
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_the_failure_mail_of_a_skipped_comparison_carries_the_note() {
    let mock = smtp().await;
    let env = cli_env(&mock, false, true);
    let mut args = crawl_args(&closed_url());
    args.extend(["--project-name", "Shop", "--compare-with-previous"].map(String::from));
    let run = env.run_async(args).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let messages = mock.messages();
    assert_eq!(messages.len(), 1, "{}", run.dump());
    // „ü“ steht Quoted-Printable-kodiert in der Mail; geprüft wird der Rest des Vermerks.
    let text = unfolded(&messages[0].data);
    assert!(
        text.contains("bersprungen (Crawl fehlgeschlagen)"),
        "{text}"
    );
    assert!(!text.contains("Change %"), "{text}");
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

/// Den Fehlschlag stuft der Crawl ein (OS-Festlegung 09.11.2026): Historie „Failed“ mit der Fehlermeldung des Crawls, mit und
/// ohne Auto-Vergleich, nie „Crawl erfolgreich …“. Der Exit-Code bleibt 0 (OUT-182).
fn assert_failed_crawl(run: &TaskRun) {
    assert_eq!(run.exit, Exit::Ok, "{}", run.summary);
    let record = run.record.as_ref().unwrap();
    assert_eq!(
        record.result,
        os_sched::history::RunResult::Failed,
        "{record:?}"
    );
    let message = record.message.as_deref().unwrap_or_default();
    assert!(message.contains("nicht erreichbar"), "{record:?}");
    assert!(!message.contains("erfolgreich"), "{record:?}");
}

// Aufgabe ohne Auto-Vergleich (eingebaute Abschlussmail): „bei Erfolg“ aus, „bei Fehlschlag“ an.
// spec: OUT-281
// spec: OUT-169
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_an_unreachable_start_url_sends_the_failure_mail() {
    let mock = smtp().await;
    let planner = Planner::new(&mock);
    let run = planner
        .run(task(&closed_url(), false, false, true), &DefaultHooks)
        .await;
    assert_failed_crawl(&run);
    let messages = mock.messages();
    assert_eq!(messages.len(), 1, "{}", run.summary);
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
    assert!(unfolded(&messages[0].data).contains("Verbindung abgelehnt (connection_refused)"));
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
    assert_failed_crawl(&run);
    assert!(mock.messages().is_empty(), "{:?}", mock.messages());
}

// Aufgabe mit Auto-Vergleich (Abschlussmail mit Änderungstabelle, OUT-152): Der fehlgeschlagene Crawl wird nicht verglichen, der
// Vergleich ist als „übersprungen (Crawl fehlgeschlagen)“ vermerkt; die Mail folgt `on_failure`, Betreff mit `[FAILED]`.
// spec: OUT-281
// spec: OUT-152
// spec: OUT-169
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_with_auto_compare_treats_a_failed_crawl_as_failure() {
    let mock = smtp().await;
    let planner = Planner::new(&mock);
    let hooks = planner.comparison_hooks(&mock);
    let run = planner
        .run(task(&closed_url(), true, true, false), &hooks)
        .await;
    assert_failed_crawl(&run);
    let record = run.record.as_ref().unwrap();
    assert!(
        record
            .warnings
            .iter()
            .any(|w| w == "Auto-Vergleich übersprungen (Crawl fehlgeschlagen)"),
        "{record:?}"
    );
    assert!(
        !record
            .warnings
            .iter()
            .any(|w| w.contains("auto comparison")),
        "{record:?}"
    );
    assert!(mock.messages().is_empty(), "{:?}", mock.messages());

    let run = planner
        .run(task(&closed_url(), true, false, true), &hooks)
        .await;
    assert_failed_crawl(&run);
    let messages = mock.messages();
    assert_eq!(messages.len(), 1, "{}", run.summary);
    let data = &messages[0].data;
    assert!(data.contains("Subject: [FAILED] Nightly: Failed"), "{data}");
    // Fehlermeldung des Crawls und Vermerk statt Änderungstabelle (Umlaut als Quoted-Printable, daher ohne „ü“ geprüft).
    assert!(data.contains("nicht erreichbar"), "{data}");
    assert!(data.contains("(Crawl fehlgeschlagen)"), "{data}");
    assert!(!data.contains("erfolgreich"), "{data}");
}

// ---------------------------------------------------------------------------------------------------
// Kommandozeile: Sperre per robots.txt (CRAWL-128, OS-Festlegung 09.11.2026)

// Eine per robots.txt gesperrte Start-URL ist kein Fehlschlag: auch mit `--fail-on-start-error` Exit 0, Warnung mit robots.txt-URL
// und sperrender Zeile. Eine nicht erreichbare Start-URL endet dagegen weiter mit Exit 3 (OUT-182).
// spec: OUT-182
// spec: OUT-204
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_a_robots_block_is_no_start_error() {
    let site = common::site("ui/fx-ui-robots-start").await;
    let env = Env::new();
    let mut args = crawl_args(&format!("{}/", site.base_url()));
    args.push("--fail-on-start-error".into());
    let run = env.run_async(args).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let expected = format!(
        "Start-URL {}/ durch robots.txt gesperrt: {}/robots.txt, Zeile 2: Disallow: /",
        site.base_url(),
        site.base_url()
    );
    assert!(run.stderr.contains(&expected), "{}", run.dump());
    assert!(!run.stderr.contains("nicht erreichbar"), "{}", run.dump());

    let env = Env::new();
    let mut args = crawl_args(&closed_url());
    args.push("--fail-on-start-error".into());
    let run = env.run_async(args).await;
    assert_eq!(run.code, 3, "{}", run.dump());
}

// Die Schlusszeile eines fehlgeschlagenen Crawls nennt dessen Fehler, nicht „Fertig“ (OS-Festlegung 09.11.2026).
// spec: OUT-182
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_the_final_line_of_a_failed_crawl_names_its_error() {
    let env = Env::new();
    let run = env.run_async(crawl_args(&closed_url())).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(
        run.last_line().contains("nicht erreichbar"),
        "{}",
        run.dump()
    );
    assert!(!run.last_line().starts_with("Fertig"), "{}", run.dump());
}

// robots.txt antwortet mit 503: Die Regel „robots.txt nicht erreichbar“ sperrt die Start-URL, das ist kein Fehlschlag (Exit 0
// auch mit `--fail-on-start-error`). Die Warnung nennt robots.txt-URL und Grund, den HTTP-Status (OS-Festlegung 09.11.2026).
// spec: OUT-182
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_the_warning_for_a_robots_txt_answering_503_names_url_and_status() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("scenario.toml"),
        "[[route]]\npath = \"/robots.txt\"\nstatus = 503\nbody = \"x\"\n\n[[route]]\npath = \"/\"\nbody = \"x\"\n",
    )
    .unwrap();
    let site = os_testsite::TestSite::start_dir(dir.path(), 0)
        .await
        .unwrap();
    let env = Env::new();
    let mut args = crawl_args(&format!("{}/", site.base_url()));
    args.push("--fail-on-start-error".into());
    let run = env.run_async(args).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let expected = format!(
        "durch robots.txt gesperrt: robots.txt {}/robots.txt nicht abrufbar (Grund: HTTP 503)",
        site.base_url()
    );
    assert!(run.stderr.contains(&expected), "{}", run.dump());
}
