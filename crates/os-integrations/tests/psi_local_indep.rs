//! Unabhängige Tests (nur aus der Spezifikation) zur PageSpeed-Quelle „Lokal“ (INTEG-130, INTEG-132, INTEG-143).
//!
//! Als Lighthouse-Laufzeit dient die Fixture `tests/fixtures/integ-psi-local/fake-lighthouse.js`: ein Node-Skript
//! ohne Netzwerk und ohne Chrome, das jeden Aufruf in `calls.log` neben sich protokolliert, bei `--version` die
//! Version `12.4.1` ausgibt und sonst einen minimalen Lighthouse-Bericht schreibt. Die Tests kopieren es in ein
//! temporäres Verzeichnis mit Leerzeichen im Namen.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use os_fetch::MemorySecretStore;
use os_integrations::crawl_glue::{self, ApiRun};
use os_integrations::credentials::{Credential, Credentials};
use os_integrations::psi::{self, local};
use os_integrations::{ApiCtx, ApiError, ApiSource};
use os_model::{ApiKind, Clock, CrawlConfig, CrawlMode, ManualClock, PsiDevice, Timestamp};
use os_store::{CrawlFile, CrawlInit, StorageProfile};
use os_testsite::mocks::{psi::PsiMock, MockServer};

const FAKE: &str = include_str!("../../../tests/fixtures/integ-psi-local/fake-lighthouse.js");
const FAKE_VERSION: &str = "12.4.1";
const MISSING: &str = "Fehler: Lighthouse-Laufzeit nicht gefunden";

/// Temporäres Verzeichnis mit einem Unterordner, dessen Name Leerzeichen enthält.
struct Install {
    _root: tempfile::TempDir,
    dir: PathBuf,
}

impl Install {
    fn new() -> Install {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Light House Tools");
        std::fs::create_dir_all(&dir).unwrap();
        Install { _root: root, dir }
    }

    /// Legt die Ersatz-Lighthouse unter `name` ab.
    fn script(&self, name: &str) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::write(&path, FAKE).unwrap();
        path
    }

    /// Aufrufe der Ersatz-Lighthouse (je Zeile die Argumente als JSON-Array).
    fn calls(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// Läufe (Aufrufe ohne `--version`).
    fn runs(&self) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|a| !a.iter().any(|x| x == "--version"))
            .collect()
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn local_cfg(path: Option<PathBuf>) -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    cfg.api.psi.enabled = true;
    cfg.api.psi.local = true;
    cfg.api.psi.lighthouse_path = path;
    cfg
}

fn ctx(cfg: CrawlConfig) -> ApiCtx {
    ApiCtx {
        cfg,
        secrets: Arc::new(MemorySecretStore::new()),
        network: Default::default(),
        allow_api_override: true,
        // Nur ein Pfad; ohne GSC wird das Kontingentbuch nicht beschrieben.
        quota_ledger: Some(std::env::temp_dir().join("os-psi-local-indep-quotas.json")),
    }
}

/// Kontext mit gespeichertem Remote-Schlüssel (Konto `default`).
fn ctx_with_key(cfg: CrawlConfig, key: &str) -> ApiCtx {
    let store = Arc::new(MemorySecretStore::new());
    Credentials::new(store.clone())
        .put(
            ApiKind::Psi,
            Credential {
                account_id: "default".into(),
                api_key: Some(key.into()),
                ..no_credential()
            },
            true,
        )
        .unwrap();
    ApiCtx {
        secrets: store,
        ..ctx(cfg)
    }
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(ManualClock::new(
        Timestamp::from_db("2026-11-15T12:00:00Z").unwrap(),
    ))
}

fn no_credential() -> Credential {
    Credential {
        account_id: "local".into(),
        label: "PageSpeed".into(),
        api_key: None,
        access_token: None,
        refresh_token: None,
        expires_at: None,
        client_secret: None,
        scopes: vec![],
    }
}

fn assert_missing<T>(result: Result<T, ApiError>, case: &str) {
    match result {
        Err(ApiError::NotConnected(m)) => assert_eq!(m, MISSING, "{case}"),
        Err(other) => panic!("{case}: erwartet NotConnected({MISSING}), erhalten {other:?}"),
        Ok(_) => panic!("{case}: erwartet NotConnected({MISSING}), erhalten Ok"),
    }
}

// spec: INTEG-132
#[test]
fn runtime_missing_status_text_matches_spec() {
    assert_eq!(local::RUNTIME_MISSING, MISSING);
}

// spec: INTEG-130
#[test]
fn default_profile_is_remote_and_has_no_local_runner() {
    let cfg = CrawlConfig::default();
    assert!(!psi::is_local(&cfg));
    assert!(psi::source_for(&cfg, clock()).local_runner().is_none());
}

// spec: INTEG-132
#[test]
fn local_without_usable_runtime_is_not_connectable() {
    let install = Install::new();
    let not_there = install.dir.join("fehlt").join("lighthouse.js");
    let dir_named_js = install.dir.join("cli.js");
    std::fs::create_dir_all(&dir_named_js).unwrap();
    let cases: Vec<(&str, Option<PathBuf>)> = vec![
        ("kein Pfad", None),
        ("leerer Pfad", Some(PathBuf::from(""))),
        ("nur Leerzeichen", Some(PathBuf::from("   "))),
        ("Pfad existiert nicht", Some(not_there)),
        ("Verzeichnis statt Datei", Some(install.dir.clone())),
        ("Verzeichnis mit .js-Endung", Some(dir_named_js)),
        (
            "relativer Pfad ohne Datei",
            Some(PathBuf::from("gibt-es-nicht").join("lighthouse.js")),
        ),
    ];
    for (case, path) in cases {
        let cfg = local_cfg(path);
        assert!(psi::is_local(&cfg), "{case}");
        assert_missing(psi::local_runner_for(&cfg), case);
        assert_missing(
            local::NodeLighthouse::discover(&local::LocalOptions::from_config(&cfg.api.psi)),
            case,
        );
        // Die Quelle bleibt ohne Läufer; „Verbinden“ scheitert mit genau dem Status aus der Spezifikation.
        let source = psi::source_for(&cfg, clock());
        assert!(source.local_runner().is_none(), "{case}");
    }
    assert!(install.calls().is_empty(), "nichts darf gestartet werden");
}

// spec: INTEG-132
#[tokio::test]
async fn connect_without_runtime_reports_missing_runtime() {
    let ctx = ctx(local_cfg(None));
    let source = psi::source_for(&ctx.cfg, clock());
    assert_missing(
        source.test_connection(&ctx, &no_credential()).await,
        "Verbinden ohne Laufzeit",
    );
}

// spec: INTEG-130
// spec: INTEG-132
#[tokio::test]
async fn script_in_path_with_spaces_connects_without_key_and_shows_version() {
    for name in ["light house.js", "cli.cjs"] {
        let install = Install::new();
        let script = install.script(name);
        let cfg = local_cfg(Some(script.clone()));
        let found =
            local::NodeLighthouse::discover(&local::LocalOptions::from_config(&cfg.api.psi))
                .unwrap();
        assert_eq!(
            found.prefix,
            vec![script.clone().into_os_string()],
            "{name}: Skript wird als Argument an Node übergeben"
        );
        assert_ne!(found.program, script, "{name}: Node startet das Skript");

        let runner = psi::local_runner_for(&cfg).unwrap();
        assert_eq!(runner.version().await.unwrap(), FAKE_VERSION, "{name}");

        // „Verbinden“ ohne Schlüssel und ohne Konto (leerer Secret Store, keine Konto-Kennung).
        let ctx = ctx(cfg);
        assert_eq!(ctx.cfg.api.psi.account, None);
        let source = psi::source_for(&ctx.cfg, clock());
        let connected = source
            .test_connection(&ctx, &no_credential())
            .await
            .unwrap();
        assert!(
            connected.label.contains(FAKE_VERSION),
            "{name}: Version im Kontodialog, erhalten {:?}",
            connected.label
        );
        assert_eq!(connected.api_key, None);
    }
}

// spec: INTEG-132
#[tokio::test]
async fn esm_script_is_run_by_node() {
    let install = Install::new();
    let script = install.dir.join("index.mjs");
    std::fs::write(
        &script,
        format!("process.stdout.write(\"Lighthouse {FAKE_VERSION}\\n\");\n"),
    )
    .unwrap();
    let runner = psi::local_runner_for(&local_cfg(Some(script))).unwrap();
    assert_eq!(runner.version().await.unwrap(), FAKE_VERSION);
}

// spec: INTEG-132
#[cfg(unix)]
#[tokio::test]
async fn executable_without_extension_is_started_directly() {
    let install = Install::new();
    let exe = install.script("light house");
    make_executable(&exe);
    let found = local::NodeLighthouse::discover(&local::LocalOptions::from_config(
        &local_cfg(Some(exe.clone())).api.psi,
    ))
    .unwrap();
    assert_eq!(found.program, exe);
    assert!(found.prefix.is_empty());
    let runner = psi::local_runner_for(&local_cfg(Some(exe))).unwrap();
    assert_eq!(runner.version().await.unwrap(), FAKE_VERSION);
    assert_eq!(install.calls(), vec![vec!["--version".to_string()]]);
}

// spec: INTEG-130
#[tokio::test]
async fn local_false_ignores_a_valid_lighthouse_path() {
    let install = Install::new();
    let mut cfg = local_cfg(Some(install.script("index.js")));
    cfg.api.psi.local = false;
    assert!(!psi::is_local(&cfg));
    assert!(psi::source_for(&cfg, clock()).local_runner().is_none());

    // Remote ohne Schlüssel bleibt „nicht verbunden“ – keine stille Umstellung auf Lokal.
    let ctx = ctx(cfg);
    let warnings = crawl_glue::preflight(&ctx, false).unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        !warnings[0].contains("Lighthouse"),
        "Remote meldet ein fehlendes Konto, nicht die Laufzeit: {warnings:?}"
    );
    assert!(crawl_glue::preflight(&ctx, true).is_err());
    assert!(install.calls().is_empty());
}

// spec: INTEG-130
#[test]
fn remote_with_key_passes_preflight_regardless_of_lighthouse_path() {
    for path in [None, Some(PathBuf::from("/gibt/es/nicht/lighthouse"))] {
        let mut cfg = local_cfg(path);
        cfg.api.psi.local = false;
        let ctx = ctx_with_key(cfg, "fixture-key");
        assert_eq!(
            crawl_glue::preflight(&ctx, true).unwrap(),
            Vec::<String>::new()
        );
        let mut run = ApiRun::new(ctx, clock(), PathBuf::from("unused.api-quotas.json"));
        assert_eq!(run.validate(true).unwrap(), Vec::<String>::new());
    }
}

// spec: INTEG-130
// spec: INTEG-132
#[test]
fn preflight_warns_without_runtime_and_fails_when_apis_are_required() {
    let install = Install::new();
    for (case, path) in [
        ("kein Pfad", None),
        ("fehlende Datei", Some(install.dir.join("fehlt.js"))),
        ("Verzeichnis", Some(install.dir.clone())),
    ] {
        let ctx = ctx(local_cfg(path));
        let warnings = crawl_glue::preflight(&ctx, false).unwrap();
        assert_eq!(warnings.len(), 1, "{case}: {warnings:?}");
        assert!(
            warnings[0].contains("Lighthouse-Laufzeit nicht gefunden"),
            "{case}: {warnings:?}"
        );
        match crawl_glue::preflight(&ctx, true) {
            Err(ApiError::NotConnected(m)) => {
                assert!(
                    m.contains("Lighthouse-Laufzeit nicht gefunden"),
                    "{case}: {m}"
                )
            }
            other => panic!("{case}: {other:?}"),
        }
        let mut run = ApiRun::new(
            ctx.clone(),
            clock(),
            PathBuf::from("unused.api-quotas.json"),
        );
        assert!(run.validate(true).is_err(), "{case}");
        let mut run = ApiRun::new(ctx, clock(), PathBuf::from("unused.api-quotas.json"));
        assert_eq!(run.validate(false).unwrap().len(), 1, "{case}");
    }
}

// spec: INTEG-130
// spec: INTEG-132
#[test]
fn preflight_with_runtime_needs_no_key_or_account() {
    let install = Install::new();
    let ctx = ctx(local_cfg(Some(install.script("index.js"))));
    assert_eq!(ctx.cfg.api.psi.account, None);
    assert_eq!(
        crawl_glue::preflight(&ctx, true).unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(
        crawl_glue::preflight(&ctx, false).unwrap(),
        Vec::<String>::new()
    );
    let mut run = ApiRun::new(ctx, clock(), PathBuf::from("unused.api-quotas.json"));
    assert_eq!(run.validate(true).unwrap(), Vec::<String>::new());
}

fn crawl_file(dir: &Path, cfg: &CrawlConfig, urls: &[&str]) -> CrawlFile {
    let file = CrawlFile::create(
        &dir.join("crawl.ospider"),
        &CrawlInit {
            crawl_id: "01900000-0000-7000-8000-000000000132".parse().unwrap(),
            mode: CrawlMode::List,
            start_url: None,
            list_source: Some("Liste".into()),
            project: "psi-local".into(),
            name: "psi-local".into(),
            config: cfg.clone(),
            created_at: clock().now(),
        },
        StorageProfile::default(),
    )
    .unwrap();
    for (i, url) in urls.iter().enumerate() {
        file.conn()
            .execute(
                "INSERT INTO urls(id,url,address,origin,is_internal,in_tables,state,discovered_via,\
                 folder_depth,url_length,content_class,status_code) \
                 VALUES(?1,?2,?2,'https://staging.site.test',1,1,'fetched','list_input',1,?3,'HTML',200)",
                (i as i64 + 1, *url, url.len() as i64),
            )
            .unwrap();
    }
    file
}

fn psi_values(file: &CrawlFile, url: &str) -> Vec<(String, Option<f64>, Option<String>)> {
    let mut stmt = file
        .conn()
        .prepare(
            "SELECT v.metric, v.v_real, v.v_text FROM api_values v JOIN urls u ON u.id = v.url_id \
             WHERE v.source = 'psi' AND u.url = ?1 ORDER BY v.metric",
        )
        .unwrap();
    stmt.query_map([url], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn text_of(values: &[(String, Option<f64>, Option<String>)], metric: &str) -> Option<String> {
    values
        .iter()
        .find(|(m, _, _)| m == metric)
        .and_then(|(_, _, t)| t.clone())
}

fn num_of(values: &[(String, Option<f64>, Option<String>)], metric: &str) -> Option<f64> {
    values
        .iter()
        .find(|(m, _, _)| m == metric)
        .and_then(|(_, n, _)| *n)
}

// spec: INTEG-130
// spec: INTEG-132
// spec: INTEG-143
#[tokio::test]
async fn crawl_with_local_runtime_stores_data_without_calling_the_psi_api() {
    let mock = PsiMock::default();
    let server = MockServer::start(vec![Arc::new(mock.clone())])
        .await
        .unwrap();
    let install = Install::new();
    let mut cfg = local_cfg(Some(install.script("index.js")));
    cfg.api.psi.base_url = Some(server.base_url("psi"));
    cfg.api.psi.device = PsiDevice::Desktop;
    // Ein gespeicherter Remote-Schlüssel darf Lokal nicht auf die API umlenken.
    let mut ctx = ctx_with_key(cfg.clone(), "key-ok");
    let dir = tempfile::tempdir().unwrap();
    ctx.quota_ledger = Some(dir.path().join("api-quotas.json"));
    let urls = [
        "https://staging.site.test/",
        "https://staging.site.test/login/area",
        "https://staging.site.test/errored",
    ];
    let file = crawl_file(dir.path(), &cfg, &urls);

    let mut run = ApiRun::new(ctx, clock(), dir.path().join("crawl.api-quotas.json"));
    assert_eq!(run.validate(true).unwrap(), Vec::<String>::new());
    run.collect(&file).await.unwrap();

    for url in &urls[..2] {
        let values = psi_values(&file, url);
        assert_eq!(
            text_of(&values, "PSI Status").as_deref(),
            Some("Success"),
            "{url}: {values:?}"
        );
        assert_eq!(
            text_of(&values, "Lighthouse Version").as_deref(),
            Some(FAKE_VERSION),
            "{url}: Spalte Lighthouse Version = Version des Laufs"
        );
        assert_eq!(num_of(&values, "Performance Score"), Some(97.0), "{url}");
    }
    // INTEG-143: Seitenfehler aus Lighthouse → Error mit Code, ohne Wiederholung.
    let errored = psi_values(&file, urls[2]);
    assert_eq!(text_of(&errored, "PSI Status").as_deref(), Some("Error"));
    assert!(text_of(&errored, "PSI Error")
        .unwrap_or_default()
        .starts_with("ERRORED_DOCUMENT_REQUEST"));

    let runs = install.runs();
    for url in urls {
        assert_eq!(
            runs.iter().filter(|a| a.iter().any(|x| x == url)).count(),
            1,
            "{url}: genau ein lokaler Lauf, {runs:?}"
        );
    }
    // INTEG-131: Gerät Desktop wird an Lighthouse weitergegeben.
    assert!(runs.iter().all(|a| a.iter().any(|x| x.contains("desktop"))));
    // Kein einziger Request an die PSI-API.
    assert!(mock.calls().is_empty(), "{:?}", mock.calls());
    assert!(mock.log.calls().is_empty());
    server.stop().await;
}

// spec: INTEG-130
#[tokio::test]
async fn remote_crawl_still_uses_the_psi_api_and_not_lighthouse() {
    let mock = PsiMock::default();
    let server = MockServer::start(vec![Arc::new(mock.clone())])
        .await
        .unwrap();
    let install = Install::new();
    let mut cfg = local_cfg(Some(install.script("index.js")));
    cfg.api.psi.local = false;
    cfg.api.psi.base_url = Some(server.base_url("psi"));
    let mut ctx = ctx_with_key(cfg.clone(), "key-ok");
    let dir = tempfile::tempdir().unwrap();
    ctx.quota_ledger = Some(dir.path().join("api-quotas.json"));
    let file = crawl_file(dir.path(), &cfg, &["https://site.test/ok"]);
    let mut run = ApiRun::new(ctx, clock(), dir.path().join("crawl.api-quotas.json"));
    assert_eq!(run.validate(true).unwrap(), Vec::<String>::new());
    run.collect(&file).await.unwrap();
    assert_eq!(mock.calls().len(), 1, "{:?}", mock.calls());
    assert!(
        install.calls().is_empty(),
        "Remote darf Lighthouse nicht lokal starten"
    );
    let values = psi_values(&file, "https://site.test/ok");
    assert_eq!(
        text_of(&values, "PSI Status").as_deref(),
        Some("Success"),
        "{values:?}"
    );
    server.stop().await;
}
