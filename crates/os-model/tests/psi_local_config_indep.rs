//! Unabhängige Tests (nur aus der Spezifikation) zur Konfiguration der PageSpeed-Quelle „Lokal“:
//! Standardquelle Remote (INTEG-130), optionaler Pfad zu einer vorhandenen Lighthouse-Installation (INTEG-132),
//! Rundreise `.osconfig` gegen die Fixtures unter `tests/fixtures/integ-psi-local/`.
use std::path::{Path, PathBuf};

use os_model::{CrawlConfig, PsiDevice};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("integ-psi-local")
        .join(name)
}

fn lighthouse_issues(cfg: &CrawlConfig) -> Vec<String> {
    cfg.validate()
        .errors
        .into_iter()
        .filter(|i| i.path == "api.psi.lighthouse_path")
        .map(|i| i.message)
        .collect()
}

// spec: INTEG-130
#[test]
fn new_profile_uses_remote_without_lighthouse_path() {
    let cfg = CrawlConfig::default();
    assert!(!cfg.api.psi.local, "Standardquelle muss Remote sein");
    assert_eq!(cfg.api.psi.lighthouse_path, None);
    assert!(lighthouse_issues(&cfg).is_empty());
}

// spec: INTEG-132
#[test]
fn local_fixture_with_spaces_loads_and_round_trips_unchanged() {
    let loaded = CrawlConfig::load_file(&fixture("local-spaces.osconfig")).unwrap();
    let cfg = loaded.config;
    assert!(
        loaded.warnings.is_empty(),
        "unerwartete Warnungen: {:?}",
        loaded.warnings
    );
    assert!(cfg.api.psi.enabled);
    assert!(cfg.api.psi.local);
    assert_eq!(cfg.api.psi.device, PsiDevice::Desktop);
    assert_eq!(
        cfg.api.psi.lighthouse_path,
        Some(PathBuf::from(
            "/opt/Light House/node_modules/lighthouse/cli/index.js"
        ))
    );
    assert!(cfg.validate().is_ok(), "{:?}", cfg.validate().errors);

    // Schreiben → Laden → Schreiben: Pfad (mit Leerzeichen) und Quelle bleiben erhalten, Text byte-identisch.
    let text = cfg.to_osconfig_string().unwrap();
    assert!(text.contains("lighthouse_path"), "{text}");
    let again = CrawlConfig::load_str(&text).unwrap().config;
    assert_eq!(again.api.psi, cfg.api.psi);
    assert_eq!(again.to_osconfig_string().unwrap(), text);
}

// spec: INTEG-130
// spec: INTEG-132
#[test]
fn remote_fixture_keeps_a_stored_path_without_switching_to_local() {
    let cfg = CrawlConfig::load_file(&fixture("remote-with-path.osconfig"))
        .unwrap()
        .config;
    assert!(cfg.api.psi.enabled);
    assert!(
        !cfg.api.psi.local,
        "Pfad allein darf nicht auf Lokal umstellen"
    );
    assert_eq!(
        cfg.api.psi.lighthouse_path,
        Some(PathBuf::from("/usr/local/bin/lighthouse"))
    );
    assert!(cfg.validate().is_ok(), "{:?}", cfg.validate().errors);
    let again = CrawlConfig::load_str(&cfg.to_osconfig_string().unwrap())
        .unwrap()
        .config;
    assert_eq!(again.api.psi, cfg.api.psi);
}

// spec: INTEG-132
#[test]
fn local_without_path_round_trips_as_not_configured() {
    let cfg = CrawlConfig::load_str("[api.psi]\nenabled = true\nlocal = true\n")
        .unwrap()
        .config;
    assert!(cfg.api.psi.local);
    assert_eq!(cfg.api.psi.lighthouse_path, None);
    assert!(lighthouse_issues(&cfg).is_empty());
    let again = CrawlConfig::load_str(&cfg.to_osconfig_string().unwrap())
        .unwrap()
        .config;
    assert_eq!(again.api.psi.lighthouse_path, None);
    assert!(again.api.psi.local);
}

// spec: INTEG-132
#[test]
fn relative_path_is_kept_verbatim() {
    let cfg = CrawlConfig::load_str(
        "[api.psi]\nlocal = true\nlighthouse_path = \"tools/lighthouse/cli/index.js\"\n",
    )
    .unwrap()
    .config;
    assert_eq!(
        cfg.api.psi.lighthouse_path,
        Some(PathBuf::from("tools/lighthouse/cli/index.js"))
    );
    let again = CrawlConfig::load_str(&cfg.to_osconfig_string().unwrap())
        .unwrap()
        .config;
    assert_eq!(again.api.psi.lighthouse_path, cfg.api.psi.lighthouse_path);
}

// spec: INTEG-132
#[test]
fn blank_lighthouse_path_is_reported_on_its_field() {
    for blank in ["", "   ", "\t"] {
        let mut cfg = CrawlConfig::default();
        cfg.api.psi.local = true;
        cfg.api.psi.lighthouse_path = Some(PathBuf::from(blank));
        assert_eq!(
            lighthouse_issues(&cfg).len(),
            1,
            "leerer Pfad {blank:?} muss am Feld api.psi.lighthouse_path gemeldet werden"
        );
    }
    let mut cfg = CrawlConfig::default();
    cfg.api.psi.local = true;
    cfg.api.psi.lighthouse_path = Some(PathBuf::from("/opt/Light House/lighthouse"));
    assert!(lighthouse_issues(&cfg).is_empty());
}
