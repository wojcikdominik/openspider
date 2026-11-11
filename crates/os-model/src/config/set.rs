//! Einzelne Konfigurationswerte setzen (`--set pfad=wert`, OUT-186) und die Voreinstellungsgruppen
//! (CRAWL-140).
//!
//! Schlüssel sind die TOML-Pfade der Konfiguration (`speed.threads`); als Kurzschlüssel gelten `threads`,
//! `max_urls` und `max_depth`. Typ und Wertebereich eines Schlüssels stammen aus dem JSON-Schema, damit es
//! keine zweite Liste gibt, die auseinanderlaufen kann. `set` prüft Typ und Bereich des einzelnen Wertes;
//! Zusammenhänge zwischen Werten (min ≤ max, Regex) prüft [`CrawlConfig::validate`] nach dem Setzen.

use std::fmt;
use std::sync::OnceLock;

use serde_json::{Number, Value};

use crate::config::schema::{
    deref, enum_values, has_type, integer_bounds, json_schema, object_properties,
};
use crate::config::validate::range_text;
use crate::config::CrawlConfig;

/// Kurzschlüssel der CLI (OUT-186) und ihre vollen Pfade.
pub const SHORT_KEYS: [(&str, &str); 3] = [
    ("threads", "speed.threads"),
    ("max_urls", "limits.max_urls"),
    ("max_depth", "limits.max_depth"),
];

/// Größter Levenshtein-Abstand, bis zu dem ein Schlüssel vorgeschlagen wird.
const MAX_SUGGESTION_DISTANCE: usize = 3;

/// Fehler von [`CrawlConfig::set_value`] / [`CrawlConfig::apply_set`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetError {
    /// Die Zuweisung hat nicht die Form `schlüssel=wert`.
    Format {
        /// Die Eingabe.
        input: String,
    },
    /// Der Schlüssel existiert nicht.
    UnknownKey {
        /// Der angegebene Schlüssel.
        key: String,
        /// Ähnlichster Schlüssel (Levenshtein-Abstand ≤ 3), falls vorhanden.
        suggestion: Option<String>,
    },
    /// Der Wert hat den falschen Typ oder liegt außerhalb des Bereichs.
    InvalidValue {
        /// Der angegebene Schlüssel.
        key: String,
        /// Erwartung, z. B. `Ganzzahl 1–200`.
        expected: String,
    },
    /// Der Schlüssel bezeichnet einen Abschnitt, keinen einzelnen Wert.
    Section {
        /// Der angegebene Schlüssel.
        key: String,
        /// Die Schlüssel darunter.
        children: Vec<String>,
    },
    /// Der Wert ist zu strukturiert für `--set` (z. B. `auth.basic`); dafür die Datei bearbeiten.
    NotSettable {
        /// Der angegebene Schlüssel.
        key: String,
    },
}

impl fmt::Display for SetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetError::Format { input } => {
                write!(
                    f,
                    "ungültige Zuweisung {input:?}: Form schlüssel=wert erwartet"
                )
            }
            SetError::UnknownKey { key, suggestion } => {
                write!(f, "unbekannter Schlüssel {key:?}")?;
                match suggestion {
                    Some(s) => write!(f, " – meinten Sie {s:?}?"),
                    None => Ok(()),
                }
            }
            SetError::InvalidValue { key, expected } => {
                write!(f, "ungültiger Wert für {key}: {expected} erwartet")
            }
            SetError::Section { key, children } => {
                write!(
                    f,
                    "{key:?} ist ein Abschnitt, kein Wert; Schlüssel darunter: {}",
                    children.join(", ")
                )
            }
            SetError::NotSettable { key } => {
                write!(
                    f,
                    "{key:?} lässt sich nicht per --set setzen; bitte die .osconfig bearbeiten"
                )
            }
        }
    }
}

impl std::error::Error for SetError {}

enum Kind {
    Bool,
    Int {
        min: i64,
        max: Option<i64>,
    },
    Float {
        positive: bool,
        range: Option<super::validate::FloatRange>,
    },
    Str,
    Enum(Vec<String>),
    StrList,
    /// Ein Schlüsselwort (z. B. `auto`) oder eine Ganzzahl im Bereich (`anyOf` aus Text-Enum und Ganzzahl,
    /// CRAWL-151).
    WordOrInt {
        words: Vec<String>,
        min: i64,
        max: Option<i64>,
    },
}

struct Leaf {
    kind: Kind,
    nullable: bool,
}

enum Resolved {
    Leaf(Leaf),
    Section,
    NotSettable,
}

fn classify(path: &str, node: &Value) -> Resolved {
    let root = json_schema();
    let (node, nullable) = deref(root, node);
    if node.get("properties").is_some() {
        return Resolved::Section;
    }
    let kind = if let Some(kind) = word_or_int(root, node) {
        kind
    } else if let Some(values) = enum_values(node) {
        Kind::Enum(values)
    } else if has_type(node, "boolean") {
        Kind::Bool
    } else if has_type(node, "integer") {
        let (min, max) = integer_bounds(path, node);
        Kind::Int { min, max }
    } else if has_type(node, "number") {
        Kind::Float {
            positive: node.get("exclusiveMinimum").is_some(),
            range: super::validate::FLOAT_RANGES
                .iter()
                .find(|r| r.path == path)
                .copied(),
        }
    } else if has_type(node, "string") {
        Kind::Str
    } else if has_type(node, "array")
        && node
            .get("items")
            .is_some_and(|i| has_type(deref(root, i).0, "string"))
    {
        Kind::StrList
    } else {
        return Resolved::NotSettable;
    };
    Resolved::Leaf(Leaf { kind, nullable })
}

/// `anyOf` aus genau einem Text-Enum und einer Ganzzahl (`"auto"` oder 1–6).
fn word_or_int(root: &Value, node: &Value) -> Option<Kind> {
    let alts = node.get("anyOf").and_then(Value::as_array)?;
    let mut words = None;
    let mut int = None;
    for alt in alts {
        let (alt, _) = deref(root, alt);
        if has_type(alt, "string") {
            words = Some(enum_values(alt)?);
        } else if has_type(alt, "integer") {
            int = Some((
                alt.get("minimum").and_then(Value::as_i64).unwrap_or(0),
                alt.get("maximum").and_then(Value::as_i64),
            ));
        } else {
            return None;
        }
    }
    let (words, (min, max)) = (words?, int?);
    Some(Kind::WordOrInt { words, min, max })
}

/// Löst einen (vollen) Pfad im Schema auf; `None` = Schlüssel unbekannt.
fn resolve(path: &str) -> Option<Resolved> {
    let root = json_schema();
    let mut node = root;
    for seg in path.split('.') {
        node = object_properties(root, node)?.get(seg)?;
    }
    Some(classify(path, node))
}

fn collect_keys(node: &Value, prefix: &str, out: &mut Vec<String>) {
    let root = json_schema();
    let Some(props) = object_properties(root, node) else {
        return;
    };
    for (name, child) in props {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        match classify(&path, child) {
            Resolved::Section => collect_keys(child, &path, out),
            Resolved::Leaf(_) => out.push(path),
            Resolved::NotSettable => {}
        }
    }
}

/// Alle Schlüssel, die sich mit `set` setzen lassen (volle Pfade, sortiert).
pub fn settable_keys() -> &'static [String] {
    static KEYS: OnceLock<Vec<String>> = OnceLock::new();
    KEYS.get_or_init(|| {
        let mut out = Vec::new();
        collect_keys(json_schema(), "", &mut out);
        out.sort();
        out
    })
}

fn full_key(key: &str) -> &str {
    SHORT_KEYS
        .iter()
        .find(|(short, _)| *short == key)
        .map_or(key, |(_, full)| full)
}

/// Ähnlichster bekannter Schlüssel (Levenshtein-Abstand ≤ 3; Groß-/Kleinschreibung und `-` gleichgesetzt).
pub fn suggest_key(key: &str) -> Option<String> {
    let norm = key.trim().to_lowercase().replace('-', "_");
    let short = SHORT_KEYS.iter().map(|(s, _)| (*s).to_string());
    let mut best: Option<(usize, String)> = None;
    for cand in settable_keys().iter().cloned().chain(short) {
        let d = strsim::levenshtein(&norm, &cand);
        if d > MAX_SUGGESTION_DISTANCE {
            continue;
        }
        let better = match &best {
            None => true,
            Some((bd, bc)) => d < *bd || (d == *bd && (cand.len(), &cand) < (bc.len(), bc)),
        };
        if better {
            best = Some((d, cand));
        }
    }
    best.map(|(_, c)| c)
}

fn is_none_word(s: &str) -> bool {
    s.is_empty() || s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("null")
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" | "ja" => Some(true),
        "false" | "no" | "off" | "0" | "nein" => Some(false),
        _ => None,
    }
}

fn parse_float(s: &str) -> Option<f64> {
    let s = if s.matches(',').count() == 1 && !s.contains('.') {
        s.replace(',', ".")
    } else {
        s.to_string()
    };
    s.parse::<f64>().ok().filter(|f| f.is_finite())
}

fn parse_list(raw: &str) -> Option<Value> {
    if raw.is_empty() {
        return Some(Value::Array(Vec::new()));
    }
    if !raw.starts_with('[') {
        return Some(Value::Array(vec![Value::String(raw.to_string())]));
    }
    let doc: toml::Table = toml::from_str(&format!("v = {raw}")).ok()?;
    let items = doc.get("v")?.as_array()?;
    items
        .iter()
        .map(|i| i.as_str().map(|s| Value::String(s.to_string())))
        .collect::<Option<Vec<_>>>()
        .map(Value::Array)
}

/// Wandelt den Text in einen JSON-Wert; `Err` enthält die Erwartung für die Fehlermeldung.
fn parse_value(leaf: &Leaf, raw: &str) -> Result<Value, String> {
    let raw = raw.trim();
    let or_none = if leaf.nullable { " oder none" } else { "" };
    if leaf.nullable && is_none_word(raw) && !matches!(leaf.kind, Kind::Str | Kind::StrList) {
        return Ok(Value::Null);
    }
    match &leaf.kind {
        Kind::Bool => parse_bool(raw)
            .map(Value::Bool)
            .ok_or_else(|| format!("true oder false{or_none}")),
        Kind::Int { min, max } => {
            let shown_max = max.filter(|m| *m < i64::from(u32::MAX));
            let expected = || format!("Ganzzahl {}{or_none}", range_text(*min, shown_max));
            let n = raw
                .strip_prefix('+')
                .unwrap_or(raw)
                .parse::<i128>()
                .map_err(|_| expected())?;
            let too_big = max.is_some_and(|m| n > i128::from(m));
            if n < i128::from(*min) || too_big {
                return Err(expected());
            }
            i64::try_from(n)
                .map(|n| Value::Number(n.into()))
                .map_err(|_| expected())
        }
        Kind::Float { positive, range } => {
            let expected = || {
                let what = if *positive {
                    "Dezimalzahl > 0"
                } else {
                    "Dezimalzahl"
                };
                format!("{what}{or_none}")
            };
            let f = parse_float(raw).ok_or_else(expected)?;
            if (*positive && f <= 0.0) || range.is_some_and(|r| !r.accepts(f)) {
                return Err(expected());
            }
            Number::from_f64(f).map(Value::Number).ok_or_else(expected)
        }
        Kind::Str => {
            if leaf.nullable && is_none_word(raw) {
                Ok(Value::Null)
            } else {
                Ok(Value::String(raw.to_string()))
            }
        }
        Kind::Enum(values) => {
            let norm = raw.to_lowercase().replace('-', "_");
            values
                .iter()
                .find(|v| **v == norm)
                .map(|v| Value::String(v.clone()))
                .ok_or_else(|| format!("einer von {}", values.join(", ")))
        }
        Kind::StrList => parse_list(raw)
            .ok_or_else(|| "Liste der Form [\"a\", \"b\"] oder ein einzelner Wert".to_string()),
        Kind::WordOrInt { words, min, max } => {
            let expected = || {
                format!(
                    "{} oder Ganzzahl {}",
                    words.join(", "),
                    range_text(*min, *max)
                )
            };
            let norm = raw.to_lowercase();
            if let Some(word) = words.iter().find(|w| **w == norm) {
                return Ok(Value::String(word.clone()));
            }
            let n = raw
                .strip_prefix('+')
                .unwrap_or(raw)
                .parse::<i64>()
                .map_err(|_| expected())?;
            if n < *min || max.is_some_and(|m| n > m) {
                return Err(expected());
            }
            Ok(Value::Number(n.into()))
        }
    }
}

fn pointer_for(path: &str) -> String {
    format!("/{}", path.replace('.', "/"))
}

impl CrawlConfig {
    /// Setzt den Wert `value` für `key` (voller Pfad oder Kurzschlüssel). Bei einem Fehler bleibt die
    /// Konfiguration unverändert.
    pub fn set_value(&mut self, key: &str, value: &str) -> Result<(), SetError> {
        let key = key.trim();
        let path = full_key(key);
        let unknown = || SetError::UnknownKey {
            key: key.to_string(),
            suggestion: suggest_key(key),
        };
        let leaf = match resolve(path).ok_or_else(unknown)? {
            Resolved::Leaf(leaf) => leaf,
            Resolved::Section => {
                let prefix = format!("{path}.");
                return Err(SetError::Section {
                    key: key.to_string(),
                    children: settable_keys()
                        .iter()
                        .filter(|k| k.starts_with(&prefix))
                        .cloned()
                        .collect(),
                });
            }
            Resolved::NotSettable => {
                return Err(SetError::NotSettable {
                    key: key.to_string(),
                })
            }
        };
        let invalid = |expected: String| SetError::InvalidValue {
            key: key.to_string(),
            expected,
        };
        let new = parse_value(&leaf, value).map_err(invalid)?;

        let mut doc = serde_json::to_value(&*self).map_err(|e| invalid(e.to_string()))?;
        match doc.pointer_mut(&pointer_for(path)) {
            Some(slot) => *slot = new,
            None => return Err(unknown()),
        }
        let updated: CrawlConfig = serde_json::from_value(doc).map_err(|_| {
            // Wert passt zum Schema-Typ, aber nicht zum Rust-Typ (z. B. größer als u32): Erwartung nennen.
            let expected = match &leaf.kind {
                Kind::Int { min, max } => format!("Ganzzahl {}", range_text(*min, *max)),
                _ => "passender Wert".to_string(),
            };
            invalid(expected)
        })?;
        *self = updated;
        Ok(())
    }

    /// Setzt eine Zuweisung der Form `schlüssel=wert` (am ersten `=` geteilt).
    pub fn apply_set(&mut self, assignment: &str) -> Result<(), SetError> {
        let (key, value) = assignment.split_once('=').ok_or_else(|| SetError::Format {
            input: assignment.to_string(),
        })?;
        self.set_value(key, value)
    }
}

/// Voreinstellungsgruppen des Konfigurationsdialogs (Spider › Voreinstellungen, CRAWL-140).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresetGroup {
    /// Seitentitel (`title.*`).
    PageTitle,
    /// Meta-Beschreibung (`meta_description.*`).
    MetaDescription,
    /// URL (`url.*`).
    Url,
    /// H1 (`h1.*`).
    H1,
    /// H2 (`h2.*`).
    H2,
    /// Bilder (`image.*`).
    Images,
    /// Inhalt (geringe Wortzahl).
    Content,
    /// Soft-404-Text.
    Soft404,
    /// Links (`links.*`).
    Links,
    /// Größengrenzen für den Suchmaschinen-Abruf (`validation.*`).
    SizeLimits,
}

impl PresetGroup {
    /// Alle Gruppen in der Reihenfolge der Oberfläche.
    pub const ALL: [PresetGroup; 10] = [
        PresetGroup::PageTitle,
        PresetGroup::MetaDescription,
        PresetGroup::Url,
        PresetGroup::H1,
        PresetGroup::H2,
        PresetGroup::Images,
        PresetGroup::Content,
        PresetGroup::Soft404,
        PresetGroup::Links,
        PresetGroup::SizeLimits,
    ];

    /// Stabile Kennung der Gruppe.
    pub fn id(self) -> &'static str {
        match self {
            PresetGroup::PageTitle => "title",
            PresetGroup::MetaDescription => "meta_description",
            PresetGroup::Url => "url",
            PresetGroup::H1 => "h1",
            PresetGroup::H2 => "h2",
            PresetGroup::Images => "image",
            PresetGroup::Content => "content",
            PresetGroup::Soft404 => "soft404",
            PresetGroup::Links => "links",
            PresetGroup::SizeLimits => "validation",
        }
    }

    /// Gruppe zu einer Kennung.
    pub fn from_id(id: &str) -> Option<PresetGroup> {
        PresetGroup::ALL.into_iter().find(|g| g.id() == id)
    }

    /// Konfigurationsschlüssel der Gruppe (jeder Wert steht unter seinem Schlüssel im Profil).
    pub fn keys(self) -> &'static [&'static str] {
        match self {
            PresetGroup::PageTitle => &[
                "title.max_chars",
                "title.min_chars",
                "title.max_px",
                "title.min_px",
            ],
            PresetGroup::MetaDescription => &[
                "meta_description.max_chars",
                "meta_description.min_chars",
                "meta_description.max_px",
                "meta_description.min_px",
            ],
            PresetGroup::Url => &["url.max_chars"],
            PresetGroup::H1 => &["h1.max_chars"],
            PresetGroup::H2 => &["h2.max_chars"],
            PresetGroup::Images => &["image.max_kb", "image.alt_max_chars"],
            PresetGroup::Content => &["content.low_word_count"],
            PresetGroup::Soft404 => &["content.soft404_phrases"],
            PresetGroup::Links => &[
                "links.high_crawl_depth",
                "links.high_internal_outlinks",
                "links.high_external_outlinks",
                "links.non_descriptive_anchors",
            ],
            PresetGroup::SizeLimits => &[
                "validation.html_doc_max_bytes",
                "validation.resource_max_bytes",
            ],
        }
    }

    /// Setzt nur die Werte dieser Gruppe auf die Werkseinstellungen zurück; alles andere bleibt.
    pub fn reset(self, cfg: &mut CrawlConfig) {
        let defaults = serde_json::to_value(CrawlConfig::default()).unwrap_or(Value::Null);
        let Ok(mut current) = serde_json::to_value(&*cfg) else {
            return;
        };
        for key in self.keys() {
            let ptr = pointer_for(key);
            if let (Some(from), Some(to)) = (defaults.pointer(&ptr), current.pointer_mut(&ptr)) {
                *to = from.clone();
            }
        }
        if let Ok(updated) = serde_json::from_value(current) {
            *cfg = updated;
        }
    }
}
