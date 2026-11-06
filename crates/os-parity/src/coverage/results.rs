//! Testergebnisse einlesen: Ausgabe von `cargo test` und JUnit-XML von `cargo nextest`.

use crate::error::ParityError;

/// Ergebnis eines Tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// `ok`.
    Ok,
    /// `FAILED`.
    Failed,
    /// `ignored`, ggf. mit dem von libtest ausgegebenen Grund.
    Ignored(Option<String>),
}

/// Art des Nextest-Binärs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextestKind {
    /// Bibliothek (Unit-Tests).
    Lib,
    /// Binär `bin/<name>`.
    Bin(String),
    /// Integrationstest `<name>`.
    Test(String),
}

/// Test-Binär, in dem ein Test lief.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryRef {
    /// Aus `Running …`-Zeilen von `cargo test`.
    Cargo {
        /// `unittests <pfad>` (sonst Integrationstest/Benchmark).
        unit: bool,
        /// Pfad der Wurzeldatei relativ zum Paket, wie ausgegeben (`src/lib.rs`, `tests/x.rs`).
        rel: String,
        /// Name des Ziels (aus `deps/<name>-<hash>`, `-` → `_`).
        name: String,
    },
    /// Aus der JUnit-Datei von `cargo nextest` (Binär-ID `paket::kind`).
    Nextest {
        /// Paketname.
        package: String,
        /// Art.
        kind: NextestKind,
    },
}

/// Ein Testergebnis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestResult {
    /// Binär.
    pub binary: BinaryRef,
    /// Testpfad (`module::pfad::name`).
    pub path: String,
    /// Ergebnis.
    pub outcome: Outcome,
    /// Laufende Nummer des Test-Binär-Blocks (`Running …`-Zeile bzw. `<testsuite>`).
    pub block: usize,
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_running(rest: &str) -> Option<BinaryRef> {
    let open = rest.rfind(" (")?;
    let desc = rest[..open].trim();
    let path = rest[open + 2..].trim_end().strip_suffix(')')?;
    let file = path.rsplit(['/', '\\']).next()?;
    let file = file.strip_suffix(".exe").unwrap_or(file);
    let name = file.rsplit_once('-').map_or(file, |(n, _)| n);
    let (unit, rel) = match desc.strip_prefix("unittests ") {
        Some(r) => (true, r.trim()),
        None => (false, desc),
    };
    Some(BinaryRef::Cargo {
        unit,
        rel: rel.replace('\\', "/"),
        name: name.replace('-', "_"),
    })
}

fn parse_outcome(text: &str) -> Option<Outcome> {
    let t = text.trim();
    if t == "ok" || t.starts_with("ok ") {
        Some(Outcome::Ok)
    } else if t.starts_with("FAILED") {
        Some(Outcome::Failed)
    } else if let Some(rest) = t.strip_prefix("ignored") {
        let reason = rest.trim_start_matches(',').trim().to_string();
        Some(Outcome::Ignored((!reason.is_empty()).then_some(reason)))
    } else {
        None
    }
}

/// Wertet die Standardausgabe von `cargo test --workspace … --no-fail-fast` aus.
/// Doc-Tests und Benchmarks werden übersprungen.
pub fn parse_cargo_output(text: &str) -> Vec<TestResult> {
    let mut results = Vec::new();
    let mut current: Option<BinaryRef> = None;
    let mut block = 0usize;
    for raw in text.lines() {
        let line = strip_ansi(raw);
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Running ") {
            block += 1;
            current = parse_running(rest);
        } else if line.starts_with("Doc-tests ") {
            current = None;
        } else if let Some(rest) = line.strip_prefix("test ") {
            let Some((name, outcome)) = rest.split_once(" ... ") else {
                continue;
            };
            let (Some(binary), Some(outcome)) = (current.clone(), parse_outcome(outcome)) else {
                continue;
            };
            let path = name.strip_suffix(" - should panic").unwrap_or(name).trim();
            results.push(TestResult {
                binary,
                path: path.to_string(),
                outcome,
                block,
            });
        }
    }
    results
}

fn unescape_xml(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find(';').map(|i| start + i) else {
            out.push_str(&rest[start..]);
            return out;
        };
        let entity = &rest[start + 1..end];
        let decoded = match entity {
            "quot" => Some('"'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            _ => entity
                .strip_prefix("#x")
                .and_then(|n| u32::from_str_radix(n, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse().ok()))
                .and_then(char::from_u32),
        };
        if let Some(c) = decoded {
            out.push(c);
        } else {
            out.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

struct XmlTag {
    name: String,
    attrs: Vec<(String, String)>,
    closing: bool,
    self_closing: bool,
}

impl XmlTag {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

fn parse_tag(body: &str) -> Option<XmlTag> {
    let mut s = body.trim();
    let closing = s.starts_with('/');
    if closing {
        s = s[1..].trim_start();
    }
    let self_closing = s.ends_with('/');
    if self_closing {
        s = s[..s.len() - 1].trim_end();
    }
    let name_end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
    let name = s[..name_end].to_string();
    if name.is_empty() {
        return None;
    }
    let mut attrs = Vec::new();
    let mut rest = s[name_end..].trim_start();
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().to_string();
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '-' | '.'))
        {
            return None;
        }
        let after = rest[eq + 1..].trim_start();
        let quote = after.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let close = after[1..].find(quote)?;
        attrs.push((key, unescape_xml(&after[1..1 + close])));
        rest = after[close + 2..].trim_start();
    }
    if !rest.is_empty() || (closing && !attrs.is_empty()) {
        return None;
    }
    Some(XmlTag {
        name,
        attrs,
        closing,
        self_closing,
    })
}

fn nextest_binary(id: &str) -> BinaryRef {
    match id.split_once("::") {
        None => BinaryRef::Nextest {
            package: id.to_string(),
            kind: NextestKind::Lib,
        },
        Some((package, rest)) => {
            let kind = match rest.strip_prefix("bin/") {
                Some(b) => NextestKind::Bin(b.to_string()),
                None => NextestKind::Test(rest.to_string()),
            };
            BinaryRef::Nextest {
                package: package.to_string(),
                kind,
            }
        }
    }
}

/// Wertet eine JUnit-Datei von `cargo nextest` aus (`<testcase>` mit `<failure>`/`<error>`/`<skipped>`).
pub fn parse_junit(xml: &str) -> Result<Vec<TestResult>, ParityError> {
    let mut results = Vec::new();
    let mut suite = String::new();
    let mut case: Option<(BinaryRef, String, Outcome, usize)> = None;
    let mut block = 0usize;
    let mut rest = xml;
    let mut saw_testsuite = false;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        if let Some(r) = rest.strip_prefix("!--") {
            rest = r.split_once("-->").map_or("", |(_, t)| t);
            continue;
        }
        if let Some(r) = rest.strip_prefix("![CDATA[") {
            rest = r.split_once("]]>").map_or("", |(_, t)| t);
            continue;
        }
        let Some(gt) = find_tag_end(rest) else {
            return Err(ParityError::Invalid(
                "JUnit-XML: Tag nicht geschlossen".into(),
            ));
        };
        let body = &rest[..gt];
        rest = &rest[gt + 1..];
        if body.starts_with('?') || body.starts_with('!') {
            continue;
        }
        let Some(tag) = parse_tag(body) else {
            return Err(ParityError::Invalid(format!(
                "JUnit-XML: unlesbarer Tag <{body}>"
            )));
        };
        match (tag.name.as_str(), tag.closing) {
            ("testsuite", false) => {
                saw_testsuite = true;
                block += 1;
                suite = tag.attr("name").unwrap_or("").to_string();
            }
            ("testcase", false) => {
                let id = tag
                    .attr("classname")
                    .filter(|c| !c.is_empty())
                    .unwrap_or(&suite);
                let entry = (
                    nextest_binary(id),
                    tag.attr("name").unwrap_or("").to_string(),
                    Outcome::Ok,
                    block,
                );
                if tag.self_closing {
                    results.push(finish(entry));
                } else {
                    case = Some(entry);
                }
            }
            ("failure" | "error", false) => {
                if let Some(c) = case.as_mut() {
                    c.2 = Outcome::Failed;
                }
            }
            ("skipped", false) => {
                if let Some(c) = case.as_mut() {
                    if c.2 != Outcome::Failed {
                        c.2 = Outcome::Ignored(None);
                    }
                }
            }
            ("testcase", true) => {
                if let Some(c) = case.take() {
                    results.push(finish(c));
                }
            }
            _ => {}
        }
    }
    if !saw_testsuite {
        return Err(ParityError::Invalid(
            "JUnit-XML ohne <testsuite>-Element".into(),
        ));
    }
    Ok(results)
}

fn finish(c: (BinaryRef, String, Outcome, usize)) -> TestResult {
    TestResult {
        binary: c.0,
        path: c.1,
        outcome: c.2,
        block: c.3,
    }
}

/// Position des `>` am Ende des Tags, unter Beachtung von Anführungszeichen in Attributwerten.
fn find_tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
   Compiling os-url v0.1.0
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.2s
     Running unittests src/lib.rs (target/debug/deps/os_url-0123456789abcdef)

running 3 tests
test normalize::tests::lowercase_host ... ok
test normalize::tests::broken ... FAILED
test slow ... ignored, M2: braucht Browser

test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out

     Running tests/basic.rs (target/debug/deps/basic-aaaaaaaaaaaaaaaa)

running 2 tests
test lowercase_host ... ok
test panics - should panic ... ok

     Running unittests src/main.rs (/abs/target/debug/deps/openspider-bbbb)
test cli_ok ... ok
   Doc-tests os_url

running 1 test
test src/lib.rs - parse (line 3) ... ok
";

    #[test]
    fn parses_binaries_and_outcomes() {
        let r = parse_cargo_output(SAMPLE);
        assert_eq!(r.len(), 6, "{r:#?}");
        assert_eq!(
            r[0].binary,
            BinaryRef::Cargo {
                unit: true,
                rel: "src/lib.rs".into(),
                name: "os_url".into()
            }
        );
        assert_eq!(r[0].path, "normalize::tests::lowercase_host");
        assert_eq!(r[0].outcome, Outcome::Ok);
        assert_eq!(r[1].outcome, Outcome::Failed);
        assert_eq!(
            r[2].outcome,
            Outcome::Ignored(Some("M2: braucht Browser".into()))
        );
        assert_eq!(
            r[3].binary,
            BinaryRef::Cargo {
                unit: false,
                rel: "tests/basic.rs".into(),
                name: "basic".into()
            }
        );
        assert_eq!(r[4].path, "panics", "' - should panic' wird entfernt");
        assert_eq!(
            r[5].binary,
            BinaryRef::Cargo {
                unit: true,
                rel: "src/main.rs".into(),
                name: "openspider".into()
            }
        );
    }

    #[test]
    fn doc_tests_and_summary_lines_are_ignored() {
        let r = parse_cargo_output(SAMPLE);
        assert!(r.iter().all(|t| !t.path.contains("line 3")));
        assert!(r.iter().all(|t| !t.path.starts_with("result")));
    }

    #[test]
    fn ansi_colors_and_windows_paths() {
        let text = "\u{1b}[32m     Running\u{1b}[0m unittests src\\lib.rs (C:\\t\\deps\\os_x-abc.exe)\ntest a::b ... \u{1b}[32mok\u{1b}[0m\n";
        let r = parse_cargo_output(text);
        assert_eq!(r.len(), 1);
        assert_eq!(
            r[0].binary,
            BinaryRef::Cargo {
                unit: true,
                rel: "src/lib.rs".into(),
                name: "os_x".into()
            }
        );
        assert_eq!(r[0].outcome, Outcome::Ok);
    }

    #[test]
    fn same_function_name_in_two_binaries_stays_separate() {
        let text = "     Running tests/a.rs (t/deps/a-1)\ntest same ... ok\n     Running tests/b.rs (t/deps/b-2)\ntest same ... FAILED\n";
        let r = parse_cargo_output(text);
        assert_eq!(r.len(), 2);
        assert_ne!(r[0].binary, r[1].binary);
        assert_eq!(r[1].outcome, Outcome::Failed);
    }

    const JUNIT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="4" failures="1">
  <testsuite name="os-url" tests="2" disabled="0" errors="0" failures="1">
    <testcase name="normalize::tests::ok" classname="os-url" time="0.01"/>
    <testcase name="normalize::tests::bad" classname="os-url" time="0.01">
      <failure message="assertion &quot;x&quot; failed" type="test failure"/>
    </testcase>
  </testsuite>
  <testsuite name="os-url::basic" tests="1">
    <testcase name="slow" classname="os-url::basic" time="0"><skipped/></testcase>
  </testsuite>
  <testsuite name="os-parity::bin/os-parity" tests="1">
    <testcase name="cli" classname="os-parity::bin/os-parity"><system-out><![CDATA[a < b > c]]></system-out></testcase>
  </testsuite>
</testsuites>
"#;

    #[test]
    fn junit_results() {
        let r = parse_junit(JUNIT).unwrap();
        assert_eq!(r.len(), 4);
        assert_eq!(
            r[0].binary,
            BinaryRef::Nextest {
                package: "os-url".into(),
                kind: NextestKind::Lib
            }
        );
        assert_eq!(r[0].outcome, Outcome::Ok);
        assert_eq!(r[1].outcome, Outcome::Failed);
        assert_eq!(
            r[2].binary,
            BinaryRef::Nextest {
                package: "os-url".into(),
                kind: NextestKind::Test("basic".into())
            }
        );
        assert_eq!(r[2].outcome, Outcome::Ignored(None));
        assert_eq!(
            r[3].binary,
            BinaryRef::Nextest {
                package: "os-parity".into(),
                kind: NextestKind::Bin("os-parity".into())
            }
        );
        assert_eq!(r[3].outcome, Outcome::Ok);
    }

    #[test]
    fn junit_without_testsuite_is_an_error() {
        assert!(parse_junit("<foo/>").is_err());
        assert!(parse_junit("<testsuite name=\"a\"").is_err());
    }

    // B4-M2CLOSE: keine Spec-ID (Infrastruktur M2-Q01). Form aus einem echten Playwright-JUnit (perf3d.spec.ts).
    #[test]
    fn playwright_skip_annotation_is_the_reason_of_an_empty_skipped_element() {
        let xml = "<testsuites><testsuite name=\"viz/perf3d.spec.ts\" hostname=\"chromium\">\
            <testcase name=\"messrezept\" classname=\"viz/perf3d.spec.ts\"><properties>\
            <property name=\"serial\" value=\"\"></property>\
            <property name=\"skip\" value=\"Benchmark: nur mit OS_VIZ_PERF3D=1\"></property>\
            </properties><skipped></skipped></testcase>\
            <testcase name=\"mit Nachricht\" classname=\"a.spec.ts\"><properties>\
            <property name=\"skip\" value=\"Eigenschaft\"></property></properties>\
            <skipped message=\"Nachricht\"/></testcase>\
            <testcase name=\"ohne Grund\" classname=\"a.spec.ts\"><skipped/></testcase>\
            <testcase name=\"lief\" classname=\"a.spec.ts\"></testcase>\
            </testsuite></testsuites>";
        let cases = super::frontend_cases(xml).unwrap();
        let outcomes: Vec<_> = cases.iter().map(|c| c.outcome.clone()).collect();
        assert_eq!(
            outcomes,
            vec![
                Outcome::Ignored(Some("Benchmark: nur mit OS_VIZ_PERF3D=1".into())),
                Outcome::Ignored(Some("Nachricht".into())),
                Outcome::Ignored(None),
                Outcome::Ok,
            ]
        );
    }
}

/// Frontend-JUnit-Fall; Dateipfad und Titel bleiben getrennt, um gleiche Titel sicher zuzuordnen.
#[derive(Debug, Clone)]
pub(crate) struct FrontendCase {
    pub file: String,
    pub suite: String,
    pub project: String,
    pub name: String,
    pub outcome: Outcome,
}

/// Liest vitest-/Playwright-JUnit. Ungültiges oder abgeschnittenes XML liefert keine Gutschriften.
/// Die XML-Reihenfolge bleibt erhalten: frontend::read_junit verwendet den letzten
/// identischen Fall als Retry-Ergebnis (block), bevor Template-Instanzen zusammenlaufen.
pub(crate) fn frontend_cases(xml: &str) -> Result<Vec<FrontendCase>, ParityError> {
    let invalid =
        || ParityError::Invalid("Frontend-JUnit: ungültiges oder unvollständiges XML".into());
    let mut rest = xml;
    let mut stack: Vec<String> = Vec::new();
    let mut suites: Vec<(String, String)> = Vec::new();
    let mut cases = Vec::new();
    let mut case: Option<FrontendCase> = None;
    // B4-M2CLOSE: Playwright schreibt den Grund von `test.skip(bedingung, grund)` nicht in `<skipped>`, sondern als
    // `<property name="skip" value="grund">` vor `<skipped>`. Ohne `message` gilt dieser Grund.
    let mut skip_note: Option<String> = None;
    let mut seen = false;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        if let Some(r) = rest.strip_prefix("!--") {
            rest = r.split_once("-->").ok_or_else(invalid)?.1;
            continue;
        }
        if let Some(r) = rest.strip_prefix("![CDATA[") {
            rest = r.split_once("]]>").ok_or_else(invalid)?.1;
            continue;
        }
        let gt = find_tag_end(rest).ok_or_else(invalid)?;
        let body = &rest[..gt];
        rest = &rest[gt + 1..];
        if body.starts_with('?') {
            continue;
        }
        let tag = parse_tag(body).ok_or_else(invalid)?;
        if tag.closing {
            if stack.pop().as_deref() != Some(&tag.name) {
                return Err(invalid());
            }
            if tag.name == "testcase" {
                cases.push(case.take().ok_or_else(invalid)?);
            } else if tag.name == "testsuite" {
                suites.pop();
            }
            continue;
        }
        match tag.name.as_str() {
            "testsuite" => {
                seen = true;
                if !tag.self_closing {
                    suites.push((
                        tag.attr("name").unwrap_or("").to_string(),
                        tag.attr("hostname").unwrap_or("").to_string(),
                    ));
                }
            }
            "testcase" => {
                if case.is_some() || suites.is_empty() {
                    return Err(invalid());
                }
                let c = FrontendCase {
                    file: tag
                        .attr("file")
                        .or_else(|| tag.attr("classname"))
                        .unwrap_or("")
                        .to_string(),
                    suite: suites.last().map(|s| s.0.clone()).unwrap_or_default(),
                    project: suites.last().map(|s| s.1.clone()).unwrap_or_default(),
                    name: tag.attr("name").ok_or_else(invalid)?.to_string(),
                    outcome: Outcome::Ok,
                };
                skip_note = None;
                if tag.self_closing {
                    cases.push(c);
                } else {
                    case = Some(c);
                }
            }
            "property" if case.is_some() && tag.attr("name") == Some("skip") => {
                skip_note = tag.attr("value").map(str::to_string);
            }
            "failure" | "error" => {
                if let Some(c) = &mut case {
                    c.outcome = Outcome::Failed;
                }
            }
            "skipped" => {
                if let Some(c) = &mut case {
                    if c.outcome != Outcome::Failed {
                        c.outcome = Outcome::Ignored(
                            tag.attr("message")
                                .map(str::to_string)
                                .or_else(|| skip_note.clone()),
                        );
                    }
                }
            }
            _ => {}
        }
        if !tag.self_closing {
            stack.push(tag.name);
        }
    }
    if !seen || !stack.is_empty() || case.is_some() {
        return Err(invalid());
    }
    Ok(cases)
}
