//! Unabhängige Tests (nur aus der Spezifikation) zur PageSpeed-Quelle Lokal über die Kommandozeile
//! (INTEG-130, INTEG-132; Exit-Codes von `--require-apis` laut OUT-201). Als Lighthouse dient die Fixture
//! `tests/fixtures/integ-psi-local/fake-lighthouse.js` (Node-Skript ohne Netzwerk, Version 12.4.1, protokolliert
//! Aufrufe in `calls.log`).
mod common;
use common::{column, read_csv, resolve, site, Env, Run};
use os_model::CrawlConfig;
use os_testsite::mocks::{psi::PsiMock, MockServer};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const FAKE: &str = include_str!("../../../tests/fixtures/integ-psi-local/fake-lighthouse.js");
const FAKE_VERSION: &str = "12.4.1";
const MISSING: &str = "Lighthouse-Laufzeit nicht gefunden";
const URLS: [&str; 2] = ["http://site.test/", "http://site.test/about.html"];

/// Ersatz-Lighthouse in einem Ordner mit Leerzeichen innerhalb des Testdatenordners.
fn install(env: &Env) -> PathBuf {
    let dir = env.home.path().join("Light House");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("index.js");
    std::fs::write(&path, FAKE).unwrap();
    path
}

fn runs(script: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(script.with_file_name("calls.log"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str::<Vec<String>>(l).unwrap())
        .filter(|a| !a.iter().any(|x| x == "--version"))
        .collect()
}

fn profile(env: &Env, local: bool, path: Option<PathBuf>, base_url: Option<String>) -> PathBuf {
    let config = env.home.path().join("psi local.osconfig");
    let mut cfg = CrawlConfig::default();
    cfg.api.psi.local = local;
    cfg.api.psi.lighthouse_path = path;
    cfg.api.psi.base_url = base_url;
    cfg.save_file(&config).unwrap();
    config
}

fn list_file(env: &Env) -> PathBuf {
    let list = env.work.path().join("urls.txt");
    std::fs::write(&list, URLS.join("\n") + "\n").unwrap();
    list
}

fn args(list: &Path, config: &Path, resolve: String, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "--crawl-list".to_string(),
        list.to_string_lossy().into_owned(),
        "--resolve".into(),
        resolve,
        "--config".into(),
        config.to_string_lossy().into_owned(),
        "--use-pagespeed".into(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    args
}

/// Wie `Env::run_async`, mit zusätzlichen Umgebungsvariablen.
async fn run_with(env: &Env, args: Vec<String>, vars: &[(&str, &str)]) -> Run {
    let mut cmd = env.command();
    cmd.args(&args);
    for (k, v) in vars {
        cmd.env(k, v);
    }
    tokio::task::spawn_blocking(move || Run::from_output(cmd.output().expect("Binary startbar")))
        .await
        .unwrap()
}

// spec: INTEG-130
// spec: INTEG-132
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn require_apis_with_local_runtime_crawls_and_exports_lighthouse_data() {
    let server = site("out-basic-site").await;
    let mock = PsiMock::default();
    let mocks = MockServer::start(vec![Arc::new(mock.clone())])
        .await
        .unwrap();
    let env = Env::new();
    let script = install(&env);
    let config = profile(
        &env,
        true,
        Some(script.clone()),
        Some(mocks.base_url("psi")),
    );
    let out = env.out();
    let run = run_with(
        &env,
        args(
            &list_file(&env),
            &config,
            resolve(&server),
            &[
                "--require-apis",
                "--output-folder",
                out.to_str().unwrap(),
                "--export-tabs",
                "PageSpeed:All",
            ],
        ),
        // Ein vorhandener Remote-Schlüssel darf Lokal nicht auf die PSI-API umlenken.
        &[("OPENSPIDER_PSI_KEY", "key-ok")],
    )
    .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(!run.stderr.contains(MISSING), "{}", run.dump());

    let runs = runs(&script);
    for url in URLS {
        assert_eq!(
            runs.iter().filter(|a| a.iter().any(|x| x == url)).count(),
            1,
            "{url}: genau ein lokaler Lauf, {runs:?}\n{}",
            run.dump()
        );
    }
    assert!(
        mock.calls().is_empty(),
        "PSI-API angesprochen: {:?}",
        mock.calls()
    );

    let csv: Vec<PathBuf> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .to_lowercase()
                .starts_with("pagespeed")
        })
        .collect();
    assert_eq!(csv.len(), 1, "{csv:?}");
    let rows = read_csv(&csv[0]);
    let address = column(&rows, "Address");
    let status = column(&rows, "PSI Status");
    let version = column(&rows, "Lighthouse Version");
    for url in URLS {
        let i = address
            .iter()
            .position(|a| a == url)
            .unwrap_or_else(|| panic!("{url} fehlt in {address:?}"));
        assert_eq!(status[i], "Success", "{url}");
        assert_eq!(version[i], FAKE_VERSION, "{url}");
    }
    mocks.stop().await;
}

// spec: INTEG-132
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_runtime_warns_without_and_fails_with_require_apis() {
    let server = site("out-basic-site").await;
    for case in ["kein Pfad", "Pfad existiert nicht", "Verzeichnis"] {
        for (flags, code) in [(vec![], 0), (vec!["--require-apis"], 2)] {
            let env = Env::new();
            let dir = env.home.path().join("Light House");
            std::fs::create_dir_all(&dir).unwrap();
            let path = match case {
                "kein Pfad" => None,
                "Pfad existiert nicht" => Some(dir.join("fehlt.js")),
                _ => Some(dir.clone()),
            };
            let config = profile(&env, true, path, None);
            let before = server.total_requests();
            let run = env
                .run_async(args(&list_file(&env), &config, resolve(&server), &flags))
                .await;
            assert_eq!(run.code, code, "{case} {flags:?}: {}", run.dump());
            assert!(run.stderr.contains(MISSING), "{case}: {}", run.dump());
            if code == 2 {
                assert_eq!(
                    before,
                    server.total_requests(),
                    "{case}: kein Request vor dem Abbruch"
                );
            } else {
                assert!(
                    server.total_requests() > before,
                    "{case}: Crawl läuft ohne PSI weiter"
                );
            }
        }
    }
}

// spec: INTEG-130
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_without_key_fails_require_apis_as_before_despite_lighthouse_path() {
    let server = site("out-basic-site").await;
    let env = Env::new();
    let script = install(&env);
    let config = profile(&env, false, Some(script.clone()), None);
    let before = server.total_requests();
    let run = env
        .run_async(args(
            &list_file(&env),
            &config,
            resolve(&server),
            &["--require-apis"],
        ))
        .await;
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(!run.stderr.contains(MISSING), "{}", run.dump());
    assert_eq!(before, server.total_requests());
    assert!(
        runs(&script).is_empty(),
        "Remote darf Lighthouse nicht starten"
    );
}
