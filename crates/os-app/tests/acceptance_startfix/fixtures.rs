//! Fixtures der Abnahmetests B4-STARTFIX: robots.txt-Varianten als temporäre Szenario-Ordner für `os-testsite`.
#![allow(dead_code)]

/// Sperrende Zeile der robots.txt aus [`robots_fixture`] (Regeltext und Zeilennummer).
pub const RULE: &str = "Disallow: /secret/";
pub const RULE_LINE: u64 = 7;

/// Ordner mit mehrgruppiger robots.txt. Die sperrende Zeile ist absichtlich nicht die zweite (wie in `ui/fx-ui-robots-start`),
/// damit eine fest verdrahtete Zeilennummer auffällt:
///
/// ```text
/// 1 # Testdatei
/// 2 User-agent: Googlebot
/// 3 Disallow: /g
/// 4
/// 5 User-agent: *
/// 6 Allow: /secret/open.html
/// 7 Disallow: /secret/
/// ```
///
/// `/` ist erlaubt und verlinkt `/open.html` sowie das gesperrte `/secret/index.html`; `/secret/open.html` ist per `Allow`
/// erlaubt.
pub fn robots_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("scenario.toml"), "# robots fixture\n").unwrap();
    std::fs::write(
        root.join("robots.txt"),
        "# Testdatei\nUser-agent: Googlebot\nDisallow: /g\n\nUser-agent: *\nAllow: /secret/open.html\nDisallow: /secret/\n",
    )
    .unwrap();
    let page = |title: &str, links: &str| {
        format!("<!doctype html><html><head><title>{title}</title></head><body><h1>{title}</h1>{links}</body></html>")
    };
    std::fs::write(
        root.join("index.html"),
        page(
            "Start",
            "<a href=\"/open.html\">open</a> <a href=\"/secret/index.html\">secret</a>",
        ),
    )
    .unwrap();
    std::fs::write(root.join("open.html"), page("Open", "")).unwrap();
    std::fs::create_dir_all(root.join("secret")).unwrap();
    std::fs::write(root.join("secret/index.html"), page("Secret", "")).unwrap();
    std::fs::write(root.join("secret/open.html"), page("Secret open", "")).unwrap();
    dir
}

/// robots.txt, die mit dem angegebenen Status (5xx, 429) antwortet; die Startseite `/` ist erreichbar.
pub fn robots_status_fixture(status: u16) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("scenario.toml"),
        format!("[[route]]\npath = \"/robots.txt\"\nstatus = {status}\nbody = \"kaputt\"\ncontent_type = \"text/plain\"\n\n[[route]]\npath = \"/\"\nbody = \"<html><head><title>x</title></head><body>x</body></html>\"\n"),
    )
    .unwrap();
    dir
}

pub fn robots_500_fixture() -> tempfile::TempDir {
    robots_status_fixture(500)
}
