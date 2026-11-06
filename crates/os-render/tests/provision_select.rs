//! M3-K01: Auswahlreihenfolge nach K3-01 – konfigurierter Pfad → `OPENSPIDER_CHROMIUM` → bereitgestelltes Chromium →
//! System-Chrome – und klare Meldung ohne Chromium.
use std::path::{Path, PathBuf};

use os_render::provision::{self, install, Lookup, Platform, Source};
use os_render::RenderError;

const PLATFORM: Platform = Platform::Win64;

fn file(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"fixture").unwrap();
    path.to_path_buf()
}

/// Bereitgestellte Ablage im Format von `provision::install` (Versionsordner + `current`).
fn provisioned(root: &Path, version: &str) -> PathBuf {
    let exe = file(&root.join(version).join(PLATFORM.executable()));
    std::fs::write(root.join(install::CURRENT), version).unwrap();
    exe
}

struct Fixture {
    _dir: tempfile::TempDir,
    configured: PathBuf,
    environment: PathBuf,
    root: PathBuf,
    provisioned: PathBuf,
    system: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("browser");
    Fixture {
        configured: file(&dir.path().join("configured").join("chrome")),
        environment: file(&dir.path().join("env").join("chrome")),
        provisioned: provisioned(&root, "156.0.8078.80"),
        system: file(&dir.path().join("system").join("chrome")),
        root,
        _dir: dir,
    }
}

impl Fixture {
    fn lookup(&self, configured: bool, environment: bool, root: bool) -> Lookup {
        Lookup {
            configured: configured.then(|| self.configured.clone()),
            environment: environment.then(|| self.environment.clone()),
            root: root.then(|| self.root.clone()),
            platform: Some(PLATFORM),
        }
    }
}

// spec: CRAWL-078 [M3]
#[test]
fn selection_order_configured_environment_provisioned_system() {
    let f = fixture();
    let system = || Some(f.system.clone());
    for (lookup, path, source) in [
        (
            f.lookup(true, true, true),
            &f.configured,
            Source::Configured,
        ),
        (
            f.lookup(false, true, true),
            &f.environment,
            Source::Environment,
        ),
        (
            f.lookup(false, false, true),
            &f.provisioned,
            Source::Provisioned,
        ),
        (f.lookup(false, false, false), &f.system, Source::System),
    ] {
        let found = provision::resolve_with(&lookup, system).unwrap();
        assert_eq!((&found.path, found.source), (path, source));
    }
}

// spec: CRAWL-077 [P1] [M2]
#[test]
fn explicit_missing_path_never_falls_back() {
    let f = fixture();
    let mut lookup = f.lookup(true, false, true);
    lookup.configured = Some(f.root.join("missing-chrome"));
    let error = provision::resolve_with(&lookup, || Some(f.system.clone())).unwrap_err();
    assert!(
        matches!(&error, RenderError::ChromiumMissing { path } if path.contains("missing-chrome"))
    );
    let mut lookup = f.lookup(false, false, true);
    lookup.environment = Some(f.root.join("missing-env-chrome"));
    let error = provision::resolve_with(&lookup, || Some(f.system.clone())).unwrap_err();
    assert!(
        matches!(&error, RenderError::ChromiumMissing { path } if path.contains("missing-env-chrome"))
    );
}

// spec: CRAWL-077 [P1] [M2]
#[test]
fn without_any_chromium_the_message_names_download_and_path() {
    let dir = tempfile::tempdir().unwrap();
    let lookup = Lookup {
        root: Some(dir.path().join("browser")),
        platform: Some(PLATFORM),
        ..Default::default()
    };
    let error = provision::resolve_with(&lookup, || None).unwrap_err();
    let text = error.to_string();
    assert!(text.starts_with("err-chromium-missing:"), "{text}");
    assert!(text.contains("herunterladen"), "{text}");
    assert!(text.contains("rendering.chromium_path"), "{text}");
    assert!(text.contains("OPENSPIDER_CHROMIUM"), "{text}");
}

// spec: CRAWL-078 [M3]
#[test]
fn incomplete_or_foreign_provisioning_is_skipped() {
    let f = fixture();
    let system = || Some(f.system.clone());
    // `current` zeigt auf eine Version ohne Programm (z. B. abgebrochenes Entfernen) → System-Chrome.
    std::fs::write(f.root.join(install::CURRENT), "157.0.0.1").unwrap();
    let found = provision::resolve_with(&f.lookup(false, false, true), system).unwrap();
    assert_eq!(found.source, Source::System);
    // Ungültiger Versionsname im Zeiger (Pfadangriff) → ignoriert.
    std::fs::write(f.root.join(install::CURRENT), "../configured").unwrap();
    let found = provision::resolve_with(&f.lookup(false, false, true), system).unwrap();
    assert_eq!(found.source, Source::System);
    // Ablage einer anderen Plattform zählt nicht.
    std::fs::write(f.root.join(install::CURRENT), "156.0.8078.80").unwrap();
    let mut other = f.lookup(false, false, true);
    other.platform = Some(Platform::MacArm64);
    let found = provision::resolve_with(&other, system).unwrap();
    assert_eq!(found.source, Source::System);
    // Plattform ohne Bereitstellung (z. B. Linux arm64) → nur Pfad oder System.
    other.platform = None;
    let found = provision::resolve_with(&other, system).unwrap();
    assert_eq!(found.source, Source::System);
}

#[test]
fn default_root_follows_openspider_home() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(
        provision::root_for_home(Some(home.path().as_os_str())),
        Some(home.path().join("browser"))
    );
    let fallback = provision::root_for_home(Some(std::ffi::OsStr::new("")));
    assert!(fallback.is_none_or(|p| p.ends_with(Path::new("OpenSpider").join("browser"))));
}

/// Echter Lauf: Ein bereitgestelltes Chromium (Ablage unter `OPENSPIDER_HOME`) wird ohne Pfadangabe gefunden und
/// rendert `/csr.html`. Kindprozess, damit `OPENSPIDER_HOME`/`OPENSPIDER_CHROMIUM` nur dort gelten. Linux, weil das
/// Programm per Symlink in die Ablage gelegt wird (Chromium findet seine Dateien über `/proc/self/exe`).
// spec: CRAWL-078 [M3]
#[cfg(target_os = "linux")]
#[test]
fn provisioned_chromium_renders_without_configured_path() {
    const CHILD: &str = "OPENSPIDER_K01_PROVISIONED_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let found = provision::resolve(None).unwrap();
            assert_eq!(found.source, Source::Provisioned, "{found:?}");
            let site = os_testsite::TestSite::start("render-r01").await.unwrap();
            let renderer = os_render::Renderer::launch(os_render::LaunchOpts::default())
                .await
                .expect("bereitgestelltes Chromium startet");
            let page = renderer
                .render(
                    &site.url("/csr.html"),
                    &os_render::RenderOpts {
                        ajax_timeout_ms: 100,
                        screenshots: false,
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            assert!(page.dom_html.contains("<title>rendered</title>"));
            renderer.close().await;
        });
        return;
    }
    // Echtes Chromium dieses Rechners (Pflicht wie in render_chromium.rs: fehlt es, ist das ein Fehler).
    let real = provision::resolve(None)
        .expect("requires_chromium: install Chrome or set OPENSPIDER_CHROMIUM")
        .path;
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("browser");
    let exe = root
        .join("156.0.8078.80")
        .join(Platform::Linux64.executable());
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(std::fs::canonicalize(&real).unwrap(), &exe).unwrap();
    std::fs::write(root.join(install::CURRENT), "156.0.8078.80").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "provisioned_chromium_renders_without_configured_path",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("OPENSPIDER_HOME", home.path())
        .env_remove("OPENSPIDER_CHROMIUM")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}
