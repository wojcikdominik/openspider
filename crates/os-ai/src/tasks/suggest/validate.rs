//! Lokale Prüfung jedes Kandidaten (AI-075, Regeln 1–6).
//!
//! Verstoßcodes (Spalte `… Violations`, mit `; ` verbunden): `empty`, `length>{max}`, `length<{min}`,
//! `pixel_width>{max}`, `pixel_width<{min}`, `duplicate_of:<Adresse>` (Stufe 1), `duplicate_candidate:<Adresse>`
//! (Stufe 2, [`super::dedupe::post_pass`]), `placeholder`, `line_break`, `quotes`, `language:<Code>`; Stil- und
//! Markenregeln (AI-077) ergänzen weitere Codes über [`super::style::StyleRules`].
//!
//! Pixelbreiten liefert ausschließlich die Funktion aus UI-038 (`os-parse`), eingebunden über [`PixelMeter`]; das
//! Frontend misst nie selbst.
use super::dedupe::SiteIndex;
use super::element::Element;
use super::lang;
use super::limits::{ElementLimits, RuleSet};
use super::style::StyleRules;

pub const EMPTY: &str = "empty";
pub const DUPLICATE_OF: &str = "duplicate_of";
pub const DUPLICATE_CANDIDATE: &str = "duplicate_candidate";
pub const PLACEHOLDER: &str = "placeholder";
pub const LINE_BREAK: &str = "line_break";
pub const QUOTES: &str = "quotes";

/// Pixelbreite nach UI-038 (Desktop-Profil: Titel 20 px, Beschreibung 14 px, Regular). Die Anwendung bindet
/// `os_parse::pixel::width_px` an; `os-ai` hängt nicht von `os-parse` ab (Architektur, E10).
pub trait PixelMeter: Send + Sync {
    fn width(&self, text: &str, size_px: f64) -> u32;
}

impl<F: Fn(&str, f64) -> u32 + Send + Sync> PixelMeter for F {
    fn width(&self, text: &str, size_px: f64) -> u32 {
        self(text, size_px)
    }
}

/// Fasst Whitespace-Folgen zu einem Leerzeichen zusammen und trimmt (Normalisierung wie bei `Title 1 Length`).
pub fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

const QUOTE_PAIRS: &[(char, char)] = &[
    ('"', '"'),
    ('\'', '\''),
    ('„', '“'),
    ('“', '”'),
    ('”', '”'),
    ('‚', '‘'),
    ('‘', '’'),
    ('«', '»'),
    ('»', '«'),
    ('‹', '›'),
];

fn quoted(text: &str) -> bool {
    let mut chars = text.chars();
    let (Some(first), Some(last)) = (chars.next(), text.chars().last()) else {
        return false;
    };
    text.chars().count() >= 2 && QUOTE_PAIRS.iter().any(|(a, b)| *a == first && *b == last)
}

fn strip_quotes(text: &str) -> String {
    let mut current = text.trim().to_string();
    while quoted(&current) {
        let inner: Vec<char> = current.chars().collect();
        current = inner[1..inner.len() - 1].iter().collect::<String>().trim().to_string();
    }
    current
}

/// Aufbereitung vor der Prüfung. Antworttexte sind bereits nach AI-021 normalisiert (Rand-Whitespace, Steuerzeichen).
/// Meta Descriptions: Zeilenumbrüche werden zu Leerzeichen und ein umschließendes Anführungszeichenpaar entfällt,
/// bevor geprüft wird (AI-072). Titles bleiben unverändert; Zeilenumbruch und Anführungszeichen sind dort Verstöße
/// nach Regel (4) (keine stille Korrektur).
pub fn prepare(element: Element, raw: &str) -> String {
    match element {
        Element::Title => raw.trim().to_string(),
        Element::MetaDescription => {
            strip_quotes(&raw.replace(['\r', '\n'], " ")).trim().to_string()
        }
    }
}

/// Unaufgelöste Platzhalter aus dem Prompt: `{{`/`}}` oder ein Abschnitt `[…]`.
pub fn has_placeholder(text: &str) -> bool {
    if text.contains("{{") || text.contains("}}") {
        return true;
    }
    match (text.find('['), text.rfind(']')) {
        (Some(open), Some(close)) => open < close,
        _ => false,
    }
}

/// Prüfwerte eines Kandidaten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// Gespeicherter Wert (nach [`prepare`]).
    pub value: String,
    /// Zeichen nach Whitespace-Normalisierung.
    pub length: u32,
    /// Pixelbreite nach UI-038.
    pub pixel_width: u32,
    pub violations: Vec<String>,
}

impl Checked {
    pub fn valid(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Alles, was die Prüfung eines Kandidaten einer URL braucht.
pub struct Checker<'a> {
    pub element: Element,
    pub limits: ElementLimits,
    pub rules: RuleSet,
    pub meter: &'a dyn PixelMeter,
    pub index: &'a SiteIndex,
    pub style: &'a dyn StyleRules,
}

impl Checker<'_> {
    /// Prüft `raw` für die URL `url_id` mit der Seitensprache `language` (Regeln 1–6, Regel 2 immer).
    pub fn check(&self, url_id: i64, language: Option<&str>, raw: &str) -> Checked {
        let value = prepare(self.element, raw);
        let measured = collapse_whitespace(&value);
        let length = u32::try_from(measured.chars().count()).unwrap_or(u32::MAX);
        let pixel_width = self.meter.width(&measured, self.element.font_px());
        let l = self.limits;
        let mut violations = Vec::new();
        if length == 0 {
            violations.push(EMPTY.to_string());
        }
        // (1) Zeichenlänge – dieselben Vergleiche wie die Filter `Over X Characters` / `Below X Characters`.
        if self.rules.length {
            if length > l.max_chars {
                violations.push(format!("length>{}", l.max_chars));
            }
            if length > 0 && length < l.min_chars {
                violations.push(format!("length<{}", l.min_chars));
            }
        }
        // (2) Pixelbreite – nicht abschaltbar; wie `Over X Pixels` / `Below X Pixels`.
        if pixel_width > l.max_px {
            violations.push(format!("pixel_width>{}", l.max_px));
        }
        if pixel_width > 0 && pixel_width < l.min_px {
            violations.push(format!("pixel_width<{}", l.min_px));
        }
        // (3) Stufe 1 der Dublettenprüfung.
        if self.rules.duplicates && length > 0 {
            if let Some(address) = self.index.duplicate_of(self.element, url_id, &measured) {
                violations.push(format!("{DUPLICATE_OF}:{address}"));
            }
        }
        // (4) Platzhalter, Zeilenumbruch, umschließendes Anführungszeichenpaar.
        if self.rules.format {
            if has_placeholder(&value) {
                violations.push(PLACEHOLDER.to_string());
            }
            if value.contains(['\n', '\r']) {
                violations.push(LINE_BREAK.to_string());
            }
            if quoted(value.trim()) {
                violations.push(QUOTES.to_string());
            }
        }
        // (5) Sprache.
        if self.rules.language {
            if let Some(v) = lang::violation(&measured, language) {
                violations.push(v);
            }
        }
        // (6) Stil- und Markenregeln (AI-077, Release 1.2).
        if self.rules.style {
            violations.extend(self.style.violations(self.element, &measured));
        }
        Checked {
            value,
            length,
            pixel_width,
            violations,
        }
    }
}

/// Konkreter Verstoß für die Reparaturrunde, z. B. „612 px > 561 px“ (AI-075).
pub fn describe(violation: &str, checked_length: u32, checked_px: u32) -> String {
    let (code, arg) = violation.split_once(['>', '<', ':']).unwrap_or((violation, ""));
    let op = if violation.contains('>') {
        ">"
    } else if violation.contains('<') {
        "<"
    } else {
        ""
    };
    match code {
        "length" => format!("{checked_length} characters {op} {arg}"),
        "pixel_width" => format!("{checked_px} px {op} {arg} px"),
        EMPTY => "empty text".into(),
        DUPLICATE_OF => format!("same as the title or description of {arg}"),
        DUPLICATE_CANDIDATE => format!("same as the suggestion for {arg}"),
        PLACEHOLDER => "contains an unresolved placeholder".into(),
        LINE_BREAK => "contains a line break".into(),
        QUOTES => "is wrapped in quotation marks".into(),
        "language" => format!("written in language {arg} instead of the page language"),
        _ => violation.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::suggest::style::NoStyleRules;
    use os_store::ai::suggestions::PageRow;

    fn per_char(text: &str, _size: f64) -> u32 {
        // 9 px je Zeichen; „W“ 18 px – genug für Grenzfälle ohne os-parse.
        text.chars().map(|c| if c == 'W' { 18 } else { 9 }).sum()
    }

    fn index() -> SiteIndex {
        SiteIndex::from_pages(&[
            PageRow {
                url_id: 1,
                address: "http://ai.test:8080/a/".into(),
                in_standard_scope: true,
                indexable: true,
                title: Some("Wanderschuhe   Damen | Shop".into()),
                title_px: None,
                meta_description: None,
                meta_description_px: None,
                h1: None,
            },
            PageRow {
                url_id: 2,
                address: "http://ai.test:8080/b/".into(),
                in_standard_scope: false,
                indexable: false,
                title: Some("Noindex Title Für Alle Fälle".into()),
                title_px: None,
                meta_description: None,
                meta_description_px: None,
                h1: None,
            },
        ])
    }

    fn checker<'a>(index: &'a SiteIndex, meter: &'a dyn PixelMeter) -> Checker<'a> {
        Checker {
            element: Element::Title,
            limits: ElementLimits {
                min_chars: 30,
                max_chars: 60,
                min_px: 200,
                max_px: 561,
            },
            rules: RuleSet::default(),
            meter,
            index,
            style: &NoStyleRules,
        }
    }

    // spec: AI-075 [M4]
    // Title: 60 Zeichen gültig, 61 ungültig; 30 gültig, 29 ungültig; 561 px gültig, 562 px ungültig.
    #[test]
    fn length_and_pixel_bounds_are_inclusive_like_the_filters() {
        let idx = index();
        let fixed = |_t: &str, _s: f64| -> u32 { 300 };
        let c = checker(&idx, &fixed);
        let a = |n: usize| "a".repeat(n);
        assert!(c.check(9, None, &a(60)).valid());
        assert_eq!(c.check(9, None, &a(61)).violations, ["length>60"]);
        assert!(c.check(9, None, &a(30)).valid());
        assert_eq!(c.check(9, None, &a(29)).violations, ["length<30"]);
        assert_eq!(c.check(9, None, "   ").violations, ["empty"]);
        for (w, ok) in [(561u32, true), (562, false), (200, true), (199, false)] {
            let m = move |_t: &str, _s: f64| -> u32 { w };
            let c3 = Checker {
                meter: &m,
                ..checker(&idx, &fixed)
            };
            let r = c3.check(9, None, &a(40));
            assert_eq!(r.valid(), ok, "{w} px: {:?}", r.violations);
            assert_eq!(r.pixel_width, w);
        }
    }

    // spec: AI-075 [M4]
    // spec: AI-071 [M4]
    // Regel 3 (Stufe 1) gegen andere indexierbare URLs, case-insensitiv und whitespace-normalisiert; Regel 4.
    #[test]
    fn duplicates_placeholders_breaks_and_quotes_are_violations() {
        let idx = index();
        let c = checker(&idx, &per_char);
        let dup = c.check(9, None, "wanderschuhe damen | SHOP");
        assert!(dup
            .violations
            .contains(&"duplicate_of:http://ai.test:8080/a/".to_string()));
        assert!(!c
            .check(1, None, "Wanderschuhe Damen | Shop")
            .violations
            .iter()
            .any(|v| v.starts_with("duplicate_of")), "eigener Title ist keine Dublette");
        assert!(!c
            .check(9, None, "Noindex Title Für Alle Fälle")
            .violations
            .iter()
            .any(|v| v.starts_with("duplicate_of")), "nicht indexierbare URL zählt nicht");
        let fmt = c.check(9, None, "Wanderschuhe für {{brand}} im Test 2026");
        assert!(fmt.violations.contains(&"placeholder".to_string()));
        assert!(c
            .check(9, None, "Wanderschuhe [Marke] im großen Test 2026")
            .violations
            .contains(&"placeholder".to_string()));
        assert!(c
            .check(9, None, "Wanderschuhe im großen\nTest 2026 für alle")
            .violations
            .contains(&"line_break".to_string()));
        assert!(c
            .check(9, None, "\"Wanderschuhe im großen Test 2026 für alle\"")
            .violations
            .contains(&"quotes".to_string()));
        let off = Checker {
            rules: RuleSet {
                format: false,
                duplicates: false,
                ..RuleSet::default()
            },
            ..checker(&idx, &per_char)
        };
        assert!(off.check(9, None, "Wanderschuhe Damen | Shop extra lang").valid());
    }

    // spec: AI-072 [M4]
    // Meta Descriptions: Zeilenumbrüche und umschließende doppelte Anführungszeichen werden vor der Prüfung entfernt.
    #[test]
    fn meta_descriptions_lose_line_breaks_and_wrapping_quotes_before_the_check() {
        assert_eq!(
            prepare(Element::MetaDescription, "\"Leichte Wanderschuhe\nfür jeden Weg.\""),
            "Leichte Wanderschuhe für jeden Weg."
        );
        assert_eq!(prepare(Element::MetaDescription, "„Zitat“"), "Zitat");
        assert_eq!(prepare(Element::Title, "\"Zitat\""), "\"Zitat\"");
    }

    // spec: AI-075 [M4]
    #[test]
    fn repair_feedback_names_concrete_values() {
        assert_eq!(describe("pixel_width>561", 50, 612), "612 px > 561 px");
        assert_eq!(describe("length>60", 65, 0), "65 characters > 60");
        assert_eq!(describe("length<30", 12, 0), "12 characters < 30");
        assert_eq!(
            describe("duplicate_of:http://ai.test:8080/a/", 0, 0),
            "same as the title or description of http://ai.test:8080/a/"
        );
    }
}
