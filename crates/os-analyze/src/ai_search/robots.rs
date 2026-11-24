//! robots.txt-Auswertung je AI-Crawler (AI-121).
//!
//! Für jeden internen Host wird die beim Crawl bereits geladene robots.txt (`robots_files`, CRAWL-098) für jedes
//! ausgewählte Katalog-Token ausgewertet – unabhängig davon, mit welchem User-Agent gecrawlt wurde. Die
//! Auswertung liest nur die Crawl-Datei und erzeugt keinen HTTP-Request.
//!
//! **Parser:** Ausgewertet wird mit dem RFC-9309-Parser des Crawl-Kerns (`os_fetch::robots`). `os-analyze` hängt
//! nicht an `os-fetch`; der Aufrufer übergibt den Parser deshalb über das Trait [`RobotsRules`] (wie
//! `aggregates::status::robots_bodies` für CRAWL-145). Gruppenauswahl und Vorrangregeln kommen damit unverändert
//! aus dem Crawl-Kern.

use std::collections::BTreeMap;

use os_model::{CrawlConfig, RobotsMode, RobotsUnreachablePolicy};
use os_store::{CrawlFile, StoreError};

use super::catalog::{Catalog, Category, CrawlerEntry};

/// Spalten des Reports „AI Crawler Access“ (kanonische Export-Header, AI-121 „Ausgabe“, dazu `Note`).
pub const REPORT_COLUMNS: [&str; 11] = [
    "Host",
    "Token",
    "Operator",
    "Category",
    "Matched Group",
    "Homepage Allowed",
    "Blocked URLs",
    "Blocked %",
    "Access",
    "Matched Rule",
    "Note",
];

/// Internal-Spalten je URL (AI-121): kommagetrennte Tokens oder leer.
pub const INTERNAL_COLUMNS: [&str; 3] = [
    "AI Search Bots Blocked",
    "AI Training Bots Blocked",
    "AI User Fetchers Blocked",
];

/// Welche Gruppe einer robots.txt für ein Token greift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMatch {
    /// Mindestens eine Gruppe nennt das Token.
    Specific,
    /// Keine Gruppe nennt das Token, es gelten die `*`-Gruppen.
    Wildcard,
    /// Weder eine Gruppe für das Token noch eine `*`-Gruppe.
    None,
}

/// Die maßgebliche Regel für Token und Pfad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleHit {
    /// `true` bei `Allow`.
    pub allowed: bool,
    /// Zeilennummer (1-basiert) in der robots.txt.
    pub line_no: u32,
    /// Regeltext der Zeile, z. B. `Disallow: /intern/`.
    pub rule: String,
}

/// Ausgewertete robots.txt eines Hosts (Adapter für den Parser des Crawl-Kerns).
pub trait RobotsRules {
    /// Gruppenauswahl für das Token (CRAWL-098: spezifische Gruppen, sonst `*`).
    fn group_match(&self, token: &str) -> GroupMatch;
    /// Maßgebliche Regel für Token und Pfad samt Query; `None` = keine Regel trifft (erlaubt).
    fn evaluate(&self, token: &str, path_and_query: &str) -> Option<RuleHit>;
}

/// Parser einer gespeicherten robots.txt (Text → ausgewertete Regeln).
pub type RobotsParser<'p> = &'p dyn Fn(&str) -> Box<dyn RobotsRules>;

/// Stand der robots.txt eines Hosts.
pub enum HostRobots {
    /// robots.txt mit Regeln (auch leer oder eigene robots.txt, CRAWL-102).
    Rules(Box<dyn RobotsRules>),
    /// 4xx außer 429: keine robots.txt, alles erlaubt (CRAWL-100).
    NotFound,
    /// 5xx, 429, Timeout, Verbindungsfehler: Behandlung nach `robots.unreachable_policy` (CRAWL-100).
    Error,
    /// Für den Host wurde keine robots.txt abgerufen (z. B. Modus „Ignorieren“, CRAWL-099).
    NotFetched,
}

/// Eingabe je Host.
pub struct HostInput {
    /// Origin des Hosts (`schema://host[:port]`), wie in `robots_files.origin`.
    pub host: String,
    /// Stand der robots.txt.
    pub robots: HostRobots,
    /// Gecrawlte interne HTML-URLs des Hosts: (`urls.id`, Pfad mit Query).
    pub urls: Vec<(i64, String)>,
}

/// Auswahl der zu prüfenden Tokens.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TokenSelection {
    /// Alle nicht veralteten Katalogeinträge (Standard).
    #[default]
    NonDeprecated,
    /// Genau diese Tokens (unbekannte werden ignoriert, Schreibweise egal).
    Tokens(Vec<String>),
}

/// Einstellungen der Auswertung.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessOptions {
    /// Zu prüfende Tokens.
    pub tokens: TokenSelection,
    /// Behandlung nicht erreichbarer robots.txt (CRAWL-100).
    pub unreachable_policy: RobotsUnreachablePolicy,
}

impl AccessOptions {
    /// Standardauswahl mit der Fehlerbehandlung der Crawl-Konfiguration.
    pub fn from_config(cfg: &CrawlConfig) -> Self {
        AccessOptions {
            tokens: TokenSelection::NonDeprecated,
            unreachable_policy: cfg.robots.unreachable_policy,
        }
    }
}

/// Ergebnis je (Host, Token).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Startseite erlaubt, keine gecrawlte URL gesperrt.
    Allowed,
    /// Teilweise gesperrt.
    PartiallyBlocked,
    /// Startseite und alle gecrawlten URLs gesperrt.
    Blocked,
    /// Keine robots.txt (= erlaubt).
    NoRobotsTxt,
    /// robots.txt nicht erreichbar (5xx u. a.).
    RobotsTxtError,
}

impl Access {
    /// Kanonischer Wert (Export).
    pub fn as_str(self) -> &'static str {
        match self {
            Access::Allowed => "Allowed",
            Access::PartiallyBlocked => "Partially Blocked",
            Access::Blocked => "Blocked",
            Access::NoRobotsTxt => "No robots.txt",
            Access::RobotsTxtError => "robots.txt Error",
        }
    }
}

/// Spalte `Matched Group`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchedGroup {
    /// Gruppe des Tokens (Schreibweise des Katalogs).
    Token(String),
    /// `*`.
    Wildcard,
    /// keine.
    None,
}

impl MatchedGroup {
    /// Kanonischer Wert: `<token>` / `*` / `none`.
    pub fn as_str(&self) -> &str {
        match self {
            MatchedGroup::Token(t) => t,
            MatchedGroup::Wildcard => "*",
            MatchedGroup::None => "none",
        }
    }
}

/// Hinweis in der Spalte `Note`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessNote {
    /// Kontroll-Token: steuert nur die Verwendung der Inhalte, nicht das Crawlen.
    ControlTokenUsageOnly,
    /// Für den Host wurde keine robots.txt abgerufen.
    RobotsNotFetched,
}

impl AccessNote {
    /// Kanonischer Text (Export; die Oberfläche übersetzt über die Variante).
    pub fn as_str(self) -> &'static str {
        match self {
            AccessNote::ControlTokenUsageOnly => {
                "Control token: governs use of the content, not crawling"
            }
            AccessNote::RobotsNotFetched => "robots.txt was not fetched for this host",
        }
    }
}

/// Zeile des Reports „AI Crawler Access“.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessRow {
    pub host: String,
    pub token: String,
    pub operator: String,
    pub category: Category,
    pub matched_group: MatchedGroup,
    pub homepage_allowed: bool,
    pub blocked_urls: u64,
    /// Anzahl der ausgewerteten (gecrawlten internen HTML-)URLs des Hosts.
    pub total_urls: u64,
    pub access: Access,
    /// Maßgebliche Regel für die Startseite.
    pub matched_rule: Option<RuleHit>,
    pub note: Option<AccessNote>,
}

impl AccessRow {
    /// Anteil gesperrter URLs in Prozent (0, wenn keine URL ausgewertet wurde).
    pub fn blocked_pct(&self) -> f64 {
        if self.total_urls == 0 {
            0.0
        } else {
            self.blocked_urls as f64 * 100.0 / self.total_urls as f64
        }
    }

    /// `Blocked %` mit zwei Nachkommastellen, z. B. `25.00`.
    pub fn blocked_pct_display(&self) -> String {
        format!("{:.2}", self.blocked_pct())
    }

    /// `Matched Rule` als Regeltext (leer, wenn keine Regel auf die Startseite trifft).
    pub fn matched_rule_display(&self) -> String {
        self.matched_rule
            .as_ref()
            .map(|r| r.rule.clone())
            .unwrap_or_default()
    }
}

/// Gesperrte Tokens je URL und Kategorie (Internal-Spalten).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UrlBlocks {
    pub url_id: i64,
    pub host: String,
    /// `AI Search Bots Blocked`.
    pub search: Vec<String>,
    /// `AI Training Bots Blocked`.
    pub training: Vec<String>,
    /// `AI User Fetchers Blocked`.
    pub user_fetch: Vec<String>,
}

impl UrlBlocks {
    /// Für welche Kategorien die URL gesperrt wäre.
    pub fn blocked_categories(&self) -> Vec<Category> {
        [
            (Category::Search, &self.search),
            (Category::Training, &self.training),
            (Category::UserFetch, &self.user_fetch),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(c, _)| c)
        .collect()
    }

    /// Spaltenwert (kommagetrennt) einer Kategorie.
    pub fn column(&self, category: Category) -> String {
        match category {
            Category::Search => self.search.join(","),
            Category::Training => self.training.join(","),
            Category::UserFetch => self.user_fetch.join(","),
            Category::ControlToken => String::new(),
        }
    }
}

/// Report „AI Crawler Access“ und Werte je URL.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AccessReport {
    /// Zeilen je (Host, Token), Hosts sortiert, Tokens in Katalogreihenfolge.
    pub rows: Vec<AccessRow>,
    /// Je ausgewerteter URL, sortiert nach `url_id`.
    pub urls: Vec<UrlBlocks>,
}

impl AccessReport {
    /// Zeile zu Host und Token.
    pub fn row(&self, host: &str, token: &str) -> Option<&AccessRow> {
        self.rows
            .iter()
            .find(|r| r.host == host && r.token.eq_ignore_ascii_case(token))
    }

    /// Werte einer URL.
    pub fn url(&self, url_id: i64) -> Option<&UrlBlocks> {
        self.urls.iter().find(|u| u.url_id == url_id)
    }
}

/// Die ausgewählten Katalogeinträge.
pub fn selected_entries<'c>(
    catalog: &'c Catalog,
    tokens: &TokenSelection,
) -> Vec<&'c CrawlerEntry> {
    match tokens {
        TokenSelection::NonDeprecated => catalog.default_selection(),
        TokenSelection::Tokens(list) => catalog
            .entries()
            .filter(|e| list.iter().any(|t| t.trim().eq_ignore_ascii_case(&e.token)))
            .collect(),
    }
}

/// Wertet die robots.txt aller Hosts für die ausgewählten Tokens aus (reine Funktion, kein I/O).
pub fn evaluate(catalog: &Catalog, hosts: &[HostInput], opts: &AccessOptions) -> AccessReport {
    let entries = selected_entries(catalog, &opts.tokens);
    let mut report = AccessReport::default();
    let mut hosts: Vec<&HostInput> = hosts.iter().collect();
    hosts.sort_by(|a, b| a.host.cmp(&b.host));

    for host in hosts {
        let mut per_url: Vec<UrlBlocks> = host
            .urls
            .iter()
            .map(|(id, _)| UrlBlocks {
                url_id: *id,
                host: host.host.clone(),
                ..Default::default()
            })
            .collect();
        for entry in &entries {
            let blocked_flags: Vec<bool> = host
                .urls
                .iter()
                .map(|(_, path)| is_blocked(&host.robots, &entry.token, path, opts))
                .collect();
            let blocked_urls = blocked_flags.iter().filter(|b| **b).count() as u64;
            let total_urls = host.urls.len() as u64;
            for (slot, blocked) in per_url.iter_mut().zip(&blocked_flags) {
                if *blocked {
                    let list = match entry.category {
                        Category::Search => &mut slot.search,
                        Category::Training => &mut slot.training,
                        Category::UserFetch => &mut slot.user_fetch,
                        Category::ControlToken => continue,
                    };
                    list.push(entry.token.clone());
                }
            }

            let (matched_group, matched_rule, homepage_allowed, access) = match &host.robots {
                HostRobots::Rules(rules) => {
                    let group = match rules.group_match(&entry.token) {
                        GroupMatch::Specific => MatchedGroup::Token(entry.token.clone()),
                        GroupMatch::Wildcard => MatchedGroup::Wildcard,
                        GroupMatch::None => MatchedGroup::None,
                    };
                    let hit = rules.evaluate(&entry.token, "/");
                    let home = hit.as_ref().is_none_or(|h| h.allowed);
                    let access = if home && blocked_urls == 0 {
                        Access::Allowed
                    } else if !home && blocked_urls == total_urls {
                        Access::Blocked
                    } else {
                        Access::PartiallyBlocked
                    };
                    (group, hit, home, access)
                }
                HostRobots::NotFound | HostRobots::NotFetched => {
                    (MatchedGroup::None, None, true, Access::NoRobotsTxt)
                }
                HostRobots::Error => (
                    MatchedGroup::None,
                    None,
                    opts.unreachable_policy == RobotsUnreachablePolicy::AllowAll,
                    Access::RobotsTxtError,
                ),
            };
            let note = if entry.category == Category::ControlToken {
                Some(AccessNote::ControlTokenUsageOnly)
            } else if matches!(host.robots, HostRobots::NotFetched) {
                Some(AccessNote::RobotsNotFetched)
            } else {
                None
            };
            report.rows.push(AccessRow {
                host: host.host.clone(),
                token: entry.token.clone(),
                operator: entry.operator.clone(),
                category: entry.category,
                matched_group,
                homepage_allowed,
                blocked_urls,
                total_urls,
                access,
                matched_rule,
                note,
            });
        }
        report.urls.extend(per_url);
    }
    report.urls.sort_by_key(|u| u.url_id);
    report
}

fn is_blocked(robots: &HostRobots, token: &str, path: &str, opts: &AccessOptions) -> bool {
    match robots {
        HostRobots::Rules(rules) => rules.evaluate(token, path).is_some_and(|h| !h.allowed),
        HostRobots::NotFound | HostRobots::NotFetched => false,
        HostRobots::Error => opts.unreachable_policy == RobotsUnreachablePolicy::BlockHost,
    }
}

/// Gespeicherte robots.txt eines Origins (`robots_files`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRobots {
    pub origin: String,
    /// `ok`, `custom`, `not_found` oder `unreachable`.
    pub fetch_status: String,
    pub status_code: Option<i64>,
    pub body: Option<String>,
}

/// Liest die gespeicherten robots.txt-Abrufe (kleine Tabelle, eine Zeile je Origin).
pub fn stored_robots(file: &CrawlFile) -> Result<Vec<StoredRobots>, StoreError> {
    let mut stmt = file.conn().prepare(
        "SELECT origin, fetch_status, status_code, body FROM robots_files ORDER BY origin",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(StoredRobots {
            origin: r.get(0)?,
            fetch_status: r.get(1)?,
            status_code: r.get(2)?,
            body: r.get(3)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)
}

/// Interne Hosts (Origins) mit ihren gecrawlten internen HTML-URLs (`content_class = 'HTML'`); Hosts ohne
/// HTML-URL erscheinen mit leerer Liste.
pub fn internal_html_paths(
    file: &CrawlFile,
) -> Result<BTreeMap<String, Vec<(i64, String)>>, StoreError> {
    let mut out: BTreeMap<String, Vec<(i64, String)>> = BTreeMap::new();
    let mut stmt = file
        .conn()
        .prepare("SELECT DISTINCT origin FROM urls WHERE is_internal = 1")?;
    for origin in stmt.query_map([], |r| r.get::<_, String>(0))? {
        out.entry(origin?).or_default();
    }
    let mut stmt = file.conn().prepare(
        "SELECT id, origin, url FROM urls WHERE is_internal = 1 AND content_class = 'HTML' ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (id, origin, url) = row?;
        let path = path_and_query(&origin, &url);
        out.entry(origin).or_default().push((id, path));
    }
    Ok(out)
}

/// Pfad samt Query einer URL ihres Origins (ohne Fragment; leerer Pfad = `/`).
pub fn path_and_query(origin: &str, url: &str) -> String {
    let rest = url.strip_prefix(origin).unwrap_or_else(|| {
        // Fallback: alles ab dem ersten `/` nach `schema://`.
        let after_scheme = url.find("://").map_or(0, |i| i + 3);
        url[after_scheme..]
            .find('/')
            .map_or("", |i| &url[after_scheme + i..])
    });
    let rest = rest.split('#').next().unwrap_or("");
    if rest.is_empty() || rest.starts_with('?') {
        format!("/{rest}")
    } else {
        rest.to_string()
    }
}

/// Stand der robots.txt aus einem gespeicherten Abruf.
pub fn host_robots(stored: Option<&StoredRobots>, parse: RobotsParser<'_>) -> HostRobots {
    match stored {
        None => HostRobots::NotFetched,
        Some(s) => match s.fetch_status.as_str() {
            "ok" | "custom" => HostRobots::Rules(parse(s.body.as_deref().unwrap_or(""))),
            "not_found" => HostRobots::NotFound,
            _ => HostRobots::Error,
        },
    }
}

/// Report „AI Crawler Access“ einer Crawl-Datei. Liest nur `robots_files` und `urls`, sendet keinen Request.
///
/// Im robots.txt-Modus „Ignorieren“ (CRAWL-099) gibt es keine gespeicherte robots.txt; dann gilt für alle Hosts
/// [`HostRobots::NotFetched`].
pub fn access_report(
    file: &CrawlFile,
    catalog: &Catalog,
    parse: RobotsParser<'_>,
    opts: &AccessOptions,
) -> Result<AccessReport, StoreError> {
    let stored = stored_robots(file)?;
    let hosts = internal_html_paths(file)?
        .into_iter()
        .map(|(host, urls)| HostInput {
            robots: host_robots(stored.iter().find(|s| s.origin == host), parse),
            host,
            urls,
        })
        .collect::<Vec<_>>();
    Ok(evaluate(catalog, &hosts, opts))
}

/// Ob die Crawl-Konfiguration robots.txt überhaupt abruft (sonst sind alle Hosts „nicht abgerufen“).
pub fn robots_fetched(cfg: &CrawlConfig) -> bool {
    cfg.robots.mode != RobotsMode::Ignore
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-121
    #[test]
    fn path_and_query_of_stored_urls() {
        let o = "http://ai.test:8080";
        assert_eq!(path_and_query(o, "http://ai.test:8080"), "/");
        assert_eq!(path_and_query(o, "http://ai.test:8080/"), "/");
        assert_eq!(
            path_and_query(o, "http://ai.test:8080/a/b?x=1#f"),
            "/a/b?x=1"
        );
        assert_eq!(path_and_query(o, "http://ai.test:8080?x=1"), "/?x=1");
        assert_eq!(
            path_and_query("http://other.test", "https://ai.test/c"),
            "/c"
        );
    }
}
