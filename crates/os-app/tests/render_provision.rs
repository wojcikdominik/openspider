//! M3-K01: Commands `chromium.*` mit dem echten HTTP-Transport der App gegen den os-testsite-Mock
//! (`mocks::chromium`), über den app-weiten Proxy (OUT-269, CRAWL-122).
use std::{
    io::{Cursor, Write},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use os_app::{
    commands::render_provision::{self, Overrides},
    App, AppConfig, CallCtx,
};
use os_render::provision::{install::sha256_hex as sha256, Manifest, Platform};
use os_testsite::mocks::{chromium::ChromiumMock, MockServer};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PLATFORM: Platform = Platform::Win64;
const VERSION: &str = "156.0.8078.80";

fn archive() -> Vec<u8> {
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        "chrome-win64/chrome.exe",
        SimpleFileOptions::default().unix_permissions(0o755),
    )
    .unwrap();
    zip.write_all(b"fixture chromium").unwrap();
    zip.start_file("chrome-win64/resources.pak", SimpleFileOptions::default())
        .unwrap();
    // Nicht komprimierbar (lineare Kongruenz), damit Abbruch und Fortsetzung mehrere Stücke sehen.
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let noise: Vec<u8> = (0..200 * 1024)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            (state >> 56) as u8
        })
        .collect();
    zip.write_all(&noise).unwrap();
    zip.finish().unwrap().into_inner()
}

fn manifest(url: String, bytes: &[u8], sha: Option<String>) -> Manifest {
    Manifest::parse(
        &json!({
            "schema": 1, "product": "Chrome for Testing", "version": VERSION, "source": "fixture",
            "platforms": { PLATFORM.key(): { "url": url, "size": bytes.len(), "sha256": sha.unwrap_or_else(|| sha256(bytes)) } }
        })
        .to_string(),
    )
    .unwrap()
}

fn app(home: &Path, manifest: Manifest) -> App {
    let app = App::new(AppConfig::new(home)).unwrap();
    render_provision::set_overrides(
        &app,
        Overrides {
            manifest: Some(manifest),
            platform: Some(Some(PLATFORM)),
            environment: Some(None),
            system: Some(None),
            ..Default::default()
        },
    );
    app
}

async fn call(app: &App, name: &str) -> Value {
    app.dispatch(CallCtx::for_window("main"), name, json!({}))
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

/// Pollt den Status, bis kein Download mehr läuft.
async fn settled(app: &App) -> Value {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let status = call(app, "chromium.status").await;
            let job = &status["job"];
            if !(job.get("downloading").is_some() || job == "verifying" || job == "installing") {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Download endet")
}

/// Weiterleitender HTTP-Proxy nur für Loopback: protokolliert den Anfragekopf und reicht die Verbindung durch.
struct ForwardProxy {
    port: u16,
    log: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl ForwardProxy {
    async fn start(target: std::net::SocketAddr) -> ForwardProxy {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let log = Arc::new(Mutex::new(Vec::new()));
        let requests = log.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut client, _)) = listener.accept().await {
                let requests = requests.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    while !head.ends_with(b"\r\n\r\n") && head.len() < 16384 {
                        let mut byte = [0];
                        if client.read_exact(&mut byte).await.is_err() {
                            return;
                        }
                        head.push(byte[0]);
                    }
                    requests
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&head).into_owned());
                    let Ok(mut upstream) = tokio::net::TcpStream::connect(target).await else {
                        return;
                    };
                    if upstream.write_all(&head).await.is_ok() {
                        let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                    }
                });
            }
        });
        ForwardProxy { port, log, task }
    }
    fn requests(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}
impl Drop for ForwardProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

// spec: OUT-260 [M3]
// spec: CRAWL-078 [M3]
#[tokio::test]
async fn download_through_app_proxy_installs_and_becomes_active() {
    let bytes = archive();
    let mock = ChromiumMock::new(bytes.clone());
    let server = MockServer::start(vec![mock.clone().into_api()])
        .await
        .unwrap();
    let proxy = ForwardProxy::start(server.addr()).await;
    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/archive.zip", server.base_url("chromium"));
    let app = app(dir.path(), manifest(url.clone(), &bytes, None));
    app.apply_settings_patch(&json!({"network":{"proxy":{
        "mode":"manual","kind":"http","host":"127.0.0.1","port":proxy.port,"bypass":[]
    }}}))
    .unwrap();

    let before = call(&app, "chromium.status").await;
    assert_eq!(before["supported"], true);
    assert_eq!(before["version"], VERSION);
    assert_eq!(before["downloadBytes"], bytes.len());
    assert_eq!(before["installed"], Value::Null);
    assert_eq!(before["active"], "none");
    assert_eq!(before["job"], "idle");

    let started = call(&app, "chromium.download").await;
    assert!(started["job"].get("downloading").is_some(), "{started}");
    let after = settled(&app).await;
    assert_eq!(after["job"], "idle", "{after}");
    assert_eq!(after["installed"], VERSION);
    assert_eq!(after["active"], "provisioned");
    let exe = dir
        .path()
        .join("browser")
        .join(VERSION)
        .join(PLATFORM.executable());
    assert!(exe.is_file());
    assert_eq!(after["activePath"], exe.display().to_string());
    // Der Download lief über den app-weiten Proxy (absolute URL im Anfragekopf an den Proxy).
    let logged = proxy.requests();
    assert!(
        logged.iter().any(|h| h.starts_with(&format!("GET {url} "))),
        "{logged:?}"
    );
    assert_eq!(mock.requests().len(), 1);
    // Ein konfigurierter Pfad hat Vorrang vor dem bereitgestellten Chromium (Auswahlreihenfolge K3-01).
    let configured = dir.path().join("own-chrome");
    std::fs::write(&configured, b"x").unwrap();
    let status = app
        .dispatch(
            CallCtx::for_window("main"),
            "chromium.status",
            json!({"configuredPath": configured.display().to_string()}),
        )
        .await
        .unwrap();
    assert_eq!(status["active"], "configured");
    server.stop().await;
}

fn set_crawl_state(app: &App, dir: &Path, state: os_model::CrawlState) {
    use os_app::{pool::CrawlKey, runtime::sessions::SessionInfo, WindowId};
    app.sessions().set_current(
        WindowId("main".into()),
        SessionInfo {
            key: CrawlKey("fixture".into()),
            path: dir.join("fixture.ospider"),
            config: Arc::new(os_model::CrawlConfig::default()),
            state,
            writable: true,
            engine: None,
        },
    );
}

async fn refused(app: &App, name: &str) -> os_app::CmdError {
    app.dispatch(CallCtx::for_window("main"), name, json!({}))
        .await
        .expect_err(name)
}

// spec: CRAWL-078 [M3]
// spec: OUT-260 [M3]
#[tokio::test]
async fn running_crawl_blocks_replacing_or_removing_an_installed_chromium() {
    use os_app::ErrorCode;
    use os_model::CrawlState;
    let bytes = archive();
    let mock = ChromiumMock::new(bytes.clone());
    let server = MockServer::start(vec![mock.clone().into_api()])
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(
        dir.path(),
        manifest(
            format!("{}/archive.zip", server.base_url("chromium")),
            &bytes,
            None,
        ),
    );
    app.apply_settings_patch(&json!({"network":{"proxy":{"mode":"none"}}}))
        .unwrap();

    // Ohne Installation nutzt kein Crawl ein bereitgestelltes Chromium: Download ist frei.
    set_crawl_state(&app, dir.path(), CrawlState::Running);
    call(&app, "chromium.download").await;
    assert_eq!(settled(&app).await["installed"], VERSION);

    // Mit Installation sperrt ein laufender oder pausierter Crawl Ersetzen und Entfernen.
    for state in [CrawlState::Running, CrawlState::Paused] {
        set_crawl_state(&app, dir.path(), state);
        for name in ["chromium.download", "chromium.remove"] {
            let err = refused(&app, name).await;
            assert_eq!(err.code, ErrorCode::CrawlRunning, "{name} {state:?}");
            assert_eq!(err.message_key, "err-chromium-crawl-running");
        }
        let status = call(&app, "chromium.status").await;
        assert_eq!(status["job"], "idle", "{status}");
        assert_eq!(status["installed"], VERSION);
    }
    assert_eq!(mock.requests().len(), 1);

    // Nach dem Crawl ist Entfernen wieder möglich.
    set_crawl_state(&app, dir.path(), CrawlState::Completed);
    let removed = call(&app, "chromium.remove").await;
    assert_eq!(removed["installed"], Value::Null);
    server.stop().await;
}

// spec: OUT-265 [M3]
#[tokio::test]
async fn wrong_checksum_fails_visibly_and_installs_nothing() {
    let bytes = archive();
    let server = MockServer::start(vec![ChromiumMock::new(bytes.clone()).into_api()])
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(
        dir.path(),
        manifest(
            format!("{}/archive.zip", server.base_url("chromium")),
            &bytes,
            Some(sha256(b"tampered")),
        ),
    );
    app.apply_settings_patch(&json!({"network":{"proxy":{"mode":"none"}}}))
        .unwrap();
    call(&app, "chromium.download").await;
    let status = settled(&app).await;
    assert_eq!(
        status["job"]["failed"]["messageKey"], "err-chromium-checksum",
        "{status}"
    );
    assert_eq!(status["installed"], Value::Null);
    assert_eq!(status["partialBytes"], 0);
    assert_eq!(status["active"], "none");
    assert!(!dir.path().join("browser").join(VERSION).exists());
    server.stop().await;
}

// spec: OUT-260 [M3]
#[tokio::test]
async fn cancel_keeps_partial_resume_completes_and_remove_cleans_up() {
    let bytes = archive();
    let mock = ChromiumMock::new(bytes.clone());
    let server = MockServer::start(vec![mock.clone().into_api()])
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(
        dir.path(),
        manifest(
            format!("{}/stall/archive.zip", server.base_url("chromium")),
            &bytes,
            None,
        ),
    );
    app.apply_settings_patch(&json!({"network":{"proxy":{"mode":"none"}}}))
        .unwrap();
    call(&app, "chromium.download").await;
    // Warten, bis die erste Hälfte da ist (danach stockt der Mock), dann abbrechen.
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let status = call(&app, "chromium.status").await;
            if status["job"]["downloading"]["done"].as_u64() >= Some(bytes.len() as u64 / 2) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    call(&app, "chromium.cancel").await;
    let cancelled = settled(&app).await;
    assert_eq!(
        cancelled["job"]["failed"]["messageKey"], "err-chromium-cancelled",
        "{cancelled}"
    );
    assert_eq!(cancelled["partialBytes"], bytes.len() / 2);
    assert_eq!(cancelled["installed"], Value::Null);

    call(&app, "chromium.download").await;
    let done = settled(&app).await;
    assert_eq!(done["installed"], VERSION, "{done}");
    assert_eq!(
        mock.requests()[1].range.as_deref(),
        Some(&*format!("bytes={}-", bytes.len() / 2))
    );

    let removed = call(&app, "chromium.remove").await;
    assert_eq!(removed["installed"], Value::Null);
    assert_eq!(removed["active"], "none");
    assert_eq!(removed["job"], "idle");
    assert!(!dir.path().join("browser").exists());
    server.stop().await;
}

#[tokio::test]
async fn unsupported_platform_reports_and_refuses_download() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    render_provision::set_overrides(
        &app,
        Overrides {
            platform: Some(Some(Platform::Linux64)),
            environment: Some(None),
            system: Some(None),
            ..Default::default()
        },
    );
    let status = call(&app, "chromium.status").await;
    assert_eq!(status["supported"], false);
    assert_eq!(status["version"], Manifest::bundled().unwrap().version);
    let error = app
        .dispatch(CallCtx::for_window("main"), "chromium.download", json!({}))
        .await
        .unwrap_err();
    assert_eq!(error.message_key, "err-chromium-unsupported");
}

/// Manuelle Prüfung der echten Archive: lädt das Archiv der Versionsdatei für `OPENSPIDER_K01_PLATFORM`
/// (`mac-arm64`, `mac-x64`, `win64`) über den App-Transport, prüft SHA-256 und entpackt es.
/// Kann in keiner Testsuite laufen: externes Netz (M2.md 7, „kein Test spricht ein externes Netz an“), ca. 200 MB je
/// Plattform und eine Plattformwahl per Umgebungsvariable. Dieselbe Logik gegen den lokalen Mock prüfen
/// `os-render/tests/provision_download.rs` und die übrigen Tests dieser Datei; Erstlauf 03.11.2026 in `M3-reports/k01.md`.
#[tokio::test]
#[ignore = "Manuell: OUT-260 – Protokoll M3-Q03 (echte Chrome-for-Testing-Archive über externes Netz, ca. 200 MB je Plattform)"]
async fn real_archive_from_versions_file() {
    let platform = std::env::var("OPENSPIDER_K01_PLATFORM")
        .ok()
        .and_then(|k| Platform::from_key(&k))
        .expect("OPENSPIDER_K01_PLATFORM");
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig::new(dir.path())).unwrap();
    render_provision::set_overrides(
        &app,
        Overrides {
            platform: Some(Some(platform)),
            environment: Some(None),
            system: Some(None),
            ..Default::default()
        },
    );
    call(&app, "chromium.download").await;
    let status = tokio::time::timeout(Duration::from_secs(1800), async {
        loop {
            let status = call(&app, "chromium.status").await;
            let job = &status["job"];
            if !(job.get("downloading").is_some() || job == "verifying" || job == "installing") {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        status["installed"],
        Manifest::bundled().unwrap().version,
        "{status}"
    );
    let exe = dir
        .path()
        .join("browser")
        .join(&Manifest::bundled().unwrap().version)
        .join(platform.executable());
    assert!(exe.is_file(), "{}", exe.display());
    eprintln!("{} ok: {}", platform.key(), exe.display());
}
