//! Statischer TypeScript-Tag-Scan und konservative Zuordnung von Frontend-JUnit.
//!
//! Literale test/it-Titel, Template-Titel und describe-Gruppen werden unterstützt. Statische
//! Template-Teile müssen eindeutig passen; gleiche Datei-/Titel-Kombinationen bleiben mehrdeutig. Fixtures als *.ts.txt werden
//! nicht gescannt. Kommentare, Strings und Templates werden lexikalisch getrennt.
//! `Option::is_none_or` benötigt Rust >= 1.82; Workspace-MSRV ist 1.90 (Cargo.toml),
//! rust-toolchain.toml folgt stable und muss dafür nicht angehoben werden.

use std::collections::BTreeMap;
use std::path::Path;

use super::results::{self, BinaryRef, NextestKind, TestResult};
use super::scan::{self, FileScan, TestFn};
use super::targets::{Target, TargetKind};
use crate::error::ParityError;

#[derive(Debug, Clone)]
enum Token {
    Word(String),
    Text(String),
    Template(String),
    Comment(String),
    Char(char),
}

fn lex(src: &str) -> Vec<(Token, u32)> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line = 1;
    let mut tokens = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            line += u32::from(c == '\n');
            i += 1;
            continue;
        }
        let start_line = line;
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            let start = i + 2;
            i = start;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            tokens.push((Token::Comment(chars[start..i].iter().collect()), line));
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    i += 2;
                    break;
                }
                line += u32::from(chars[i] == '\n');
                i += 1;
            }
        } else if c == '/' && tokens.iter().rev().find(|(t, _)| !matches!(t, Token::Comment(_))).is_none_or(|(token, _)| {
            matches!(token, Token::Char('(' | '[' | '{' | '=' | ':' | ',' | '!' | '?' | ';' | '>' | '|' | '&'))
                || matches!(token, Token::Word(w) if matches!(w.as_str(), "return" | "throw" | "case"))
        }) {
            // Regex-Literale können Anführungszeichen, Kommentarzeichen und Klammern enthalten.
            // Sie sind ein einzelner Ausdruck; ihre Zeichen beeinflussen keine describe-Scope.
            i += 1;
            let mut in_class = false;
            while i < chars.len() {
                match chars[i] {
                    '\\' => i += 2,
                    '[' => { in_class = true; i += 1; }
                    ']' => { in_class = false; i += 1; }
                    '/' if !in_class => { i += 1; break; }
                    '\n' => break,
                    _ => i += 1,
                }
            }
            while i < chars.len() && chars[i].is_ascii_alphabetic() { i += 1; }
            tokens.push((Token::Char('?'), start_line));
        } else if matches!(c, '\'' | '"' | '`') {
            i += 1;
            let mut text = String::new();
            let mut dynamic = false;
            while i < chars.len() && chars[i] != c {
                let ch = chars[i];
                dynamic |= c == '`' && ch == '$' && chars.get(i + 1) == Some(&'{');
                line += u32::from(ch == '\n');
                if ch == '\\' && i + 1 < chars.len() {
                    i += 1;
                    text.push(match chars[i] {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    });
                } else {
                    text.push(ch);
                }
                i += 1;
            }
            i += 1;
            tokens.push((
                if dynamic {
                    Token::Template(text)
                } else {
                    Token::Text(text)
                },
                start_line,
            ));
        } else if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '$')) {
                i += 1;
            }
            tokens.push((Token::Word(chars[start..i].iter().collect()), line));
        } else {
            tokens.push((Token::Char(c), line));
            i += 1;
        }
    }
    tokens
}

fn word(tokens: &[(Token, u32)], i: usize) -> Option<&str> {
    match tokens.get(i)?.0 {
        Token::Word(ref s) => Some(s),
        _ => None,
    }
}
fn text(tokens: &[(Token, u32)], i: usize) -> Option<&str> {
    match tokens.get(i)?.0 {
        Token::Text(ref s) => Some(s),
        _ => None,
    }
}
fn title(tokens: &[(Token, u32)], i: usize) -> Option<&str> {
    match &tokens.get(i)?.0 {
        Token::Text(s) | Token::Template(s) => Some(s),
        _ => None,
    }
}

fn template(tokens: &[(Token, u32)], i: usize) -> Option<Vec<String>> {
    let Token::Template(value) = &tokens.get(i)?.0 else {
        return None;
    };
    let mut rest = value.as_str();
    let mut parts = Vec::new();
    while let Some(start) = rest.find("${") {
        parts.push(rest[..start].into());
        let mut depth = 1;
        let mut quote = None;
        let mut escaped = false;
        let mut end = None;
        for (i, c) in rest[start + 2..].char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
                continue;
            }
            match c {
                '\'' | '"' => quote = Some(c),
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + 2 + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &rest[end?..];
    }
    parts.push(rest.into());
    Some(parts)
}

fn append_segments(parts: &mut Vec<String>, next: Vec<String>) {
    parts
        .last_mut()
        .expect("Titel enthält mindestens ein Segment")
        .push_str(&next[0]);
    parts.extend(next.into_iter().skip(1));
}

fn matches_title(test: &TestFn, title: &str) -> bool {
    fn matches(parts: &[String], title: &str) -> bool {
        let Some(mut rest) = title.strip_prefix(&parts[0]) else {
            return false;
        };
        for part in &parts[1..parts.len() - 1] {
            let Some(pos) = rest.find(part) else {
                return false;
            };
            rest = &rest[pos + part.len()..];
        }
        rest.ends_with(parts.last().expect("mindestens zwei Segmente"))
    }
    if let Some(parts) = &test.title_segments {
        matches(parts, title)
            || matches(
                &parts
                    .iter()
                    .map(|p| p.replace(" > ", " "))
                    .collect::<Vec<_>>(),
                title,
            )
    } else {
        title == test.name || title == test.name.replace(" > ", " ")
    }
}

fn ch(tokens: &[(Token, u32)], i: usize, c: char) -> bool {
    matches!(tokens.get(i), Some((Token::Char(v), _)) if *v == c)
}

struct Group {
    title: String,
    depth: usize,
    ignore: Option<String>,
    segments: Option<Vec<String>>,
}

/// Tests einer TS-/TSX-Datei; Tags müssen unmittelbar vor test()/it() stehen.
pub fn scan_source(file: &str, src: &str) -> FileScan {
    let tokens = lex(src);
    let mut out = FileScan::default();
    let mut tags: Vec<String> = Vec::new();
    // Gruppen gelten nur bis zum Ende ihres Callback-Blocks.
    let mut groups: Vec<Group> = Vec::new();
    let mut starts: BTreeMap<usize, Group> = BTreeMap::new();
    let mut depth = 0usize;
    for i in 0..tokens.len() {
        if let Some(mut group) = starts.remove(&i) {
            group.depth = depth + 1;
            groups.push(group);
        }
        match &tokens[i].0 {
            Token::Comment(c) => {
                if scan::parse_spec_comment(c).is_some() {
                    tags.push(c.clone());
                }
                continue;
            }
            Token::Char('{') => depth += 1,
            Token::Char('}') => {
                if groups.last().is_some_and(|g| g.depth == depth) {
                    groups.pop();
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        let Some(kind @ ("test" | "it" | "describe")) = word(&tokens, i) else {
            tags.clear();
            continue;
        };
        if i > 0 && ch(&tokens, i - 1, '.') {
            tags.clear();
            continue;
        }
        let mut j = i + 1;
        let mut is_describe = kind == "describe";
        let mut ignored = None;
        while ch(&tokens, j, '.') {
            let modifier = word(&tokens, j + 1).unwrap_or("");
            is_describe |= modifier == "describe";
            if matches!(modifier, "skip" | "fixme" | "todo") {
                ignored = Some(String::new());
            }
            j += 2;
        }
        if !ch(&tokens, j, '(') {
            tags.clear();
            continue;
        }
        let Some(title) = title(&tokens, j + 1) else {
            tags.clear();
            continue;
        };
        if !ch(&tokens, j + 2, ',') {
            tags.clear();
            continue;
        }
        // Registrierungsaufruf bis zur schließenden Klammer, Strings sind bereits Tokens.
        let mut end = j + 1;
        let mut parens = 1;
        while end < tokens.len() {
            if ch(&tokens, end, '(') {
                parens += 1;
            }
            if ch(&tokens, end, ')') {
                parens -= 1;
            }
            if parens == 0 {
                break;
            }
            end += 1;
        }
        let callback = (j + 3..end).find(|&k| ch(&tokens, k, '{'));
        if is_describe {
            if let Some(k) = callback {
                starts.insert(
                    k,
                    Group {
                        title: title.into(),
                        depth: 0,
                        ignore: ignored,
                        segments: template(&tokens, j + 1),
                    },
                );
            }
            tags.clear();
            continue;
        }
        // test.fixme('Titel', 'Grund', callback) und test.fixme(true, 'Grund') im Testkörper.
        if ignored.is_some() {
            if let Some(reason) = text(&tokens, j + 3) {
                ignored = Some(reason.to_string());
            }
        }
        for k in j + 3..end {
            if word(&tokens, k) != Some("test")
                || !ch(&tokens, k + 1, '.')
                || !matches!(word(&tokens, k + 2), Some("fixme" | "skip"))
                || !ch(&tokens, k + 3, '(')
            {
                continue;
            }
            if word(&tokens, k + 4) == Some("false")
                && (ch(&tokens, k + 5, ',') || ch(&tokens, k + 5, ')'))
            {
                continue;
            }
            let mut nesting = 0i32;
            for arg in k + 4..end {
                if ch(&tokens, arg, ')') && nesting == 0 {
                    break;
                }
                if ch(&tokens, arg, ',') && nesting == 0 {
                    ignored = Some(text(&tokens, arg + 1).unwrap_or("").to_string());
                    break;
                }
                if matches!(tokens[arg].0, Token::Char('(' | '[' | '{')) {
                    nesting += 1;
                }
                if matches!(tokens[arg].0, Token::Char(')' | ']' | '}')) {
                    nesting -= 1;
                }
            }
        }
        if ignored.is_none() {
            ignored = groups.iter().rev().find_map(|g| g.ignore.clone());
        }
        let mut ids = Vec::new();
        let mut criteria: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut deferred_hint = false;
        for tag in &tags {
            if let Some((tag_ids, hint)) = scan::parse_spec_comment(tag) {
                deferred_hint |= hint;
                for id in tag_ids {
                    if !ids.contains(&id) {
                        ids.push(id.clone());
                    }
                    criteria
                        .entry(id)
                        .or_default()
                        .extend(scan::criterion_markers(tag));
                }
            }
        }
        let name = groups
            .iter()
            .map(|g| g.title.as_str())
            .chain(std::iter::once(title))
            .collect::<Vec<_>>()
            .join(" > ");
        let mut segments = vec![String::new()];
        for group in &groups {
            append_segments(
                &mut segments,
                group
                    .segments
                    .clone()
                    .unwrap_or_else(|| vec![group.title.clone()]),
            );
            append_segments(&mut segments, vec![" > ".into()]);
        }
        append_segments(
            &mut segments,
            template(&tokens, j + 1).unwrap_or_else(|| vec![title.into()]),
        );
        out.tests.push(TestFn {
            file: file.into(),
            module: vec![],
            name,
            ids,
            criteria,
            title_segments: (segments.len() > 1).then_some(segments),
            deferred_hint,
            ignore: ignored,
            line: tokens[i].1,
        });
        tags.clear();
    }
    out
}

/// Scannt nur Projektquellen, niemals node_modules oder generierte Testergebnisse. Neben TypeScript der App
/// zählen die `*.test.mjs` unter `integrations/tests` (node:test, gleiche Tag-Syntax).
pub fn scan(root: &Path) -> Result<BTreeMap<String, FileScan>, ParityError> {
    fn walk(
        root: &Path,
        rel: &str,
        out: &mut BTreeMap<String, FileScan>,
    ) -> Result<(), ParityError> {
        if !root.join(rel).is_dir() {
            return Ok(());
        }
        let mut entries = std::fs::read_dir(root.join(rel))?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("{rel}/{name}");
            if entry.file_type()?.is_dir() {
                if !matches!(
                    name.as_str(),
                    "node_modules" | "dist" | "test-results" | "playwright-report" | ".git"
                ) {
                    walk(root, &path, out)?;
                }
            } else if name.ends_with(".ts")
                || name.ends_with(".tsx")
                || (rel.starts_with("integrations/") && name.ends_with(".test.mjs"))
            {
                out.insert(
                    path.clone(),
                    scan_source(&path, &std::fs::read_to_string(root.join(&path))?),
                );
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    for dir in [
        "apps/desktop/src",
        "apps/desktop/e2e",
        "apps/desktop/e2e-b",
        "apps/desktop/e2e-c",
        // B4-M2CLOSE: node:test-Verträge von Skill und Plugin (AGENT-041/042, M2-M02).
        "integrations/tests",
    ] {
        walk(root, dir, &mut out)?;
    }
    Ok(out)
}

pub(crate) fn targets(scans: &BTreeMap<String, FileScan>) -> Vec<Target> {
    scans
        .keys()
        .map(|file| Target {
            package: format!("frontend/{file}"),
            name: file.replace('-', "_"),
            raw_name: file.clone(),
            kind: TargetKind::Test,
            root: file.clone(),
            crate_dir: String::new(),
        })
        .collect()
}

fn matches_file(source: &str, reported: &str) -> bool {
    let reported = reported.replace('\\', "/");
    let reported = reported.trim_start_matches("./");
    !reported.is_empty()
        && (source == reported
            || source.ends_with(&format!("/{reported}"))
            || reported.ends_with(&format!("/{source}")))
}

/// Liefert je zugeordnetem Fall das Ergebnis und das Browserprojekt (`hostname` der Suite, bei vitest leer).
/// Das Projekt braucht die Regel für bedingte Skips in `m2.rs` (B4-M2GATE).
pub(crate) fn read_junit(
    root: &Path,
    path: &Path,
    scans: &BTreeMap<String, FileScan>,
    warnings: &mut Vec<String>,
) -> Result<Vec<(TestResult, String)>, ParityError> {
    let xml = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    // Playwright-Retries: der letzte Eintrag derselben Datei/Suite und desselben
    // konkreten Titels im selben Browserprojekt gewinnt. Template-Instanzen bleiben getrennt;
    // zwischen verschiedenen JUnit-Dateien bleibt ein Fehlschlag maßgeblich.
    let cases = results::frontend_cases(&xml)?;
    let mut last = BTreeMap::new();
    for (block, case) in cases.iter().enumerate() {
        last.insert((&case.file, &case.suite, &case.project, &case.name), block);
    }
    for (block, case) in cases.iter().enumerate() {
        if last.get(&(&case.file, &case.suite, &case.project, &case.name)) != Some(&block) {
            continue;
        }
        let mut candidates = Vec::new();
        for (file, scan) in scans {
            let relative = root.join(file).to_string_lossy().replace('\\', "/");
            if ![case.file.as_str(), case.suite.as_str()]
                .into_iter()
                .any(|f| matches_file(file, f) || matches_file(&relative, f))
            {
                continue;
            }
            for test in &scan.tests {
                let title = case.name.replace(" › ", " > ");
                if matches_title(test, &title) {
                    candidates.push((file, test));
                }
            }
        }
        if candidates.len() == 1 {
            let (file, test) = candidates[0];
            out.push((
                TestResult {
                    binary: BinaryRef::Nextest {
                        package: format!("frontend/{file}"),
                        kind: NextestKind::Test(file.clone()),
                    },
                    path: test.name.clone(),
                    outcome: case.outcome.clone(),
                    block,
                },
                case.project.clone(),
            ));
        } else {
            warnings.push(format!("Frontend-JUnit {}: {:?} / {:?} nicht eindeutig zuordenbar ({} Kandidaten); keine Gutschrift", path.display(), case.file, case.name, candidates.len()));
        }
    }
    Ok(out)
}
