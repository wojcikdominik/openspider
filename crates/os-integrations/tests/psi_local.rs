//! M2-I04: Lokaler Lighthouse-Lauf (INTEG-132). Der Läufer wird gegen einen Ersatzprozess geprüft: das Testbinary
//! selbst startet mit dem Test `fake_lighthouse_entry` und verhält sich je nach Umgebungsvariable wie Lighthouse
//! (Bericht schreiben, hängen, abstürzen, Version ausgeben). Ein Lauf gegen eine echte Lighthouse-Installation ist bis
//! zur Bündelungsentscheidung (Klärung K2-09) zurückgestellt.
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_fetch::MemorySecretStore;
use os_integrations::psi::{
    lighthouse::{parse_lighthouse, parse_psi, Cell},
    local::{
        basic_auth_header, config_headers, max_parallel, parse_version, Job, Launcher,
        LocalOptions, LocalRunner, NodeLighthouse, RUNTIME_MISSING,
    },
    PsiSource,
};
use os_integrations::{ApiCtx, ApiError, ApiSource, UrlRef};
use os_model::{config::HeaderEntry, CrawlConfig, PsiDevice};

const MODE: &str = "OS_FAKE_LH_MODE";
const OUT: &str = "OS_FAKE_LH_OUT";
const URL_VAR: &str = "OS_FAKE_LH_URL";

/// Ersatz-Lighthouse; ohne Umgebungsvariable ein leerer Test.
#[test]
fn fake_lighthouse_entry() {
    let Ok(mode) = std::env::var(MODE) else {
        return;
    };
    let url = std::env::var(URL_VAR).unwrap_or_default();
    let out = std::env::var(OUT).map(PathBuf::from);
    let report = |lhr: serde_json::Value| {
        std::fs::write(out.clone().unwrap(), lhr.to_string()).unwrap();
    };
    match mode.as_str() {
        "version" => println!("13.0.0"),
        "hang" => std::thread::sleep(Duration::from_secs(120)),
        "crash" => std::process::exit(70),
        "slow" => {
            std::thread::sleep(Duration::from_millis(400));
            report(os_testsite::mocks::psi::lighthouse_fast(&url, false));
        }
        "errored" => {
            let mut lhr = os_testsite::mocks::psi::lighthouse_fast(&url, false);
            lhr["runtimeError"] = serde_json::json!({
                "code": "ERRORED_DOCUMENT_REQUEST",
                "message": "The page could not be loaded (status code 401)."
            });
            report(lhr);
        }
        _ => report(os_testsite::mocks::psi::lighthouse_heavy(
            &url,
            mode == "desktop",
        )),
    }
}

/// Startet das Testbinary als Lighthouse; der Modus hängt am letzten Pfadabschnitt der URL.
#[derive(Default)]
struct Fake {
    headers: Mutex<Vec<serde_json::Value>>,
}

impl Launcher for Fake {
    fn command(&self, job: &Job) -> Command {
        if let Some(file) = &job.headers_file {
            let raw = std::fs::read_to_string(file).unwrap();
            self.headers
                .lock()
                .unwrap()
                .push(serde_json::from_str(&raw).unwrap());
        }
        let mode = job.url.rsplit('/').next().unwrap_or("heavy").to_string();
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "fake_lighthouse_entry",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(MODE, mode)
        .env(OUT, &job.output)
        .env(URL_VAR, &job.url);
        cmd
    }

    fn version_command(&self) -> Command {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "fake_lighthouse_entry",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(MODE, "version");
        cmd
    }
}

fn runner(parallel: u32, timeout: Duration) -> (Arc<Fake>, Arc<LocalRunner>) {
    let fake = Arc::new(Fake::default());
    let opts = LocalOptions {
        parallel,
        timeout,
        ..LocalOptions::default()
    };
    (fake.clone(), Arc::new(LocalRunner::new(fake, &opts)))
}

fn local_ctx() -> ApiCtx {
    let mut cfg = CrawlConfig::default();
    cfg.api.psi.enabled = true;
    cfg.api.psi.local = true;
    ApiCtx {
        cfg,
        secrets: Arc::new(MemorySecretStore::new()),
        network: Default::default(),
        allow_api_override: false,
        quota_ledger: None,
    }
}

fn text(out: &os_integrations::psi::lighthouse::Outcome, name: &str) -> String {
    match out.get(name) {
        Some(Cell::Text(t)) => t.clone(),
        other => panic!("{name}: {other:?}"),
    }
}

// spec: INTEG-132
#[tokio::test]
async fn without_runtime_local_cannot_connect() {
    let err = NodeLighthouse::discover(&LocalOptions::default()).unwrap_err();
    assert_eq!(err, ApiError::NotConnected(RUNTIME_MISSING.into()));
    assert_eq!(
        RUNTIME_MISSING,
        "Fehler: Lighthouse-Laufzeit nicht gefunden"
    );
    let missing = LocalOptions {
        lighthouse: Some(PathBuf::from("does-not-exist").join("cli").join("index.js")),
        ..LocalOptions::default()
    };
    assert!(LocalRunner::discover(&missing).is_err());
    // Die Quelle meldet denselben Status beim Verbinden und beim Abruf.
    let ctx = local_ctx();
    let source = PsiSource::new();
    let cred = os_integrations::credentials::Credential {
        account_id: "local".into(),
        label: String::new(),
        api_key: None,
        access_token: None,
        refresh_token: None,
        expires_at: None,
        client_secret: None,
        scopes: vec![],
    };
    assert_eq!(
        source.test_connection(&ctx, &cred).await.unwrap_err(),
        ApiError::NotConnected(RUNTIME_MISSING.into())
    );
    let page = UrlRef {
        url_id: None,
        url: "https://site.test/".into(),
    };
    assert!(source.fetch(&[page], &ctx).await.is_err());
}

// spec: INTEG-132
#[test]
fn dialog_allows_at_most_half_the_cores_and_validates_the_timeout() {
    assert_eq!(max_parallel(4), 2);
    assert_eq!(max_parallel(1), 1);
    assert_eq!(max_parallel(16), 8);
    let mut opts = LocalOptions::default();
    assert_eq!((opts.parallel, opts.timeout), (1, Duration::from_secs(90)));
    assert!(opts.validate(4).is_ok());
    opts.parallel = 3;
    assert!(opts.validate(4).is_err());
    opts.parallel = 2;
    opts.timeout = Duration::from_secs(20);
    assert!(opts.validate(4).is_err());
    opts.timeout = Duration::from_secs(600);
    assert!(opts.validate(4).is_ok());
}

// spec: INTEG-132
#[test]
fn command_line_uses_own_chrome_device_emulation_and_header_file() {
    let job = Job {
        url: "https://site.test/".into(),
        device: PsiDevice::Desktop,
        headers_file: Some(PathBuf::from("h.json")),
        output: PathBuf::from("r.json"),
    };
    let args: Vec<String> = NodeLighthouse::args(&job)
        .into_iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args[0], "https://site.test/");
    assert!(args.contains(&"--output=json".to_string()));
    assert!(args.contains(&"--output-path=r.json".to_string()));
    assert!(args.contains(&"--preset=desktop".to_string()));
    assert!(args.contains(&"--extra-headers=h.json".to_string()));
    assert!(args
        .iter()
        .any(|a| a.starts_with("--chrome-flags=--headless")));
    let mobile = Job {
        device: PsiDevice::Mobile,
        headers_file: None,
        ..job
    };
    let args: Vec<String> = NodeLighthouse::args(&mobile)
        .into_iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(args.contains(&"--form-factor=mobile".to_string()));
    assert!(!args.iter().any(|a| a.starts_with("--extra-headers")));
}

// spec: INTEG-132
#[tokio::test]
async fn parallelism_one_and_three_are_never_exceeded() {
    for parallel in [1u32, 3] {
        let (_, runner) = runner(parallel, Duration::from_secs(60));
        let sampler = {
            let runner = runner.clone();
            tokio::spawn(async move {
                let mut max = 0;
                loop {
                    max = max.max(runner.active());
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    if runner.mean_run().is_some() && runner.active() == 0 {
                        return max;
                    }
                }
            })
        };
        let jobs = (0..6).map(|i| {
            let runner = runner.clone();
            async move {
                runner
                    .run(
                        &format!("https://site.test/{i}/slow"),
                        PsiDevice::Mobile,
                        &[],
                    )
                    .await
            }
        });
        let results = futures_util::future::join_all(jobs).await;
        assert!(results.iter().all(|r| r.is_success()), "{results:?}");
        let sampled = sampler.await.unwrap();
        assert!(sampled <= parallel as usize, "{sampled} > {parallel}");
        assert!(runner.peak() <= parallel as usize);
        assert_eq!(
            runner.peak(),
            parallel as usize,
            "Parallelität wird genutzt"
        );
        assert_eq!(runner.active(), 0);
    }
}

// spec: INTEG-132
#[tokio::test]
async fn a_page_that_never_loads_times_out_and_the_next_url_runs_normally() {
    let (_, runner) = runner(1, Duration::from_secs(2));
    let out = runner
        .run("https://site.test/never-loads/hang", PsiDevice::Mobile, &[])
        .await;
    assert_eq!(text(&out, "PSI Status"), "Error");
    assert_eq!(text(&out, "PSI Error"), "Timeout");
    assert_eq!(runner.active(), 0, "Prozess des Laufs ist beendet");
    let next = runner
        .run("https://site.test/heavy", PsiDevice::Mobile, &[])
        .await;
    assert!(next.is_success());
}

// spec: INTEG-132
#[tokio::test]
async fn a_crashed_lighthouse_process_only_fails_its_own_url() {
    let (_, runner) = runner(2, Duration::from_secs(60));
    let (a, b, c) = tokio::join!(
        runner.run("https://site.test/a/heavy", PsiDevice::Mobile, &[]),
        runner.run("https://site.test/b/crash", PsiDevice::Mobile, &[]),
        runner.run("https://site.test/c/heavy", PsiDevice::Mobile, &[]),
    );
    assert!(a.is_success() && c.is_success());
    assert_eq!(text(&b, "PSI Status"), "Error");
    assert!(
        text(&b, "PSI Error").starts_with("LIGHTHOUSE_CRASHED"),
        "{b:?}"
    );
}

// spec: INTEG-132
#[tokio::test]
async fn lighthouse_version_is_shown_in_the_dialog_and_the_column() {
    let (_, runner) = runner(1, Duration::from_secs(60));
    let version = runner.version().await.unwrap();
    assert_eq!(version, "13.0.0");
    let out = runner
        .run("https://site.test/heavy", PsiDevice::Mobile, &[])
        .await;
    assert_eq!(text(&out, "Lighthouse Version"), version);
    let source = PsiSource::new().with_local(runner);
    let cred = os_integrations::credentials::Credential {
        account_id: "local".into(),
        label: String::new(),
        api_key: None,
        access_token: None,
        refresh_token: None,
        expires_at: None,
        client_secret: None,
        scopes: vec![],
    };
    let connected = source.test_connection(&local_ctx(), &cred).await.unwrap();
    assert_eq!(connected.label, "Lighthouse 13.0.0");
    assert_eq!(parse_version("Lighthouse v12.8.2\n"), Some("12.8.2".into()));
    assert_eq!(parse_version("running 1 test"), None);
}

// spec: INTEG-132
#[tokio::test]
async fn crawl_authentication_headers_are_passed_to_the_local_run() {
    let (fake, runner) = runner(1, Duration::from_secs(60));
    let mut ctx = local_ctx();
    ctx.cfg.http.headers = vec![
        HeaderEntry {
            name: "X-Staging".into(),
            value: "1".into(),
            host: None,
        },
        HeaderEntry {
            name: "X-Other-Host".into(),
            value: "nein".into(),
            host: Some("other.test".into()),
        },
    ];
    assert_eq!(
        config_headers(&ctx.cfg, "https://site.test/"),
        vec![("X-Staging".to_string(), "1".to_string())]
    );
    let source = PsiSource::new()
        .with_local(runner)
        .with_local_headers(vec![basic_auth_header("user", "pass")]);
    let values = source
        .fetch(
            &[UrlRef {
                url_id: Some(os_model::UrlId(4)),
                url: "https://site.test/auth/heavy".into(),
            }],
            &ctx,
        )
        .await
        .unwrap();
    assert!(values
        .iter()
        .any(|v| v.metric == "Performance Score" && v.v_real == Some(42.0)));
    let seen = fake.headers.lock().unwrap().clone();
    assert_eq!(
        seen,
        vec![serde_json::json!({"X-Staging": "1", "Authorization": "Basic dXNlcjpwYXNz"})]
    );
    // Ohne Zugangsdaten liefert die Seite einen Lighthouse-Fehler (Ersatzprozess: Modus `errored`).
    let (_, plain) = self::runner(1, Duration::from_secs(60));
    let out = plain
        .run("https://site.test/auth/errored", PsiDevice::Mobile, &[])
        .await;
    assert_eq!(text(&out, "PSI Status"), "Error");
    assert!(text(&out, "PSI Error").starts_with("ERRORED_DOCUMENT_REQUEST"));
}

// spec: INTEG-132
#[tokio::test]
#[ignore = "Klärung K3-01: Lighthouse wird nicht gebündelt, Lauf gegen eine vorhandene Node-/Lighthouse-Installation (OPENSPIDER_LIGHTHOUSE)"]
async fn real_lighthouse_installation_scores_a_basic_auth_page() {
    let cli = std::env::var_os("OPENSPIDER_LIGHTHOUSE").expect("Pfad zur Lighthouse-CLI");
    let runner = LocalRunner::discover(&LocalOptions {
        lighthouse: Some(cli.into()),
        ..LocalOptions::default()
    })
    .unwrap();
    let url =
        std::env::var("OPENSPIDER_LIGHTHOUSE_AUTH_URL").expect("Fixture-URL hinter Basic Auth");
    let ok = runner
        .run(
            &url,
            PsiDevice::Mobile,
            &[basic_auth_header("user", "pass")],
        )
        .await;
    assert!(ok.get("Performance Score").is_some(), "{ok:?}");
    let denied = runner.run(&url, PsiDevice::Mobile, &[]).await;
    assert!(text(&denied, "PSI Error").starts_with("ERRORED_DOCUMENT_REQUEST"));
}

// spec: TECH-230
#[tokio::test]
async fn local_and_remote_yield_identical_values_for_the_same_lighthouse_result() {
    let (_, runner) = runner(1, Duration::from_secs(60));
    let url = "https://site.test/heavy";
    let local = runner.run(url, PsiDevice::Mobile, &[]).await;
    let remote = parse_psi(&os_testsite::mocks::psi::psi_heavy(url, false));
    let without_crux: Vec<_> = remote
        .cells
        .iter()
        .filter(|(n, _)| !n.contains("CrUX") && n != "Core Web Vitals Assessment")
        .cloned()
        .collect();
    assert_eq!(local.cells, without_crux);
    assert_eq!(
        local,
        parse_lighthouse(&os_testsite::mocks::psi::lighthouse_heavy(url, false))
    );
    // Lokal: keine CrUX-Spalten.
    assert!(!local.cells.iter().any(|(n, _)| n.contains("CrUX")));
}

// spec: INTEG-142
#[tokio::test]
async fn the_daily_limit_does_not_apply_to_the_local_source() {
    let (_, runner) = runner(1, Duration::from_secs(60));
    let mut ctx = local_ctx();
    ctx.cfg.api.psi.limits.quota_limit = Some(1);
    let source = PsiSource::new().with_local(runner);
    for i in 0..2 {
        let out = source
            .fetch_outcome(&format!("https://site.test/{i}/heavy"), &ctx)
            .await
            .unwrap();
        assert!(out.is_success(), "Lauf {i}: {out:?}");
    }
    assert_eq!(source.daily_used(), 0);
}

// --- RC1-PSILOCAL (Issue 21): Quelle Lokal im Crawl-Weg über `ApiRun` ---

fn crawl_file(dir: &tempfile::TempDir, cfg: &CrawlConfig) -> os_store::CrawlFile {
    use os_model::Clock;
    let clock =
        os_model::ManualClock::new(os_model::Timestamp::from_db("2026-11-15T12:00:00Z").unwrap());
    let file = os_store::CrawlFile::create(
        &dir.path().join("crawl.ospider"),
        &os_store::CrawlInit {
            crawl_id: "01900000-0000-7000-8000-000000000021".parse().unwrap(),
            mode: os_model::CrawlMode::List,
            start_url: None,
            list_source: None,
            project: "fixture".into(),
            name: "fixture".into(),
            config: cfg.clone(),
            created_at: clock.now(),
        },
        os_store::StorageProfile::default(),
    )
    .unwrap();
    // Zwei abfragbare HTML-Seiten (Ersatzprozess: Modus `heavy`) und eine Ressource, die PSI nie abfragt.
    for (id, url, class) in [
        (1, "https://site.test/1/heavy", "HTML"),
        (2, "https://site.test/2/heavy", "HTML"),
        (3, "https://site.test/style.css", "CSS"),
    ] {
        file.conn()
            .execute(
                "INSERT INTO urls(id,url,address,origin,is_internal,in_tables,state,discovered_via,folder_depth,\
                 url_length,content_class,status_code) \
                 VALUES(?1,?2,?2,'https://site.test',1,1,'discovered','list_input',1,?3,?4,200)",
                (id, url, url.len() as i64, class),
            )
            .unwrap();
    }
    file
}

fn glue_clock() -> Arc<dyn os_model::Clock> {
    Arc::new(os_model::ManualClock::new(
        os_model::Timestamp::from_db("2026-11-15T12:00:00Z").unwrap(),
    ))
}

// spec: INTEG-130
// spec: INTEG-132
#[tokio::test]
async fn local_source_without_account_runs_lighthouse_through_the_crawl_glue() {
    use os_integrations::crawl_glue::ApiRun;
    let (_, runner) = runner(1, Duration::from_secs(60));
    let ctx = local_ctx();
    let dir = tempfile::tempdir().unwrap();
    let file = crawl_file(&dir, &ctx.cfg);
    // Kein Konto im Schlüsselbund: Quelle Lokal verlangt keinen Schlüssel.
    let mut run = ApiRun::new(ctx, glue_clock(), dir.path().join("quota.json"))
        .with_psi_local(runner.clone());
    assert!(run.validate(true).unwrap().is_empty());
    run.collect(&file).await.unwrap();
    let rows: Vec<(i64, f64)> = file
        .conn()
        .prepare(
            "SELECT url_id, v_real FROM api_values WHERE source='psi' \
             AND metric='Performance Score' ORDER BY url_id",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, vec![(1, 42.0), (2, 42.0)]);
    assert!(runner.mean_run().is_some(), "Läufer wurde aufgerufen");
    let errors: i64 = file
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM api_errors WHERE source='psi'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(errors, 0);
    // Der Weg ohne Testläufer hängt einen Läufer an, sobald die Laufzeit gefunden wird; ohne Laufzeit keinen.
    assert!(
        os_integrations::psi::source_for(&local_ctx().cfg, glue_clock())
            .local_runner()
            .is_none()
    );
}

// spec: INTEG-132
// spec: INTEG-143
#[tokio::test]
async fn local_source_without_runtime_warns_or_fails_with_the_runtime_message() {
    use os_integrations::crawl_glue::{self, ApiRun};
    let ctx = local_ctx();
    // Ohne konfigurierten Pfad findet die Suche keine Laufzeit (kein Bündel, kein Konfigurationsfeld).
    assert_eq!(
        os_integrations::psi::local_runner_for(&ctx.cfg).err(),
        Some(ApiError::NotConnected(RUNTIME_MISSING.into()))
    );
    let warnings = crawl_glue::preflight(&ctx, false).unwrap();
    assert_eq!(
        warnings,
        ["psi: PageSpeed läuft nicht lokal (Fehler: Lighthouse-Laufzeit nicht gefunden)"]
    );
    assert!(!warnings[0].contains("Konto"), "kein Konto verlangt");
    let err = crawl_glue::preflight(&ctx, true).unwrap_err();
    assert!(err.to_string().contains(RUNTIME_MISSING), "{err}");
    assert!(crawl_glue::validate_apis(&ctx).is_err());

    let dir = tempfile::tempdir().unwrap();
    let file = crawl_file(&dir, &ctx.cfg);
    let mut run = ApiRun::new(ctx.clone(), glue_clock(), dir.path().join("quota.json"));
    assert_eq!(run.validate(false).unwrap(), warnings);
    run.collect(&file).await.unwrap();
    let values: i64 = file
        .conn()
        .query_row("SELECT COUNT(*) FROM api_values", [], |r| r.get(0))
        .unwrap();
    assert_eq!(values, 0, "weggefallene Quelle fragt nichts ab");
    let mut strict = ApiRun::new(ctx, glue_clock(), dir.path().join("quota.json"));
    assert!(strict
        .validate(true)
        .unwrap_err()
        .to_string()
        .contains(RUNTIME_MISSING));
}

// spec: INTEG-130
#[tokio::test]
async fn remote_source_without_key_stays_disconnected_and_is_not_switched_to_local() {
    use os_integrations::crawl_glue::{self, ApiRun};
    let mut ctx = local_ctx();
    ctx.cfg.api.psi.local = false;
    assert_eq!(
        crawl_glue::preflight(&ctx, false).unwrap(),
        ["psi: Konto Standard nicht verbunden"]
    );
    assert!(crawl_glue::preflight(&ctx, true).is_err());
    assert!(os_integrations::psi::source_for(&ctx.cfg, glue_clock())
        .local_runner()
        .is_none());
    // Ein Testläufer ändert an Remote nichts: keine stille Umstellung auf Lokal.
    let (_, runner) = runner(1, Duration::from_secs(60));
    let dir = tempfile::tempdir().unwrap();
    let file = crawl_file(&dir, &ctx.cfg);
    let mut run = ApiRun::new(ctx, glue_clock(), dir.path().join("quota.json"))
        .with_psi_local(runner.clone());
    assert_eq!(
        run.validate(false).unwrap(),
        ["psi: Konto Standard nicht verbunden"]
    );
    run.collect(&file).await.unwrap();
    assert!(runner.mean_run().is_none(), "kein lokaler Lauf");
}

// --- RC1-PSILOCAL: Pfad `api.psi.lighthouse_path` bis zum Läufer ---

/// Starter wie der npm-Starter `lighthouse`: ein ausführbares Skript ohne Endung, das hier den Ersatzprozess
/// (`fake_lighthouse_entry`) aufruft. Nur Unix: der Starter ist ein Shell-Skript (nur im Test).
#[cfg(unix)]
fn fake_cli(dir: &std::path::Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let exe = std::env::current_exe().unwrap();
    let script = format!(
        "#!/bin/sh\n\
         url=\"$1\"; out=\"\"\n\
         for a in \"$@\"; do case \"$a\" in --output-path=*) out=\"${{a#--output-path=}}\";; esac; done\n\
         if [ \"$url\" = \"--version\" ]; then mode=version; else mode=\"${{url##*/}}\"; fi\n\
         export {MODE}=\"$mode\" {OUT}=\"$out\" {URL_VAR}=\"$url\"\n\
         exec '{}' fake_lighthouse_entry --exact --nocapture --test-threads=1\n",
        exe.display()
    );
    let cli = dir.join("lighthouse");
    std::fs::write(&cli, script).unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    cli
}

// spec: INTEG-132
#[cfg(unix)]
#[tokio::test]
async fn configured_lighthouse_path_reaches_the_runner_through_the_crawl_glue() {
    use os_integrations::crawl_glue::{self, ApiRun};
    let bin = tempfile::tempdir().unwrap();
    let mut ctx = local_ctx();
    ctx.cfg.api.psi.lighthouse_path = Some(fake_cli(bin.path()));
    assert_eq!(
        LocalOptions::from_config(&ctx.cfg.api.psi).lighthouse,
        ctx.cfg.api.psi.lighthouse_path
    );
    assert!(crawl_glue::preflight(&ctx, true).unwrap().is_empty());
    assert!(crawl_glue::validate_apis(&ctx).is_ok());
    // Verbinden zeigt die Version der konfigurierten Installation.
    let cred = os_integrations::credentials::Credential {
        account_id: "local".into(),
        label: String::new(),
        api_key: None,
        access_token: None,
        refresh_token: None,
        expires_at: None,
        client_secret: None,
        scopes: vec![],
    };
    let connected = os_integrations::psi::source_for(&ctx.cfg, glue_clock())
        .test_connection(&ctx, &cred)
        .await
        .unwrap();
    assert_eq!(connected.label, "Lighthouse 13.0.0");
    let dir = tempfile::tempdir().unwrap();
    let file = crawl_file(&dir, &ctx.cfg);
    // Ohne Testläufer: ApiRun baut den Läufer allein aus der Konfiguration.
    let mut run = ApiRun::new(ctx, glue_clock(), dir.path().join("quota.json"));
    assert!(run.validate(true).unwrap().is_empty());
    run.collect(&file).await.unwrap();
    let rows: Vec<(i64, f64)> = file
        .conn()
        .prepare(
            "SELECT url_id, v_real FROM api_values WHERE source='psi' \
             AND metric='Performance Score' ORDER BY url_id",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, vec![(1, 42.0), (2, 42.0)]);
}

// spec: INTEG-132
#[tokio::test]
async fn missing_or_invalid_lighthouse_path_reports_the_runtime_message() {
    use os_integrations::crawl_glue;
    let dir = tempfile::tempdir().unwrap();
    for path in [
        dir.path().join("fehlt").join("lighthouse"),
        dir.path().to_path_buf(),
    ] {
        let mut ctx = local_ctx();
        ctx.cfg.api.psi.lighthouse_path = Some(path.clone());
        assert_eq!(
            os_integrations::psi::local_runner_for(&ctx.cfg).err(),
            Some(ApiError::NotConnected(RUNTIME_MISSING.into())),
            "{}",
            path.display()
        );
        let err = crawl_glue::preflight(&ctx, true).unwrap_err();
        assert!(err.to_string().contains(RUNTIME_MISSING), "{err}");
        let cred = os_integrations::credentials::Credential {
            account_id: "local".into(),
            label: String::new(),
            api_key: None,
            access_token: None,
            refresh_token: None,
            expires_at: None,
            client_secret: None,
            scopes: vec![],
        };
        assert_eq!(
            os_integrations::psi::source_for(&ctx.cfg, glue_clock())
                .test_connection(&ctx, &cred)
                .await
                .unwrap_err(),
            ApiError::NotConnected(RUNTIME_MISSING.into())
        );
    }
}

// spec: INTEG-132
#[test]
fn a_directly_started_cli_finds_node_in_its_own_directory() {
    let dir = tempfile::tempdir().unwrap();
    let node = NodeLighthouse {
        program: dir.path().join("lighthouse"),
        prefix: Vec::new(),
    };
    let cmd = node.version_command();
    let path = cmd
        .get_envs()
        .find(|(k, _)| *k == "PATH")
        .and_then(|(_, v)| v)
        .expect("PATH des Kindprozesses");
    assert_eq!(
        std::env::split_paths(path).next().as_deref(),
        Some(dir.path())
    );
    // Ein Skript mit Node als Programm lässt PATH unverändert.
    let script = NodeLighthouse {
        program: dir.path().join("node"),
        prefix: vec![dir.path().join("cli.js").into_os_string()],
    };
    assert!(script.version_command().get_envs().next().is_none());
}

// spec: INTEG-132
#[test]
fn windows_npm_launchers_resolve_to_the_lighthouse_script_and_are_never_run() {
    let dir = tempfile::tempdir().unwrap();
    let node = dir.path().join("node-bin");
    std::fs::write(&node, "").unwrap();
    let script = dir
        .path()
        .join("node_modules")
        .join("lighthouse")
        .join("cli")
        .join("index.js");
    for name in [
        "lighthouse.cmd",
        "LIGHTHOUSE.CMD",
        "lighthouse.bat",
        "lighthouse.Ps1",
    ] {
        let launcher = dir.path().join(name);
        std::fs::write(&launcher, "@echo off\r\n").unwrap();
        let opts = LocalOptions {
            lighthouse: Some(launcher.clone()),
            node: Some(node.clone()),
            ..LocalOptions::default()
        };
        // Ohne Skript daneben: Laufzeit nicht gefunden (der Starter wird nicht ausgeführt).
        let _ = std::fs::remove_file(&script);
        assert_eq!(
            NodeLighthouse::discover(&opts).unwrap_err(),
            ApiError::NotConnected(RUNTIME_MISSING.into()),
            "{name}"
        );
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, "").unwrap();
        let found = NodeLighthouse::discover(&opts).unwrap();
        assert_eq!(found.program, node, "{name}");
        assert_eq!(
            found.prefix,
            vec![script.clone().into_os_string()],
            "{name}"
        );
        let cmd = found.version_command();
        assert_ne!(cmd.get_program(), launcher.as_os_str(), "{name}");
        assert!(!cmd.get_args().any(|a| a == launcher.as_os_str()), "{name}");
    }
}
