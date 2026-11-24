//! Local redaction of personal data before sending (AI-024).
//!
//! Fixed patterns, implemented without a regex engine: e-mail addresses → `[EMAIL]`, telephone numbers → `[PHONE]`,
//! IBANs with ISO 13616 check digits (mod 97) → `[IBAN]`. A string in IBAN shape with wrong check digits (e.g. an
//! article number) is left untouched and is also protected from the telephone pattern. Custom patterns (one regex
//! per line, `fancy-regex` via [`FancyRegexEngine`]) run after the fixed patterns and are replaced by `[REDACTED]`.
//!
//! All fixed patterns are ASCII, so byte positions of matches are always UTF-8 character boundaries.
use super::fields::FieldValues;
use serde::{Deserialize, Serialize};
use std::{fmt, ops::Range, sync::Arc};

pub const EMAIL_PLACEHOLDER: &str = "[EMAIL]";
pub const PHONE_PLACEHOLDER: &str = "[PHONE]";
pub const IBAN_PLACEHOLDER: &str = "[IBAN]";
/// Placeholder for matches of custom patterns (the spec names none; OS interpretation, see report).
pub const CUSTOM_PLACEHOLDER: &str = "[REDACTED]";

/// Number of replacements per kind; [`RedactionCounts::total`] is the log column `Redactions` (AI-014/AI-024).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionCounts {
    pub email: u32,
    pub phone: u32,
    pub iban: u32,
    pub custom: u32,
}

impl RedactionCounts {
    pub fn total(&self) -> u32 {
        self.email + self.phone + self.iban + self.custom
    }
}

impl std::ops::AddAssign for RedactionCounts {
    fn add_assign(&mut self, rhs: Self) {
        self.email += rhs.email;
        self.phone += rhs.phone;
        self.iban += rhs.iban;
        self.custom += rhs.custom;
    }
}

/// Syntax error reported by a [`PatternEngine`]; `position` is the 1-based character column in the pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternSyntaxError {
    pub position: usize,
    pub message: String,
}

/// Rejected custom pattern with 1-based line (in the pattern list) and 1-based character position (AI-024).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidPattern {
    pub line: usize,
    pub position: usize,
    pub message: String,
}

impl fmt::Display for InvalidPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, position {}: {}",
            self.line, self.position, self.message
        )
    }
}

impl std::error::Error for InvalidPattern {}

/// Compiled custom pattern: byte ranges of all non-overlapping matches.
pub trait CompiledPattern: Send + Sync {
    fn find_all(&self, text: &str) -> Vec<Range<usize>>;
}

/// Regex engine for custom patterns; [`FancyRegexEngine`] is the default, tests may inject another one.
pub trait PatternEngine: Send + Sync {
    fn compile(&self, pattern: &str) -> Result<Box<dyn CompiledPattern>, PatternSyntaxError>;
}

/// Default engine: the workspace regex crate `fancy-regex` (same syntax as the other pattern settings).
#[derive(Debug, Clone, Copy, Default)]
pub struct FancyRegexEngine;

struct FancyPattern(fancy_regex::Regex);

impl CompiledPattern for FancyPattern {
    fn find_all(&self, text: &str) -> Vec<Range<usize>> {
        // A runtime error (e.g. backtrack limit) ends the search for this pattern; matches found so far count.
        self.0
            .find_iter(text)
            .map_while(Result::ok)
            .map(|m| m.start()..m.end())
            .collect()
    }
}

impl PatternEngine for FancyRegexEngine {
    fn compile(&self, pattern: &str) -> Result<Box<dyn CompiledPattern>, PatternSyntaxError> {
        match fancy_regex::Regex::new(pattern) {
            Ok(regex) => Ok(Box::new(FancyPattern(regex))),
            Err(error) => {
                // Parse errors carry a byte offset; compile errors of the inner engine carry none (position 1).
                let position = match &error {
                    fancy_regex::Error::ParseError(offset, _) => char_position(pattern, *offset),
                    _ => 1,
                };
                Err(PatternSyntaxError {
                    position,
                    message: error.to_string(),
                })
            }
        }
    }
}

/// 1-based character position of a byte offset (clamped to the pattern, rounded down to a character boundary).
fn char_position(pattern: &str, offset: usize) -> usize {
    let mut offset = offset.min(pattern.len());
    while !pattern.is_char_boundary(offset) {
        offset -= 1;
    }
    pattern[..offset].chars().count() + 1
}

/// Custom pattern list, one pattern per line; blank lines are ignored. Parsing validates every line (AI-024:
/// an invalid pattern is rejected on check/save with line and position).
#[derive(Clone, Default)]
pub struct PatternList {
    sources: Vec<String>,
    compiled: Vec<Arc<dyn CompiledPattern>>,
}

impl fmt::Debug for PatternList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PatternList")
            .field("sources", &self.sources)
            .finish()
    }
}

impl PatternList {
    /// Check a pattern list with the default engine (`fancy-regex`).
    pub fn parse(text: &str) -> Result<Self, InvalidPattern> {
        Self::parse_with(text, &FancyRegexEngine)
    }

    pub fn parse_with(text: &str, engine: &dyn PatternEngine) -> Result<Self, InvalidPattern> {
        Self::from_lines_with(text.lines(), engine)
    }

    /// Lines given separately (e.g. `AiAppSettings::redaction_patterns`); entry n is line n.
    pub fn from_lines_with<'a>(
        lines: impl IntoIterator<Item = &'a str>,
        engine: &dyn PatternEngine,
    ) -> Result<Self, InvalidPattern> {
        let mut list = Self::default();
        for (index, line) in lines.into_iter().enumerate() {
            let pattern = line.trim_end_matches('\r');
            if pattern.trim().is_empty() {
                continue;
            }
            let compiled = engine.compile(pattern).map_err(|e| InvalidPattern {
                line: index + 1,
                position: e.position.max(1),
                message: e.message,
            })?;
            list.sources.push(pattern.to_string());
            list.compiled.push(Arc::from(compiled));
        }
        Ok(list)
    }

    pub fn sources(&self) -> &[String] {
        &self.sources
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

/// Setting „Personenbezogene Daten schwärzen" (default **on**, OS-Festlegung AI-024) plus custom patterns.
#[derive(Debug, Clone)]
pub struct RedactionSettings {
    pub enabled: bool,
    pub custom: PatternList,
}

impl Default for RedactionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            custom: PatternList::default(),
        }
    }
}

impl RedactionSettings {
    /// From the app settings (`redact_personal_data`, `redaction_patterns`; entry n = line n of the list).
    pub fn from_app_settings(
        settings: &os_model::config::AiAppSettings,
    ) -> Result<Self, InvalidPattern> {
        Self::from_app_settings_with(settings, &FancyRegexEngine)
    }

    pub fn from_app_settings_with(
        settings: &os_model::config::AiAppSettings,
        engine: &dyn PatternEngine,
    ) -> Result<Self, InvalidPattern> {
        Ok(Self {
            enabled: settings.redact_personal_data,
            custom: PatternList::from_lines_with(
                settings.redaction_patterns.iter().map(String::as_str),
                engine,
            )?,
        })
    }

    pub fn off() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Email,
    Phone,
    Iban,
}

/// Applies [`RedactionSettings`] to texts and field values.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    settings: RedactionSettings,
}

impl Redactor {
    pub fn new(settings: RedactionSettings) -> Self {
        Self { settings }
    }

    pub fn is_enabled(&self) -> bool {
        self.settings.enabled
    }

    /// Redact one text. With redaction off, the text is returned unchanged with zero counts.
    pub fn redact_text(&self, text: &str) -> (String, RedactionCounts) {
        let mut counts = RedactionCounts::default();
        if !self.settings.enabled {
            return (text.to_string(), counts);
        }
        let matches = fixed_matches(text);
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for (range, kind) in matches {
            out.push_str(&text[last..range.start]);
            out.push_str(match kind {
                Kind::Email => {
                    counts.email += 1;
                    EMAIL_PLACEHOLDER
                }
                Kind::Phone => {
                    counts.phone += 1;
                    PHONE_PLACEHOLDER
                }
                Kind::Iban => {
                    counts.iban += 1;
                    IBAN_PLACEHOLDER
                }
            });
            last = range.end;
        }
        out.push_str(&text[last..]);
        for pattern in &self.settings.custom.compiled {
            let (replaced, n) = replace_ranges(&out, pattern.find_all(&out));
            out = replaced;
            counts.custom += n;
        }
        (out, counts)
    }

    /// Redact all text fields named by AI-024 (`page_text`, `h2`, `image_context`, `meta_description`, `title`,
    /// `h1`) in place; other fields stay unchanged. Returns the counts for the request (`Redactions`).
    pub fn redact_fields(&self, values: &mut FieldValues) -> RedactionCounts {
        let mut counts = RedactionCounts::default();
        if !self.settings.enabled {
            return counts;
        }
        for (field, text) in values.texts_mut() {
            if field.is_redacted() {
                let (redacted, n) = self.redact_text(text);
                *text = redacted;
                counts += n;
            }
        }
        counts
    }
}

fn replace_ranges(text: &str, mut ranges: Vec<Range<usize>>) -> (String, u32) {
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut count = 0;
    for r in ranges {
        let valid = r.start < r.end
            && r.end <= text.len()
            && r.start >= last
            && text.is_char_boundary(r.start)
            && text.is_char_boundary(r.end);
        if !valid {
            continue;
        }
        out.push_str(&text[last..r.start]);
        out.push_str(CUSTOM_PLACEHOLDER);
        last = r.end;
        count += 1;
    }
    out.push_str(&text[last..]);
    (out, count)
}

/// Word character for boundary checks; non-ASCII bytes count as word characters (letters like `ü`).
fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || !b.is_ascii()
}

fn fixed_matches(text: &str) -> Vec<(Range<usize>, Kind)> {
    let bytes = text.as_bytes();
    let mut matches = Vec::new();
    let mut protected: Vec<Range<usize>> = Vec::new();
    for (range, valid) in iban_candidates(bytes) {
        if valid {
            matches.push((range, Kind::Iban));
        } else {
            protected.push(range);
        }
    }
    let taken = |r: &Range<usize>, m: &[(Range<usize>, Kind)], p: &[Range<usize>]| {
        m.iter()
            .map(|(x, _)| x)
            .chain(p.iter())
            .any(|x| r.start < x.end && x.start < r.end)
    };
    for range in email_matches(bytes) {
        if !taken(&range, &matches, &protected) {
            matches.push((range, Kind::Email));
        }
    }
    for range in phone_matches(bytes) {
        if !taken(&range, &matches, &protected) {
            matches.push((range, Kind::Phone));
        }
    }
    matches.sort_by_key(|(r, _)| r.start);
    matches
}

/// ISO 13616 check: move the first four characters to the end, letters → 10..35, remainder mod 97 must be 1.
pub fn iban_is_valid(compact: &str) -> bool {
    let b = compact.as_bytes();
    if !(15..=34).contains(&b.len())
        || !b[..2].iter().all(u8::is_ascii_uppercase)
        || !b[2..4].iter().all(u8::is_ascii_digit)
        || !b
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return false;
    }
    let mut rem: u32 = 0;
    for &c in b[4..].iter().chain(&b[..4]) {
        if c.is_ascii_digit() {
            rem = (rem * 10 + u32::from(c - b'0')) % 97;
        } else {
            let v = u32::from(c - b'A') + 10;
            rem = (rem * 100 + v) % 97;
        }
    }
    rem == 1
}

/// IBAN-shaped spans: two capital letters, two digits, then capital letters/digits, optionally grouped by single
/// spaces, 15–34 characters without spaces, ending at a word boundary. `true` = valid check digits.
fn iban_candidates(b: &[u8]) -> Vec<(Range<usize>, bool)> {
    let alnum = |c: u8| c.is_ascii_uppercase() || c.is_ascii_digit();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= b.len() {
        let starts = (i == 0 || !is_word(b[i - 1]))
            && b[i].is_ascii_uppercase()
            && b[i + 1].is_ascii_uppercase()
            && b[i + 2].is_ascii_digit()
            && b[i + 3].is_ascii_digit();
        if !starts {
            i += 1;
            continue;
        }
        // Candidate ends: after a group whose next byte is not a word character.
        let mut ends: Vec<(usize, String)> = Vec::new();
        let mut compact = String::new();
        let mut j = i;
        loop {
            while j < b.len() && alnum(b[j]) {
                compact.push(char::from(b[j]));
                j += 1;
            }
            if compact.len() > 34 {
                break;
            }
            if j < b.len() && is_word(b[j]) {
                break; // lowercase letter or other word character glued to the group: no boundary here
            }
            if compact.len() >= 15 {
                ends.push((j, compact.clone()));
            }
            if j + 1 < b.len() && b[j] == b' ' && alnum(b[j + 1]) {
                j += 1;
            } else {
                break;
            }
        }
        match ends.iter().rev().find(|(_, c)| iban_is_valid(c)) {
            Some((end, _)) => {
                out.push((i..*end, true));
                i = *end;
            }
            None => match ends.last() {
                Some((end, _)) => {
                    out.push((i..*end, false));
                    i = *end;
                }
                None => i += 1,
            },
        }
    }
    out
}

fn email_matches(b: &[u8]) -> Vec<Range<usize>> {
    // Non-ASCII bytes belong to the local part (internationalised addresses are redacted completely).
    let local = |c: u8| c.is_ascii_alphanumeric() || b"._%+-".contains(&c) || !c.is_ascii();
    let domain = |c: u8| c.is_ascii_alphanumeric() || c == b'.' || c == b'-';
    let mut out: Vec<Range<usize>> = Vec::new();
    for (at, _) in b.iter().enumerate().filter(|(_, c)| **c == b'@') {
        if out.last().is_some_and(|r| r.end > at) {
            continue;
        }
        let mut s = at;
        while s > 0 && local(b[s - 1]) {
            s -= 1;
        }
        while s < at && b[s] == b'.' {
            s += 1;
        }
        if s == at {
            continue;
        }
        let mut e = at + 1;
        while e < b.len() && domain(b[e]) {
            e += 1;
        }
        while e > at + 1 && (b[e - 1] == b'.' || b[e - 1] == b'-') {
            e -= 1;
        }
        let host = &b[at + 1..e];
        let labels: Vec<&[u8]> = host.split(|c| *c == b'.').collect();
        let ok = labels.len() >= 2
            && labels.iter().all(|l| !l.is_empty())
            && labels
                .last()
                .is_some_and(|tld| tld.len() >= 2 && tld.iter().all(u8::is_ascii_alphabetic));
        if ok {
            out.push(s..e);
        }
    }
    out
}

/// Telephone numbers: start with `+<digit>`, `0<digit>` or `(` before `0`/`+`, not preceded by a word character;
/// digits with single separators (space, `-`, `/`, parentheses; at most two separator characters in a row);
/// 7–15 digits (E.164 maximum); must not be followed by a word character.
fn phone_matches(b: &[u8]) -> Vec<Range<usize>> {
    let sep = |c: u8| matches!(c, b' ' | b'-' | b'/' | b'(' | b')');
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let next_digit = |k: usize| k < b.len() && b[k].is_ascii_digit();
        let starts = (i == 0 || !(is_word(b[i - 1]) || b[i - 1] == b'+'))
            && match b[i] {
                b'+' => next_digit(i + 1),
                b'0' => next_digit(i + 1),
                b'(' => i + 1 < b.len() && (b[i + 1] == b'0' || b[i + 1] == b'+'),
                _ => false,
            };
        if !starts {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut digits = usize::from(b[i].is_ascii_digit());
        let mut last_digit_end = if b[i].is_ascii_digit() { i + 1 } else { i };
        let mut run = usize::from(!b[i].is_ascii_digit());
        while j < b.len() && digits <= 15 {
            let c = b[j];
            if c.is_ascii_digit() {
                digits += 1;
                run = 0;
                last_digit_end = j + 1;
            } else if sep(c) && run < 2 {
                run += 1;
            } else {
                break;
            }
            j += 1;
        }
        // A closing parenthesis right after the last digit belongs to the number only when balanced; keep it simple
        // and end at the last digit.
        let end = last_digit_end;
        let followed_by_word = end < b.len() && is_word(b[end]);
        if (7..=15).contains(&digits) && !followed_by_word {
            out.push(i..end);
            i = end;
        } else {
            i += 1;
        }
    }
    out
}
