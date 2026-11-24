//! AI-Crawler-Katalog (AI-120).
//!
//! Versionierte Datendatei `resources/ai/ai-crawlers.json` (zur Übersetzungszeit eingebettet, Aktualisierung mit
//! App-Updates) mit den bekannten Crawlern und Steuer-Tokens von KI-Anbietern. Die Prüfregeln des JSON-Schemas
//! (Pflichtfelder, Kategorien, Datum, https-URL bzw. `null` mit `docs_note`) und die Regeln, die ein JSON-Schema
//! nicht ausdrücken kann (eindeutige Tokens ohne Rücksicht auf Groß-/Kleinschreibung, `replaced_by` verweist auf
//! ein vorhandenes Token), stehen in [`Catalog::parse`]. Benutzereinträge ergänzen den Katalog und überschreiben
//! nichts ([`Catalog::add_user_entry`]).
//!
//! Das Paket kommt ohne KI-Schlüssel aus und spricht keinen Anbieter an (08 „Abgrenzung“).

use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Inhalt des mitgelieferten Katalogs.
pub const CATALOG_JSON: &str = include_str!("../../../../resources/ai/ai-crawlers.json");

/// Unterstützte Version des Dateiformats (`schema_version`).
pub const SCHEMA_VERSION: u64 = 1;

/// Höchstalter von `verified_at` in Tagen, ab dem der Release-Check warnt (AI-120).
pub const RELEASE_CHECK_MAX_AGE_DAYS: i64 = 180;

/// Tokens, die unabhängig von ihrer Kategorie als User-Agent-Voreinstellung erscheinen (CRAWL-114 [M4]).
pub const AI_PRESET_EXTRA_TOKENS: [&str; 3] = ["GPTBot", "ClaudeBot", "Google-Extended"];

/// Pflichtfelder je Eintrag (müssen vorhanden sein; `ua_pattern`, `ua_example` und `docs_url` dürfen `null` sein).
pub const REQUIRED_FIELDS: [&str; 8] = [
    "token",
    "operator",
    "category",
    "ua_pattern",
    "ua_example",
    "respects_robots",
    "docs_url",
    "verified_at",
];

/// Optionale Felder je Eintrag.
pub const OPTIONAL_FIELDS: [&str; 4] = ["docs_note", "deprecated", "replaced_by", "renders_js"];

/// Spalten der Katalogtabelle (kanonische Export-Header, AI-120 „Ausgabe“).
pub const CATALOG_COLUMNS: [&str; 6] = [
    "Token",
    "Operator",
    "Category",
    "Respects robots.txt",
    "Verified At",
    "Docs",
];

/// Kategorie eines Katalogeintrags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    /// Sammeln für Modelltraining.
    Training,
    /// Index für KI-Suche bzw. Zitate.
    Search,
    /// Abruf auf Nutzeranfrage.
    UserFetch,
    /// Nur Steuerzeichen in robots.txt, crawlt nicht selbst.
    ControlToken,
}

impl Category {
    /// Alle Kategorien in Dateireihenfolge der Spec.
    pub const ALL: [Category; 4] = [
        Category::Training,
        Category::Search,
        Category::UserFetch,
        Category::ControlToken,
    ];

    /// Wert in der Datei und im Export.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Training => "training",
            Category::Search => "search",
            Category::UserFetch => "user-fetch",
            Category::ControlToken => "control-token",
        }
    }

    /// Liest den Dateiwert.
    pub fn parse(value: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.as_str() == value)
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Angabe des Betreibers, ob der Crawler robots.txt befolgt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RespectsRobots {
    /// Befolgt robots.txt.
    Yes,
    /// Befolgt robots.txt nur eingeschränkt.
    Partial,
    /// Befolgt robots.txt in der Regel nicht.
    No,
    /// Keine Angabe des Betreibers.
    Unknown,
}

impl RespectsRobots {
    /// Alle Werte.
    pub const ALL: [RespectsRobots; 4] = [
        RespectsRobots::Yes,
        RespectsRobots::Partial,
        RespectsRobots::No,
        RespectsRobots::Unknown,
    ];

    /// Wert in der Datei und im Export.
    pub fn as_str(self) -> &'static str {
        match self {
            RespectsRobots::Yes => "yes",
            RespectsRobots::Partial => "partial",
            RespectsRobots::No => "no",
            RespectsRobots::Unknown => "unknown",
        }
    }

    /// Liest den Dateiwert.
    pub fn parse(value: &str) -> Option<RespectsRobots> {
        RespectsRobots::ALL
            .into_iter()
            .find(|r| r.as_str() == value)
    }
}

/// Ein Katalogeintrag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrawlerEntry {
    /// robots.txt-User-Agent (Produkt-Token).
    pub token: String,
    /// Betreiber.
    pub operator: String,
    /// Kategorie.
    pub category: Category,
    /// Erkennungsmuster für Logs und Tests: Teilzeichenkette des HTTP-User-Agents, ohne Rücksicht auf
    /// Groß-/Kleinschreibung (OS-Festlegung); `None`, wenn der Betreiber keinen User-Agent dokumentiert.
    pub ua_pattern: Option<String>,
    /// Beispiel des HTTP-User-Agents aus der Betreiberdokumentation (für AI-125 und CRAWL-114 [M4]).
    pub ua_example: Option<String>,
    /// Angabe des Betreibers zu robots.txt.
    pub respects_robots: RespectsRobots,
    /// Betreiberseite (https); `None` nur mit `docs_note`.
    pub docs_url: Option<String>,
    /// Hinweis zur Quelle (Pflicht, wenn `docs_url` fehlt).
    #[serde(default)]
    pub docs_note: Option<String>,
    /// Prüfdatum `JJJJ-MM-TT`.
    pub verified_at: String,
    /// Vom Betreiber nicht mehr verwendet.
    #[serde(default)]
    pub deprecated: bool,
    /// Nachfolger eines veralteten Tokens.
    #[serde(default)]
    pub replaced_by: Option<String>,
    /// Führt der Crawler JavaScript aus (optional, AI-126 / Offene Punkte 6)?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renders_js: Option<bool>,
}

impl CrawlerEntry {
    /// Erscheint das Token als User-Agent-Voreinstellung (CRAWL-114 [M4])?
    pub fn is_ai_preset(&self) -> bool {
        (!self.deprecated && matches!(self.category, Category::Search | Category::UserFetch))
            || AI_PRESET_EXTRA_TOKENS
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&self.token))
    }

    /// Erkennt ein HTTP-User-Agent diesen Crawler (über `ua_pattern`)?
    pub fn matches_user_agent(&self, user_agent: &str) -> bool {
        self.ua_pattern.as_deref().is_some_and(|p| {
            !p.is_empty()
                && user_agent
                    .to_ascii_lowercase()
                    .contains(&p.to_ascii_lowercase())
        })
    }
}

/// Herkunft eines Eintrags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntrySource {
    /// Mitgelieferter Katalog.
    Builtin,
    /// Benutzerkatalog.
    User,
}

/// Prüffehler einer Katalogdatei oder eines Benutzereintrags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    /// Kein gültiges JSON.
    Json(String),
    /// Wurzel ist kein Objekt mit `schema_version` und `crawlers`.
    Root(String),
    /// Nicht unterstützte `schema_version`.
    SchemaVersion(String),
    /// Pflichtfeld fehlt.
    MissingField { index: usize, field: &'static str },
    /// Unbekanntes Feld.
    UnknownField { index: usize, field: String },
    /// Falscher Typ.
    WrongType {
        index: usize,
        field: String,
        expected: &'static str,
    },
    /// Leere Zeichenkette in einem Pflichtfeld.
    Empty { index: usize, field: &'static str },
    /// Token ist kein Produkt-Token (`A-Z a-z 0-9 - _`).
    InvalidToken { index: usize, value: String },
    /// Ungültige Kategorie.
    InvalidCategory { index: usize, value: String },
    /// Ungültiger Wert für `respects_robots`.
    InvalidRespectsRobots { index: usize, value: String },
    /// `verified_at` ist kein Datum `JJJJ-MM-TT`.
    InvalidDate { index: usize, value: String },
    /// `docs_url` ist keine https-URL.
    DocsUrlNotHttps { index: usize, value: String },
    /// `docs_url` ist `null` ohne nicht leeren `docs_note`.
    DocsNoteMissing { index: usize },
    /// Token doppelt (ohne Rücksicht auf Groß-/Kleinschreibung).
    DuplicateToken { index: usize, token: String },
    /// `replaced_by` nennt kein vorhandenes Token.
    UnknownReplacement { index: usize, value: String },
    /// `replaced_by` an einem nicht veralteten Eintrag.
    ReplacementWithoutDeprecated { index: usize },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatalogError::Json(e) => write!(f, "kein gültiges JSON: {e}"),
            CatalogError::Root(e) => write!(f, "Aufbau der Datei: {e}"),
            CatalogError::SchemaVersion(v) => {
                write!(f, "nicht unterstützte schema_version {v}")
            }
            CatalogError::MissingField { index, field } => {
                write!(f, "Eintrag {index}: Pflichtfeld {field} fehlt")
            }
            CatalogError::UnknownField { index, field } => {
                write!(f, "Eintrag {index}: unbekanntes Feld {field}")
            }
            CatalogError::WrongType {
                index,
                field,
                expected,
            } => write!(f, "Eintrag {index}: {field} muss {expected} sein"),
            CatalogError::Empty { index, field } => {
                write!(f, "Eintrag {index}: {field} ist leer")
            }
            CatalogError::InvalidToken { index, value } => {
                write!(f, "Eintrag {index}: {value:?} ist kein robots.txt-Token")
            }
            CatalogError::InvalidCategory { index, value } => {
                write!(f, "Eintrag {index}: ungültige Kategorie {value:?}")
            }
            CatalogError::InvalidRespectsRobots { index, value } => {
                write!(
                    f,
                    "Eintrag {index}: ungültiger Wert respects_robots {value:?}"
                )
            }
            CatalogError::InvalidDate { index, value } => {
                write!(
                    f,
                    "Eintrag {index}: verified_at {value:?} ist kein Datum JJJJ-MM-TT"
                )
            }
            CatalogError::DocsUrlNotHttps { index, value } => {
                write!(f, "Eintrag {index}: docs_url {value:?} ist keine https-URL")
            }
            CatalogError::DocsNoteMissing { index } => {
                write!(f, "Eintrag {index}: docs_url ist null ohne docs_note")
            }
            CatalogError::DuplicateToken { index, token } => {
                write!(f, "Eintrag {index}: Token {token} ist bereits vorhanden")
            }
            CatalogError::UnknownReplacement { index, value } => {
                write!(
                    f,
                    "Eintrag {index}: replaced_by {value} ist kein Token des Katalogs"
                )
            }
            CatalogError::ReplacementWithoutDeprecated { index } => {
                write!(f, "Eintrag {index}: replaced_by nur bei deprecated = true")
            }
        }
    }
}

impl std::error::Error for CatalogError {}

/// Zeile der Katalogtabelle (Spalten [`CATALOG_COLUMNS`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRow {
    pub token: String,
    pub operator: String,
    pub category: Category,
    pub respects_robots: RespectsRobots,
    pub verified_at: String,
    /// `docs_url`, sonst `docs_note`.
    pub docs: String,
    pub source: EntrySource,
}

/// Der AI-Crawler-Katalog (mitgeliefert plus Benutzereinträge).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    entries: Vec<(CrawlerEntry, EntrySource)>,
}

impl Catalog {
    /// Der mitgelieferte Katalog (einmal geprüft; ein Test stellt die Gültigkeit sicher).
    pub fn builtin() -> &'static Catalog {
        static CELL: OnceLock<Catalog> = OnceLock::new();
        CELL.get_or_init(|| {
            Catalog::parse(CATALOG_JSON)
                .expect("resources/ai/ai-crawlers.json ist gültig (Test deckt das ab)")
        })
    }

    /// Prüft und liest eine Katalogdatei (alle Fehler, nicht nur den ersten).
    pub fn parse(text: &str) -> Result<Catalog, Vec<CatalogError>> {
        let entries = parse_entries(text)?;
        Ok(Catalog {
            entries: entries
                .into_iter()
                .map(|e| (e, EntrySource::Builtin))
                .collect(),
        })
    }

    /// Ergänzt die Einträge eines Benutzerkatalogs (gleiches Format). Ein Token, das es schon gibt, wird
    /// abgelehnt; der Katalog bleibt dann unverändert.
    pub fn with_user_catalog(&self, text: &str) -> Result<Catalog, Vec<CatalogError>> {
        let user = parse_entries(text)?;
        let mut out = self.clone();
        let mut errors = Vec::new();
        for (i, entry) in user.into_iter().enumerate() {
            if let Err(e) = out.add_user_entry(entry) {
                errors.push(match e {
                    CatalogError::DuplicateToken { token, .. } => {
                        CatalogError::DuplicateToken { index: i, token }
                    }
                    other => other,
                });
            }
        }
        if errors.is_empty() {
            Ok(out)
        } else {
            Err(errors)
        }
    }

    /// Ergänzt einen Benutzereintrag. Abgelehnt wird ein bereits vorhandenes Token (AI-120) und jeder
    /// Eintrag, der die Prüfregeln verletzt.
    pub fn add_user_entry(&mut self, entry: CrawlerEntry) -> Result<(), CatalogError> {
        let index = self.entries.len();
        if self.get(&entry.token).is_some() {
            return Err(CatalogError::DuplicateToken {
                index,
                token: entry.token,
            });
        }
        let mut errors = Vec::new();
        check_entry_values(index, &entry, &mut errors);
        if let Some(rep) = &entry.replaced_by {
            if self.get(rep).is_none() {
                errors.push(CatalogError::UnknownReplacement {
                    index,
                    value: rep.clone(),
                });
            }
        }
        if let Some(e) = errors.into_iter().next() {
            return Err(e);
        }
        self.entries.push((entry, EntrySource::User));
        Ok(())
    }

    /// Alle Einträge in Katalogreihenfolge (mitgelieferte zuerst).
    pub fn entries(&self) -> impl Iterator<Item = &CrawlerEntry> {
        self.entries.iter().map(|(e, _)| e)
    }

    /// Alle Einträge mit Herkunft.
    pub fn entries_with_source(&self) -> impl Iterator<Item = (&CrawlerEntry, EntrySource)> {
        self.entries.iter().map(|(e, s)| (e, *s))
    }

    /// Nur die Benutzereinträge (zum Speichern als Benutzerkatalog).
    pub fn user_entries(&self) -> Vec<&CrawlerEntry> {
        self.entries
            .iter()
            .filter(|(_, s)| *s == EntrySource::User)
            .map(|(e, _)| e)
            .collect()
    }

    /// Benutzerkatalog als Datei im Katalogformat.
    pub fn user_catalog_json(&self) -> String {
        let doc = serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "crawlers": self.user_entries(),
        });
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    }

    /// Anzahl der Einträge.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Leer?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Eintrag zu einem Token (ohne Rücksicht auf Groß-/Kleinschreibung).
    pub fn get(&self, token: &str) -> Option<&CrawlerEntry> {
        self.entries()
            .find(|e| e.token.eq_ignore_ascii_case(token.trim()))
    }

    /// Standardauswahl der zu prüfenden Tokens: alle nicht veralteten (AI-121).
    pub fn default_selection(&self) -> Vec<&CrawlerEntry> {
        self.entries().filter(|e| !e.deprecated).collect()
    }

    /// Tokens der KI-Crawler-Voreinstellungen (CRAWL-114 [M4]): {nicht veraltete `search`- und
    /// `user-fetch`-Einträge} ∪ {`GPTBot`, `ClaudeBot`, `Google-Extended`}, in der Schreibweise des Katalogs.
    pub fn ai_preset_tokens(&self) -> BTreeSet<String> {
        self.entries()
            .filter(|e| e.is_ai_preset())
            .map(|e| e.token.clone())
            .collect()
    }

    /// Erkennt den Crawler eines HTTP-User-Agents (erster Treffer über `ua_pattern`).
    pub fn detect_user_agent(&self, user_agent: &str) -> Option<&CrawlerEntry> {
        self.entries().find(|e| e.matches_user_agent(user_agent))
    }

    /// Katalogtabelle (AI-120 „Ausgabe“).
    pub fn table_rows(&self) -> Vec<CatalogRow> {
        self.entries_with_source()
            .map(|(e, source)| CatalogRow {
                token: e.token.clone(),
                operator: e.operator.clone(),
                category: e.category,
                respects_robots: e.respects_robots,
                verified_at: e.verified_at.clone(),
                docs: e
                    .docs_url
                    .clone()
                    .or_else(|| e.docs_note.clone())
                    .unwrap_or_default(),
                source,
            })
            .collect()
    }

    /// Alle `docs_url` ohne Doppelte (Eingabe des nächtlichen Erreichbarkeitslaufs, AI-120).
    pub fn docs_urls(&self) -> Vec<&str> {
        let mut seen = BTreeSet::new();
        self.entries()
            .filter_map(|e| e.docs_url.as_deref())
            .filter(|u| seen.insert(*u))
            .collect()
    }

    /// Einträge, deren `verified_at` mehr als `max_age_days` Tage vor `today` (`JJJJ-MM-TT`) liegt.
    pub fn stale_entries(&self, today: &str, max_age_days: i64) -> Option<Vec<&CrawlerEntry>> {
        let today = day_number(today)?;
        Some(
            self.entries()
                .filter(|e| day_number(&e.verified_at).is_some_and(|d| today - d > max_age_days))
                .collect(),
        )
    }

    /// Warnungen des Release-Checks (AI-120: `verified_at` älter als 180 Tage; nur CI, nicht in der App).
    pub fn release_check_warnings(&self, today: &str) -> Vec<String> {
        match self.stale_entries(today, RELEASE_CHECK_MAX_AGE_DAYS) {
            None => vec![format!("Release-Check: ungültiges Datum {today:?}")],
            Some(stale) => stale
                .into_iter()
                .map(|e| {
                    format!(
                        "AI-Crawler-Katalog: {} zuletzt am {} geprüft (älter als {} Tage)",
                        e.token, e.verified_at, RELEASE_CHECK_MAX_AGE_DAYS
                    )
                })
                .collect(),
        }
    }
}

/// Liest und prüft die Einträge einer Katalogdatei.
fn parse_entries(text: &str) -> Result<Vec<CrawlerEntry>, Vec<CatalogError>> {
    let root: Value =
        serde_json::from_str(text).map_err(|e| vec![CatalogError::Json(e.to_string())])?;
    let Value::Object(root) = root else {
        return Err(vec![CatalogError::Root("Wurzel ist kein Objekt".into())]);
    };
    for key in root.keys() {
        if key != "schema_version" && key != "crawlers" {
            return Err(vec![CatalogError::Root(format!("unbekanntes Feld {key}"))]);
        }
    }
    match root.get("schema_version") {
        Some(Value::Number(n)) if n.as_u64() == Some(SCHEMA_VERSION) => {}
        Some(other) => return Err(vec![CatalogError::SchemaVersion(other.to_string())]),
        None => return Err(vec![CatalogError::Root("schema_version fehlt".into())]),
    }
    let Some(Value::Array(items)) = root.get("crawlers") else {
        return Err(vec![CatalogError::Root(
            "crawlers fehlt oder ist keine Liste".into(),
        )]);
    };

    let mut errors = Vec::new();
    let mut entries = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match check_shape(index, item) {
            Ok(obj) => match serde_json::from_value::<CrawlerEntry>(Value::Object(obj)) {
                Ok(entry) => {
                    check_entry_values(index, &entry, &mut errors);
                    entries.push((index, entry));
                }
                Err(e) => errors.push(CatalogError::Root(format!("Eintrag {index}: {e}"))),
            },
            Err(mut e) => errors.append(&mut e),
        }
    }

    // Eindeutigkeit ohne Rücksicht auf Groß-/Kleinschreibung und Verweise von `replaced_by`.
    let mut first: HashMap<String, usize> = HashMap::new();
    for (index, entry) in &entries {
        let key = entry.token.to_ascii_lowercase();
        match first.entry(key) {
            Entry::Occupied(_) => errors.push(CatalogError::DuplicateToken {
                index: *index,
                token: entry.token.clone(),
            }),
            Entry::Vacant(slot) => {
                slot.insert(*index);
            }
        }
    }
    for (index, entry) in &entries {
        if let Some(rep) = &entry.replaced_by {
            if !first.contains_key(&rep.to_ascii_lowercase()) {
                errors.push(CatalogError::UnknownReplacement {
                    index: *index,
                    value: rep.clone(),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(entries.into_iter().map(|(_, e)| e).collect())
    } else {
        Err(errors)
    }
}

/// Prüft Felder und Typen eines Eintrags (Teil des JSON-Schemas, der vor dem Einlesen greift).
fn check_shape(index: usize, item: &Value) -> Result<Map<String, Value>, Vec<CatalogError>> {
    let Value::Object(obj) = item else {
        return Err(vec![CatalogError::WrongType {
            index,
            field: "Eintrag".into(),
            expected: "ein Objekt",
        }]);
    };
    let mut errors = Vec::new();
    for field in REQUIRED_FIELDS {
        if !obj.contains_key(field) {
            errors.push(CatalogError::MissingField { index, field });
        }
    }
    for (key, value) in obj {
        let expected = match key.as_str() {
            "token" | "operator" | "category" | "respects_robots" | "verified_at" => {
                (!value.is_string()).then_some("eine Zeichenkette")
            }
            "ua_pattern" | "ua_example" | "docs_url" | "docs_note" | "replaced_by" => {
                (!value.is_string() && !value.is_null()).then_some("eine Zeichenkette oder null")
            }
            "deprecated" => (!value.is_boolean()).then_some("true oder false"),
            "renders_js" => {
                (!value.is_boolean() && !value.is_null()).then_some("true, false oder null")
            }
            _ => {
                errors.push(CatalogError::UnknownField {
                    index,
                    field: key.clone(),
                });
                None
            }
        };
        if let Some(expected) = expected {
            errors.push(CatalogError::WrongType {
                index,
                field: key.clone(),
                expected,
            });
        }
    }
    if let Some(Value::String(v)) = obj.get("category") {
        if Category::parse(v).is_none() {
            errors.push(CatalogError::InvalidCategory {
                index,
                value: v.clone(),
            });
        }
    }
    if let Some(Value::String(v)) = obj.get("respects_robots") {
        if RespectsRobots::parse(v).is_none() {
            errors.push(CatalogError::InvalidRespectsRobots {
                index,
                value: v.clone(),
            });
        }
    }
    if errors.is_empty() {
        Ok(obj.clone())
    } else {
        Err(errors)
    }
}

/// Wertregeln eines eingelesenen Eintrags (ohne Eindeutigkeit und Verweise).
fn check_entry_values(index: usize, e: &CrawlerEntry, errors: &mut Vec<CatalogError>) {
    if e.token.trim().is_empty() {
        errors.push(CatalogError::Empty {
            index,
            field: "token",
        });
    } else if !is_product_token(&e.token) {
        errors.push(CatalogError::InvalidToken {
            index,
            value: e.token.clone(),
        });
    }
    if e.operator.trim().is_empty() {
        errors.push(CatalogError::Empty {
            index,
            field: "operator",
        });
    }
    if day_number(&e.verified_at).is_none() {
        errors.push(CatalogError::InvalidDate {
            index,
            value: e.verified_at.clone(),
        });
    }
    let note_ok = e.docs_note.as_deref().is_some_and(|n| !n.trim().is_empty());
    match &e.docs_url {
        Some(url) if !is_https_url(url) => errors.push(CatalogError::DocsUrlNotHttps {
            index,
            value: url.clone(),
        }),
        None if !note_ok => errors.push(CatalogError::DocsNoteMissing { index }),
        _ => {}
    }
    if e.replaced_by.is_some() && !e.deprecated {
        errors.push(CatalogError::ReplacementWithoutDeprecated { index });
    }
}

/// Produkt-Token nach RFC 9309: Buchstaben, Ziffern, `-` und `_`.
fn is_product_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `https://` gefolgt von einem Host ohne Leerzeichen.
fn is_https_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    !host.is_empty() && !url.chars().any(char::is_whitespace)
}

/// Tagnummer seit 1970-01-01 für ein gültiges Datum `JJJJ-MM-TT`, sonst `None`.
pub fn day_number(date: &str) -> Option<i64> {
    let b = date.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let digits = |r: std::ops::Range<usize>| -> Option<i64> {
        let s = &date[r];
        s.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| s.parse().ok())
            .flatten()
    };
    let (y, m, d) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days_in_month = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if d < 1 || d > days_in_month {
        return None;
    }
    // Tage seit 1970-01-01 (proleptischer gregorianischer Kalender).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-120
    #[test]
    fn day_numbers_and_invalid_dates() {
        assert_eq!(day_number("1970-01-01"), Some(0));
        assert_eq!(
            day_number("2026-11-24").unwrap() - day_number("2026-05-28").unwrap(),
            180
        );
        assert_eq!(
            day_number("2024-02-29").unwrap() - day_number("2024-02-28").unwrap(),
            1
        );
        for bad in [
            "2025-02-29",
            "2026-13-01",
            "2026-1-01",
            "26-11-24",
            "2026-11-24T00:00",
            "",
        ] {
            assert_eq!(day_number(bad), None, "{bad}");
        }
    }

    // spec: AI-120
    #[test]
    fn https_and_token_rules() {
        assert!(is_https_url("https://example.test/a"));
        assert!(!is_https_url("http://example.test/a"));
        assert!(!is_https_url("https:///a"));
        assert!(is_product_token("Google-CloudVertexBot"));
        assert!(!is_product_token("Claude Bot"));
        assert!(!is_product_token("GPTBot/1.4"));
    }
}
