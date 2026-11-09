//! Tag-Scan: findet Testfunktionen mit `// spec: ID[, ID] [M2]`-Tag und `#[ignore = "…"]` in Rust-Quellen.
//!
//! Der Scanner arbeitet auf einem einfachen Token-Strom (Kommentare, Bezeichner, Zeichenketten,
//! Satzzeichen), nicht auf Zeilen: Attribute über mehrere Zeilen, Zeichenketten mit `\`-Fortsetzung,
//! Raw-Strings, Blockkommentare und verschachtelte `mod name { … }` werden korrekt behandelt. Er kennt
//! zusätzlich die Moduldeklarationen `mod name;`, damit die Zuordnung von Testergebnissen
//! (`Binär → erreichbare Dateien → Modulpfad`) eindeutig bleibt.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::ParityError;

/// Eine Testfunktion mit ihren Tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFn {
    /// Quelldatei relativ zur Repository-Wurzel (immer mit `/`).
    pub file: String,
    /// Inline-Module in der Datei (`mod tests { … }` → `["tests"]`).
    pub module: Vec<String>,
    /// Funktionsname.
    pub name: String,
    /// Spec-IDs aus `// spec:`-Kommentaren direkt über der Funktion.
    pub ids: Vec<String>,
    /// `[M2]`/`[M3]` im Tag (zurückgestelltes Teilkriterium).
    pub deferred_hint: bool,
    /// Kriterienmarkierungen je ID (Anhang B), ohne Übertragung zwischen Tags.
    pub criteria: BTreeMap<String, Vec<String>>,
    /// Statische Titelteile eines TS-Templates; Zwischenräume sind dynamische Ausdrücke.
    pub title_segments: Option<Vec<String>>,
    /// `Some(grund)` bei `#[ignore = "grund"]`, `Some("")` bei `#[ignore]` ohne Grund.
    pub ignore: Option<String>,
    /// Zeile der Funktion (1-basiert).
    pub line: u32,
}

/// Externe Moduldeklaration `mod name;`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModDecl {
    /// Inline-Module, in denen die Deklaration steht.
    pub inline: Vec<String>,
    /// Modulname.
    pub name: String,
    /// Wert von `#[path = "…"]`, falls vorhanden.
    pub path_attr: Option<String>,
}

/// Ergebnis des Scans einer Datei.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileScan {
    /// Testfunktionen (nur mit Test-Attribut).
    pub tests: Vec<TestFn>,
    /// Externe Moduldeklarationen.
    pub mods: Vec<ModDecl>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Comment(String),
    Ident(String),
    Punct(char),
    Str(String),
    Other,
}

struct Lexed {
    tok: Tok,
    line: u32,
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

fn lex(src: &str) -> Vec<Lexed> {
    let ch: Vec<char> = src.chars().collect();
    let n = ch.len();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1u32;
    while i < n {
        let c = ch[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && ch.get(i + 1) == Some(&'/') {
            let start = i + 2;
            let mut j = start;
            while j < n && ch[j] != '\n' {
                j += 1;
            }
            out.push(Lexed {
                tok: Tok::Comment(ch[start..j].iter().collect()),
                line,
            });
            i = j;
        } else if c == '/' && ch.get(i + 1) == Some(&'*') {
            let mut depth = 1;
            i += 2;
            while i < n && depth > 0 {
                if ch[i] == '/' && ch.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if ch[i] == '*' && ch.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    if ch[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if c == '"' || ((c == 'b') && ch.get(i + 1) == Some(&'"')) {
            if c == 'b' {
                i += 1;
            }
            let start_line = line;
            i += 1;
            let mut s = String::new();
            while i < n && ch[i] != '"' {
                if ch[i] == '\\' && i + 1 < n {
                    i += 1;
                    match ch[i] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '0' => s.push('\0'),
                        '\n' => {
                            line += 1;
                            while i + 1 < n && ch[i + 1].is_whitespace() {
                                if ch[i + 1] == '\n' {
                                    line += 1;
                                }
                                i += 1;
                            }
                        }
                        'x' => {
                            let hex: String = ch[i + 1..(i + 3).min(n)].iter().collect();
                            if let Some(c) = u8::from_str_radix(&hex, 16).ok().map(char::from) {
                                s.push(c);
                            }
                            i += 2;
                        }
                        'u' if ch.get(i + 1) == Some(&'{') => {
                            let mut j = i + 2;
                            let mut hex = String::new();
                            while j < n && ch[j] != '}' {
                                hex.push(ch[j]);
                                j += 1;
                            }
                            if let Some(c) =
                                u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                            {
                                s.push(c);
                            }
                            i = j;
                        }
                        other => s.push(other),
                    }
                } else {
                    if ch[i] == '\n' {
                        line += 1;
                    }
                    s.push(ch[i]);
                }
                i += 1;
            }
            i += 1;
            out.push(Lexed {
                tok: Tok::Str(s),
                line: start_line,
            });
        } else if let Some((consumed, content, lines)) = raw_string(&ch, i) {
            out.push(Lexed {
                tok: Tok::Str(content),
                line,
            });
            line += lines;
            i += consumed;
        } else if c == '\'' {
            // Zeichenliteral oder Lebensdauer.
            if ch.get(i + 1) == Some(&'\\') {
                let mut j = i + 2;
                while j < n && ch[j] != '\'' {
                    j += 1;
                }
                i = j + 1;
            } else if ch.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
                while i < n && is_ident_char(ch[i]) {
                    i += 1;
                }
            }
            out.push(Lexed {
                tok: Tok::Other,
                line,
            });
        } else if is_ident_start(c) {
            let mut j = i;
            while j < n && is_ident_char(ch[j]) {
                j += 1;
            }
            let mut ident: String = ch[i..j].iter().collect();
            // Raw-Bezeichner r#name
            if ident == "r"
                && ch.get(j) == Some(&'#')
                && ch.get(j + 1).is_some_and(|c| is_ident_start(*c))
            {
                let mut k = j + 1;
                while k < n && is_ident_char(ch[k]) {
                    k += 1;
                }
                ident = ch[j + 1..k].iter().collect();
                j = k;
            }
            out.push(Lexed {
                tok: Tok::Ident(ident),
                line,
            });
            i = j;
        } else if c.is_ascii_digit() {
            let mut j = i;
            while j < n && is_ident_char(ch[j]) {
                j += 1;
            }
            out.push(Lexed {
                tok: Tok::Other,
                line,
            });
            i = j;
        } else {
            out.push(Lexed {
                tok: Tok::Punct(c),
                line,
            });
            i += 1;
        }
    }
    out
}

/// Erkennt `r"…"`, `r#"…"#`, `br"…"` ab Position `i`; liefert (Länge, Inhalt, enthaltene Zeilenumbrüche).
fn raw_string(ch: &[char], i: usize) -> Option<(usize, String, u32)> {
    let mut j = i;
    if ch.get(j) == Some(&'b') {
        j += 1;
    }
    if ch.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while ch.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if ch.get(j) != Some(&'"') {
        return None;
    }
    j += 1;
    let start = j;
    while j < ch.len() {
        if ch[j] == '"' && (0..hashes).all(|k| ch.get(j + 1 + k) == Some(&'#')) {
            let content: String = ch[start..j].iter().collect();
            let lines = content.matches('\n').count() as u32;
            return Some((j + 1 + hashes - i, content, lines));
        }
        j += 1;
    }
    None
}

/// Prüft das Muster einer Spec-ID (`CRAWL-014`, `D4S-072`, `UI-038`).
pub fn is_spec_id(t: &str) -> bool {
    let Some((prefix, num)) = t.split_once('-') else {
        return false;
    };
    let mut chars = prefix.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && !num.is_empty()
        && num.chars().all(|c| c.is_ascii_digit())
}

/// Wertet den Text eines Zeilenkommentars aus (`" spec: CRAWL-014, CRAWL-017 [M2]"`).
/// Liefert die IDs und ob ein Meilenstein-Hinweis (`[M2]` … `[M5]`) dabeisteht.
pub fn parse_spec_comment(text: &str) -> Option<(Vec<String>, bool)> {
    let t = text.trim_start_matches('/').trim();
    let head = t.get(..5)?;
    if !head.eq_ignore_ascii_case("spec:") {
        return None;
    }
    let rest = &t[5..];
    let mut ids = Vec::new();
    let mut deferred = false;
    for raw in rest.split(|c: char| c.is_whitespace() || c == ',') {
        let tok = raw.trim_end_matches(['.', ';', ':']);
        if tok.is_empty() {
            continue;
        }
        let flag = tok.trim_matches(['[', ']']);
        // M3-Q01: `[M4]`/`[M5]` (Teilkriterien aus `extra_spec_ids`, M4.md E7, M5.md G6) wie `[M3]`.
        if matches!(flag, "P1" | "M2" | "M3" | "M4" | "M5") && tok != flag {
            deferred |= flag != "P1";
            continue;
        }
        if is_spec_id(tok) {
            ids.push(tok.to_string());
        } else {
            break;
        }
    }
    (!ids.is_empty()).then_some((ids, deferred))
}

/// Markierungen eines einzelnen Spec-Kommentars: `[P1]`/`[M2]` für Anhang B (M2), `[M3]` … `[M5]` für die
/// Teilkriterien der Meilensteine M3–M5 (M3-Q01). Jede Auswertung fragt nur ihren eigenen Marker ab.
pub fn criterion_markers(text: &str) -> Vec<String> {
    ["P1", "M2", "M3", "M4", "M5"]
        .into_iter()
        .filter(|m| text.contains(&format!("[{m}]")))
        .map(str::to_string)
        .collect()
}

struct Attr {
    path: String,
    args: Vec<Tok>,
}

fn is_test_attr(a: &Attr) -> bool {
    a.path.rsplit("::").next() == Some("test")
}

/// Liest ein Attribut ab dem Token nach `[`; liefert das Attribut und den Index nach dem schließenden `]`.
fn read_attr(toks: &[Lexed], mut i: usize) -> (Attr, usize) {
    let mut path = String::new();
    let mut expect_ident = true;
    // Pfad: ident (:: ident)*
    loop {
        match toks.get(i).map(|t| &t.tok) {
            Some(Tok::Ident(id)) if expect_ident => {
                path.push_str(id);
                expect_ident = false;
                i += 1;
            }
            Some(Tok::Punct(':'))
                if !expect_ident
                    && matches!(toks.get(i + 1).map(|t| &t.tok), Some(Tok::Punct(':'))) =>
            {
                path.push_str("::");
                expect_ident = true;
                i += 2;
            }
            _ => break,
        }
    }
    let mut depth = 1;
    let mut args = Vec::new();
    while let Some(t) = toks.get(i) {
        match &t.tok {
            Tok::Punct('[') => depth += 1,
            Tok::Punct(']') => {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
            }
            _ => {}
        }
        args.push(t.tok.clone());
        i += 1;
    }
    (Attr { path, args }, i)
}

/// Scannt den Quelltext einer Datei; `file` ist der Pfad relativ zur Repository-Wurzel.
pub fn scan_source(file: &str, src: &str) -> FileScan {
    let toks = lex(src);
    let mut out = FileScan::default();
    let mut depth = 0i32;
    // (Name, Tiefe innerhalb des Moduls)
    let mut mods: Vec<(String, i32)> = Vec::new();
    let mut attrs: Vec<Attr> = Vec::new();
    let mut tags: Vec<(Vec<String>, bool, Vec<String>)> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i].tok {
            Tok::Comment(c) => {
                if let Some(t) = parse_spec_comment(c) {
                    tags.push((t.0, t.1, criterion_markers(c)));
                }
                i += 1;
            }
            Tok::Punct('#') => {
                let inner = matches!(toks.get(i + 1).map(|t| &t.tok), Some(Tok::Punct('!')));
                let open = i + 1 + usize::from(inner);
                if matches!(toks.get(open).map(|t| &t.tok), Some(Tok::Punct('['))) {
                    let (attr, next) = read_attr(&toks, open + 1);
                    if !inner {
                        attrs.push(attr);
                    }
                    i = next;
                } else {
                    attrs.clear();
                    tags.clear();
                    i += 1;
                }
            }
            Tok::Ident(kw) if kw == "mod" => {
                let name = match toks.get(i + 1).map(|t| &t.tok) {
                    Some(Tok::Ident(n)) => Some(n.clone()),
                    _ => None,
                };
                match (name, toks.get(i + 2).map(|t| &t.tok)) {
                    (Some(name), Some(Tok::Punct('{'))) => {
                        depth += 1;
                        mods.push((name, depth));
                        i += 3;
                    }
                    (Some(name), Some(Tok::Punct(';'))) => {
                        let path_attr =
                            attrs.iter().find(|a| a.path == "path").and_then(|a| {
                                match a.args.as_slice() {
                                    [Tok::Punct('='), Tok::Str(s), ..] => Some(s.clone()),
                                    _ => None,
                                }
                            });
                        out.mods.push(ModDecl {
                            inline: mods.iter().map(|m| m.0.clone()).collect(),
                            name,
                            path_attr,
                        });
                        i += 3;
                    }
                    _ => i += 1,
                }
                attrs.clear();
                tags.clear();
            }
            Tok::Ident(kw) if kw == "fn" => {
                if let Some(Tok::Ident(name)) = toks.get(i + 1).map(|t| &t.tok) {
                    if attrs.iter().any(is_test_attr) {
                        let ignore = attrs.iter().find(|a| a.path == "ignore").map(|a| {
                            match a.args.as_slice() {
                                [Tok::Punct('='), Tok::Str(s), ..] => s.clone(),
                                _ => String::new(),
                            }
                        });
                        let mut ids: Vec<String> = Vec::new();
                        let mut hint = false;
                        let mut criteria = BTreeMap::new();
                        for (tag_ids, h, markers) in &tags {
                            for id in tag_ids {
                                if !ids.contains(id) {
                                    ids.push(id.clone());
                                }
                                criteria
                                    .entry(id.clone())
                                    .or_insert_with(Vec::new)
                                    .extend(markers.clone());
                            }
                            hint |= *h;
                        }
                        out.tests.push(TestFn {
                            file: file.to_string(),
                            module: mods.iter().map(|m| m.0.clone()).collect(),
                            name: name.clone(),
                            ids,
                            deferred_hint: hint,
                            criteria,
                            title_segments: None,
                            ignore,
                            line: toks[i].line,
                        });
                    }
                }
                attrs.clear();
                tags.clear();
                i += 1;
            }
            Tok::Ident(kw)
                if matches!(
                    kw.as_str(),
                    "pub"
                        | "crate"
                        | "super"
                        | "self"
                        | "in"
                        | "async"
                        | "unsafe"
                        | "const"
                        | "extern"
                        | "default"
                ) =>
            {
                i += 1;
            }
            Tok::Punct('(') | Tok::Punct(')') | Tok::Str(_) => i += 1,
            Tok::Punct('{') => {
                depth += 1;
                attrs.clear();
                tags.clear();
                i += 1;
            }
            Tok::Punct('}') => {
                if mods.last().is_some_and(|m| m.1 == depth) {
                    mods.pop();
                }
                depth -= 1;
                attrs.clear();
                tags.clear();
                i += 1;
            }
            _ => {
                attrs.clear();
                tags.clear();
                i += 1;
            }
        }
    }
    out
}

/// Liest eine Datei und scannt sie.
pub fn scan_file(root: &Path, rel: &str) -> Result<FileScan, ParityError> {
    let bytes = std::fs::read(root.join(rel))?;
    Ok(scan_source(rel, &String::from_utf8_lossy(&bytes)))
}

/// Alle `.rs`-Dateien unter `dir` (rekursiv, sortiert), relativ zu `root` mit `/`.
pub fn rust_files(root: &Path, dir: &str) -> Vec<String> {
    fn walk(root: &Path, rel: &str, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(root.join(rel)) else {
            return;
        };
        let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            let child = format!("{rel}/{name}");
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !matches!(name.as_str(), "target" | "node_modules" | ".git") {
                    walk(root, &child, out);
                }
            } else if name.ends_with(".rs") {
                out.push(child);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, dir, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_comment_variants() {
        assert_eq!(
            parse_spec_comment(" spec: CRAWL-014"),
            Some((vec!["CRAWL-014".into()], false))
        );
        assert_eq!(
            parse_spec_comment(" spec: CRAWL-014, CRAWL-017 [M2]"),
            Some((vec!["CRAWL-014".into(), "CRAWL-017".into()], true))
        );
        assert_eq!(
            parse_spec_comment("/ spec: UI-038 [M3] Pixelbreite"),
            Some((vec!["UI-038".into()], true))
        );
        assert_eq!(
            parse_spec_comment(" spec: OUT-001: Titel"),
            Some((vec!["OUT-001".into()], false))
        );
        // M3-Q01: [M4]/[M5] beenden die ID-Liste nicht mehr und gelten als Meilenstein-Hinweis.
        assert_eq!(
            parse_spec_comment(" spec: AI-012 [M5], AI-013 [M4]"),
            Some((vec!["AI-012".into(), "AI-013".into()], true))
        );
        assert_eq!(
            criterion_markers(" spec: UI-004 [M4]"),
            vec!["M4".to_string()]
        );
        assert_eq!(parse_spec_comment(" spec: kein Id"), None);
        assert_eq!(parse_spec_comment(" etwas anderes CRAWL-001"), None);
        assert_eq!(
            parse_spec_comment(" spec: D4S-072 ist Text, CRAWL-001 nicht mehr"),
            Some((vec!["D4S-072".into()], false)),
            "nach der ersten Nicht-ID endet die Liste"
        );
    }

    #[test]
    fn spec_id_pattern() {
        for ok in ["CRAWL-014", "UI-038", "D4S-072", "AI-107", "ONPAGE-193"] {
            assert!(is_spec_id(ok), "{ok}");
        }
        for bad in [
            "crawl-014",
            "CRAWL014",
            "CRAWL-",
            "-014",
            "CRAWL-01a",
            "1A-001",
            "",
        ] {
            assert!(!is_spec_id(bad), "{bad}");
        }
    }

    #[test]
    fn finds_tagged_tests_with_attributes_in_any_order() {
        let src = r#"
// spec: CRAWL-001
#[test]
fn plain() {}

#[test]
// spec: CRAWL-002, CRAWL-003
#[ignore = "M2: CRAWL-003 Cookie-Banner"]
fn tagged_between_attrs() {}

// spec: CRAWL-004
#[tokio::test(start_paused = true)]
async fn async_test() {}

// spec: CRAWL-005
#[test]
pub(crate) fn visible() {}

fn helper_without_test() {}

// spec: CRAWL-006
fn tag_without_test_attribute() {}

#[test]
fn untagged() {}
"#;
        let s = scan_source("crates/x/tests/a.rs", src);
        let names: Vec<_> = s.tests.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "plain",
                "tagged_between_attrs",
                "async_test",
                "visible",
                "untagged"
            ]
        );
        assert_eq!(s.tests[0].ids, ["CRAWL-001"]);
        assert_eq!(s.tests[1].ids, ["CRAWL-002", "CRAWL-003"]);
        assert_eq!(
            s.tests[1].ignore.as_deref(),
            Some("M2: CRAWL-003 Cookie-Banner")
        );
        assert_eq!(s.tests[2].ids, ["CRAWL-004"]);
        assert_eq!(s.tests[3].ids, ["CRAWL-005"]);
        assert!(s.tests[4].ids.is_empty());
        assert_eq!(s.tests[0].ignore, None);
    }

    #[test]
    fn multiline_ignore_reason_and_bare_ignore() {
        let src = "// spec: ONPAGE-001 [M2]\n#[test]\n#[ignore = \"Klärung K-12: lange \\\n        Begründung\"]\nfn a() {}\n#[test]\n#[ignore]\nfn b() {}\n";
        let s = scan_source("f.rs", src);
        assert_eq!(
            s.tests[0].ignore.as_deref(),
            Some("Klärung K-12: lange Begründung")
        );
        assert!(s.tests[0].deferred_hint);
        assert_eq!(s.tests[1].ignore.as_deref(), Some(""));
    }

    #[test]
    fn nested_modules_are_tracked_and_braces_in_strings_ignored() {
        let src = r##"
mod outer {
    const S: &str = "} {";
    mod inner {
        // spec: TECH-001
        #[test]
        fn deep() { let c = '}'; let r = r#"}"#; }
    }
    // spec: TECH-002
    #[test]
    fn mid() {}
}
// spec: TECH-003
#[test]
fn top() {}
"##;
        let s = scan_source("f.rs", src);
        assert_eq!(s.tests.len(), 3);
        assert_eq!(s.tests[0].module, ["outer", "inner"]);
        assert_eq!(s.tests[1].module, ["outer"]);
        assert!(s.tests[2].module.is_empty());
        assert_eq!(s.tests[2].ids, ["TECH-003"]);
    }

    #[test]
    fn comments_in_block_comments_and_strings_do_not_count() {
        let src = "/* // spec: X-1\n#[test] fn nope() {} */\nconst S: &str = \"// spec: CRAWL-009\";\n// spec: CRAWL-010\n#[test]\nfn yes() {}\n";
        let s = scan_source("f.rs", src);
        assert_eq!(s.tests.len(), 1);
        assert_eq!(s.tests[0].ids, ["CRAWL-010"]);
    }

    #[test]
    fn tag_does_not_leak_to_later_function() {
        let src = "// spec: CRAWL-001\nstruct S;\n#[test]\nfn later() {}\n";
        let s = scan_source("f.rs", src);
        assert!(s.tests[0].ids.is_empty());
    }

    #[test]
    fn lifetimes_and_char_literals_do_not_break_lexing() {
        let src = "fn f<'a>(x: &'a str) -> char { '{' }\n// spec: CRAWL-001\n#[test]\nfn t() {}\n";
        let s = scan_source("f.rs", src);
        assert_eq!(s.tests.len(), 1);
        assert_eq!(s.tests[0].ids, ["CRAWL-001"]);
    }

    #[test]
    fn module_declarations_and_path_attribute() {
        let src = "mod a;\n#[path = \"other/x.rs\"]\nmod b;\nmod inline { mod c; }\n";
        let s = scan_source("src/lib.rs", src);
        assert_eq!(s.mods.len(), 3);
        assert_eq!(s.mods[0].name, "a");
        assert_eq!(s.mods[1].path_attr.as_deref(), Some("other/x.rs"));
        assert_eq!(s.mods[2].inline, ["inline"]);
    }

    #[test]
    fn proptest_macro_tests_are_found() {
        let src =
            "proptest! {\n    // spec: CRAWL-001\n    #[test]\n    fn prop(x in 0..10) {}\n}\n";
        let s = scan_source("f.rs", src);
        assert_eq!(s.tests.len(), 1);
        assert_eq!(s.tests[0].ids, ["CRAWL-001"]);
    }
}
