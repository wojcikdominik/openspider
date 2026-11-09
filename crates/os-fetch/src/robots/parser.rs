//! robots.txt-Parser nach RFC 9309 und dem dokumentierten Google-Verhalten.
//!
//! * Zeilenenden CRLF, LF und CR, BOM am Anfang, Kommentare (`#`), Leerzeichen um den Doppelpunkt.
//! * Nur die ersten 500 KiB werden ausgewertet; eine dabei angeschnittene letzte Zeile entfällt
//!   (ein abgeschnittenes Muster würde sonst mehr sperren als gewollt).
//! * Gruppen wie bei Google (OS-Festlegung K-29): aufeinanderfolgende `User-agent`-Zeilen bilden eine
//!   Gruppe, auch über `Sitemap:`, `Crawl-delay` und unbekannte Zeilen hinweg. Eine neue Gruppe beginnt
//!   erst mit einer `User-agent`-Zeile nach einer Regelzeile (`Allow`/`Disallow`). Regeln vor der ersten
//!   `User-agent`-Zeile gelten nirgends.
//! * Auswahl: alle Gruppen, die das Token nennen (case-insensitiv), werden zusammengeführt; nur wenn es
//!   keine gibt, gelten die `*`-Gruppen (ebenfalls zusammengeführt).
//! * Vorrang: längstes Muster (Länge des kanonischen Musters inklusive `*` und `$`), bei Gleichstand
//!   gewinnt `Allow`, bei Gleichstand gleicher Art die frühere Zeile.
//! * `Crawl-delay` wird nur gelesen ([`RobotsTxt::crawl_delay`]), nie angewandt (OS-Festlegung).

use super::pattern::{canonical, Pattern, MAX_BYTES};

/// Eine treffende Regel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RobotsMatch {
    /// `true` bei `Allow`, `false` bei `Disallow`.
    pub allowed: bool,
    /// Zeilennummer (1-basiert) in der robots.txt.
    pub line_no: u32,
    /// Regeltext der Zeile.
    pub line: String,
}

impl RobotsMatch {
    /// Anzeigeform für die Spalte „Gesperrt durch Zeile“: `Zeile n: Regeltext`.
    pub fn matched_line(&self) -> String {
        format!("Zeile {}: {}", self.line_no, self.line)
    }

    /// Umkehrung von [`RobotsMatch::matched_line`]: Zeilennummer und Regeltext aus der gespeicherten Anzeigeform, damit
    /// Oberflächen die Zeile selbst formulieren (i18n) statt den gespeicherten Präfix zu zeigen (UI-019). `None`, wenn der
    /// Text nicht diese Form hat.
    pub fn split_matched_line(text: &str) -> Option<(u32, &str)> {
        let (number, line) = text.strip_prefix("Zeile ")?.split_once(": ")?;
        Some((number.parse().ok()?, line))
    }
}

#[derive(Debug, Clone)]
struct Rule {
    allowed: bool,
    pattern: Pattern,
    line_no: u32,
    line: String,
}

#[derive(Debug, Clone, Default)]
struct Group {
    /// Kleingeschriebene Produkt-Tokens oder `*`.
    agents: Vec<String>,
    rules: Vec<Rule>,
    crawl_delay: Option<f64>,
}

/// Ausgewertete robots.txt.
#[derive(Debug, Clone, Default)]
pub struct RobotsTxt {
    groups: Vec<Group>,
    sitemaps: Vec<String>,
}

/// Produkt-Token eines `User-agent`-Werts bzw. des eigenen Tokens: führende Zeichen aus
/// `A-Z a-z 0-9 - _`, kleingeschrieben (`OpenSpider/1.0` → `openspider`).
fn product_token(value: &str) -> String {
    value
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Schneidet auf höchstens 500 KiB (an einer Zeichengrenze) und verwirft eine angeschnittene Zeile.
fn limit_input(body: &str) -> &str {
    if body.len() <= MAX_BYTES {
        return body;
    }
    let mut cut = MAX_BYTES;
    while !body.is_char_boundary(cut) {
        cut -= 1;
    }
    let head = &body[..cut];
    let on_line_end = head.ends_with(['\n', '\r']) || body[cut..].starts_with(['\n', '\r']);
    if on_line_end {
        head
    } else {
        match head.rfind(['\n', '\r']) {
            Some(i) => &head[..=i],
            None => "",
        }
    }
}

/// Wertet den Text einer robots.txt aus.
pub fn parse(body: &str) -> RobotsTxt {
    let text = limit_input(body);
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut txt = RobotsTxt::default();
    // Gruppe, der folgende Regeln zugeordnet werden; `agent_run`: es folgen weitere User-agent-Zeilen
    // in dieselbe Gruppe (bleibt bis zur nächsten Regelzeile bestehen, K-29).
    let mut current: Option<usize> = None;
    let mut agent_run = false;

    let mut rest = text;
    let mut line_no: u32 = 0;
    while !rest.is_empty() {
        line_no = line_no.saturating_add(1);
        let end = rest.find(['\r', '\n']).unwrap_or(rest.len());
        let raw = &rest[..end];
        let advance = if rest[end..].starts_with("\r\n") {
            end + 2
        } else if end < rest.len() {
            end + 1
        } else {
            end
        };
        rest = &rest[advance..];

        let content = raw.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = content.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        match key.as_str() {
            "user-agent" | "useragent" | "user agent" => {
                let token = if value == "*" {
                    "*".to_string()
                } else {
                    product_token(value)
                };
                if token.is_empty() {
                    continue;
                }
                let idx = match (agent_run, current) {
                    (true, Some(idx)) => idx,
                    _ => {
                        txt.groups.push(Group::default());
                        let idx = txt.groups.len() - 1;
                        current = Some(idx);
                        idx
                    }
                };
                agent_run = true;
                let agents = &mut txt.groups[idx].agents;
                if !agents.contains(&token) {
                    agents.push(token);
                }
            }
            "allow" | "disallow" => {
                agent_run = false;
                if let (Some(idx), Some(pattern)) = (current, Pattern::new(value)) {
                    txt.groups[idx].rules.push(Rule {
                        allowed: key == "allow",
                        pattern,
                        line_no,
                        line: content.to_string(),
                    });
                }
            }
            "crawl-delay" => {
                if let (Some(idx), Ok(secs)) = (current, value.parse::<f64>()) {
                    if secs.is_finite() && secs >= 0.0 {
                        txt.groups[idx].crawl_delay.get_or_insert(secs);
                    }
                }
            }
            "sitemap" if !value.is_empty() => txt.sitemaps.push(value.to_string()),
            _ => {}
        }
    }
    txt
}

impl RobotsTxt {
    /// Gruppen, die für das Token gelten: spezifische, sonst `*`.
    fn groups_for<'a>(&'a self, token: &str) -> Vec<&'a Group> {
        let token = product_token(token);
        let specific: Vec<&Group> = if token.is_empty() {
            Vec::new()
        } else {
            self.groups
                .iter()
                .filter(|g| g.agents.contains(&token))
                .collect()
        };
        if !specific.is_empty() {
            return specific;
        }
        self.groups
            .iter()
            .filter(|g| g.agents.iter().any(|a| a == "*"))
            .collect()
    }

    /// Bestimmt die maßgebliche Regel für Token und Pfad samt Query; `None` = keine Regel trifft (erlaubt).
    ///
    /// `path_and_query` ist der Pfad mit Query, z. B. `/a/b?x=1`, ohne Fragment.
    pub fn evaluate(&self, token: &str, path_and_query: &str) -> Option<RobotsMatch> {
        let path = canonical(path_and_query);
        let mut best: Option<&Rule> = None;
        for rule in self
            .groups_for(token)
            .into_iter()
            .flat_map(|g| g.rules.iter())
        {
            if !rule.pattern.matches(&path) {
                continue;
            }
            let better = match best {
                None => true,
                Some(b) => {
                    rule.pattern.len() > b.pattern.len()
                        || (rule.pattern.len() == b.pattern.len() && rule.allowed && !b.allowed)
                }
            };
            if better {
                best = Some(rule);
            }
        }
        best.map(|r| RobotsMatch {
            allowed: r.allowed,
            line_no: r.line_no,
            line: r.line.clone(),
        })
    }

    /// `Crawl-delay` in Sekunden für das Token (nur Hinweis, wird nicht angewandt).
    pub fn crawl_delay(&self, token: &str) -> Option<f64> {
        self.groups_for(token)
            .into_iter()
            .find_map(|g| g.crawl_delay)
    }

    /// `Sitemap:`-Einträge in Dateireihenfolge.
    pub fn sitemaps(&self) -> &[String] {
        &self.sitemaps
    }

    /// Anzahl der gültigen `Allow`-/`Disallow`-Regeln über alle Gruppen.
    pub fn rule_count(&self) -> usize {
        self.groups.iter().map(|g| g.rules.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(txt: &RobotsTxt, token: &str, path: &str) -> bool {
        txt.evaluate(token, path).is_some_and(|m| !m.allowed)
    }

    // spec: CRAWL-098
    #[test]
    fn collects_sitemap_lines() {
        let txt = parse("User-agent: *\nDisallow: /x\nSitemap: https://site.test/s.xml\n");
        assert_eq!(txt.sitemaps(), ["https://site.test/s.xml"]);
        assert!(blocked(&txt, "openspider", "/x"));
    }

    // spec: CRAWL-098
    #[test]
    fn most_specific_group_wins_case_insensitive() {
        let txt =
            parse("User-agent: *\nDisallow: /\n\nUser-agent: OPENSPIDER\nDisallow: /nur-spider/\n");
        assert!(blocked(&txt, "OpenSpider", "/nur-spider/a"));
        assert!(!blocked(&txt, "openspider", "/andere"));
        assert!(blocked(&txt, "otherbot", "/andere"));
        assert!(blocked(&txt, "", "/andere"));
    }

    // spec: CRAWL-098
    #[test]
    fn star_group_only_without_specific() {
        let txt = parse("User-agent: *\nDisallow: /star/\nUser-agent: googlebot\nDisallow: /g/\n");
        assert!(!blocked(&txt, "openspider", "/g/x"));
        assert!(blocked(&txt, "openspider", "/star/x"));
        // Eine spezifische Gruppe ohne Regeln verdrängt `*` trotzdem.
        let txt = parse("User-agent: *\nDisallow: /\nUser-agent: openspider\nDisallow:\n");
        assert!(txt.evaluate("openspider", "/x").is_none());
    }

    // spec: CRAWL-098
    #[test]
    fn groups_for_same_agent_are_merged() {
        let txt = parse(
            "User-agent: openspider\nDisallow: /a/\nUser-agent: googlebot\nDisallow: /g/\n\
             User-agent: OpenSpider\nDisallow: /b/\n",
        );
        assert!(blocked(&txt, "openspider", "/a/x"));
        assert!(blocked(&txt, "openspider", "/b/x"));
        assert!(!blocked(&txt, "openspider", "/g/x"));
        let txt = parse("User-agent: *\nDisallow: /a/\nUser-agent: *\nDisallow: /b/\n");
        assert!(blocked(&txt, "x", "/a/1") && blocked(&txt, "x", "/b/1"));
    }

    // spec: CRAWL-098
    #[test]
    fn shared_group_lists_several_agents() {
        let txt = parse("User-agent: googlebot\nUser-agent: openspider\nDisallow: /geteilt/\n");
        assert!(blocked(&txt, "openspider", "/geteilt/x"));
        assert!(blocked(&txt, "googlebot", "/geteilt/x"));
        assert!(!blocked(&txt, "bingbot", "/geteilt/x"));
    }

    // spec: CRAWL-098
    #[test]
    fn longest_pattern_wins_allow_on_tie() {
        let txt = parse(
            "User-agent: *\nDisallow: /privat/\nAllow: /privat/oeffentlich\n\
             Allow: /seite$\nDisallow: /seite\nDisallow: /gleich\nAllow: /gleich\n",
        );
        let m = txt.evaluate("x", "/privat/oeffentlich/a").expect("Treffer");
        assert!(m.allowed);
        assert_eq!(m.line_no, 3);
        assert!(blocked(&txt, "x", "/privat/geheim"));
        assert!(!blocked(&txt, "x", "/seite"));
        assert!(blocked(&txt, "x", "/seite1"));
        // Gleich lang: Allow gewinnt, unabhängig von der Reihenfolge.
        let m = txt.evaluate("x", "/gleich").expect("Treffer");
        assert!(m.allowed);
        assert_eq!(m.line, "Allow: /gleich");
    }

    // spec: CRAWL-098
    #[test]
    fn wildcard_anchor_and_query_matching() {
        let txt = parse("User-agent: *\nDisallow: /*.pdf$\nDisallow: /*?sessionid=\n");
        assert!(blocked(&txt, "x", "/a/b.pdf"));
        assert!(!blocked(&txt, "x", "/a/b.pdf?download=1"));
        assert!(!blocked(&txt, "x", "/a/b.pdfx"));
        assert!(blocked(&txt, "x", "/p?sessionid=42"));
        assert!(!blocked(&txt, "x", "/p?a=1&sessionid=42"));
        assert!(!blocked(&txt, "x", "/p"));
    }

    // spec: CRAWL-098
    #[test]
    fn percent_encoded_comparison() {
        let txt = parse("User-agent: *\nDisallow: /caf\u{e9}/\nDisallow: /a%2Fb\n");
        assert!(blocked(&txt, "x", "/caf%C3%A9/menu"));
        assert!(blocked(&txt, "x", "/caf%c3%a9/menu"));
        assert!(blocked(&txt, "x", "/a%2fb"));
        assert!(!blocked(&txt, "x", "/a/b"));
    }

    // spec: CRAWL-098
    #[test]
    fn empty_disallow_allows_everything() {
        let txt = parse("User-agent: *\nDisallow:\n");
        assert_eq!(txt.rule_count(), 0);
        assert!(txt.evaluate("x", "/alles").is_none());
        let txt = parse("User-agent: *\nDisallow:\nDisallow: /nur-das/\n");
        assert!(blocked(&txt, "x", "/nur-das/a"));
        assert!(!blocked(&txt, "x", "/anderes"));
    }

    // spec: CRAWL-098
    #[test]
    fn unknown_directives_ignored_and_crawl_delay_read() {
        let txt = parse(
            "User-agent: openspider\nCrawl-delay: 2.5\nNoindex: /x\nHost: example.test\n\
             Disallow: /y\n",
        );
        assert_eq!(txt.crawl_delay("openspider"), Some(2.5));
        assert_eq!(txt.crawl_delay("anderer"), None);
        assert!(blocked(&txt, "openspider", "/y"));
        // Crawl-delay sperrt nichts.
        assert!(txt.evaluate("openspider", "/x").is_none());
        // Ungültige Werte werden ignoriert.
        assert_eq!(
            parse("User-agent: *\nCrawl-delay: bald\n").crawl_delay("x"),
            None
        );
    }

    // spec: CRAWL-098
    #[test]
    fn tolerates_bom_line_endings_comments_and_spaces() {
        let crlf =
            "\u{feff}User-agent : *\r\nDisallow :   /a/   # Kommentar\r\n# nur Kommentar\r\n\
                    DISALLOW: /b/\r\n";
        for text in [
            crlf.to_string(),
            crlf.replace("\r\n", "\n"),
            crlf.replace("\r\n", "\r"),
        ] {
            let txt = parse(&text);
            assert!(blocked(&txt, "x", "/a/1"), "{text:?}");
            assert!(blocked(&txt, "x", "/b/1"), "{text:?}");
            let m = txt.evaluate("x", "/b/1").expect("Treffer");
            assert_eq!(m.line_no, 4, "{text:?}");
            assert_eq!(m.matched_line(), "Zeile 4: DISALLOW: /b/");
        }
    }

    // spec: CRAWL-098
    #[test]
    fn matched_line_numbers_count_every_line() {
        let txt = parse("\n# Kopf\n\nUser-agent: *\n\nDisallow: /x\n");
        let m = txt.evaluate("x", "/x").expect("Treffer");
        assert_eq!(m.matched_line(), "Zeile 6: Disallow: /x");
        // Der Regeltext enthält keinen Kommentar.
        let txt = parse("User-agent: *\nDisallow: /y # warum\n");
        assert_eq!(
            txt.evaluate("x", "/y").expect("Treffer").matched_line(),
            "Zeile 2: Disallow: /y"
        );
    }

    // Zeilennummer und Regeltext getrennt für die Oberfläche (B4-STARTFIX).
    // spec: UI-019
    // spec: CRAWL-098
    #[test]
    fn the_matched_line_splits_into_number_and_rule() {
        let txt = parse("User-agent: *\nDisallow: /\n");
        let m = txt.evaluate("x", "/").expect("Treffer");
        assert_eq!(
            RobotsMatch::split_matched_line(&m.matched_line()),
            Some((2, "Disallow: /"))
        );
        assert_eq!(
            RobotsMatch::split_matched_line("Zeile 12: Disallow: /a: b"),
            Some((12, "Disallow: /a: b"))
        );
        for text in ["Disallow: /", "Zeile x: Disallow: /", "Zeile 3 Disallow"] {
            assert_eq!(RobotsMatch::split_matched_line(text), None, "{text}");
        }
    }

    // spec: CRAWL-098
    #[test]
    fn rules_outside_groups_and_group_boundaries_follow_google() {
        // Regeln vor jedem User-agent gelten nirgends.
        let txt = parse("Disallow: /vorab\nUser-agent: *\nDisallow: /x\n");
        assert!(!blocked(&txt, "x", "/vorab"));
        // Sitemap in der Mitte einer Gruppe beendet die Gruppe nicht für ihre Regeln ...
        let txt = parse("User-agent: a\nSitemap: https://s.test/m.xml\nDisallow: /a/\n");
        assert!(blocked(&txt, "a", "/a/x"));
        // ... und auch nicht die Aufzählung der Agenten (K-29): a und b bilden eine Gruppe.
        let txt =
            parse("User-agent: a\nSitemap: https://s.test/m.xml\nUser-agent: b\nDisallow: /b/\n");
        assert!(blocked(&txt, "b", "/b/x"));
        assert!(blocked(&txt, "a", "/b/x"));
        // Dasselbe über unbekannte Zeilen und Crawl-delay hinweg.
        let txt =
            parse("User-agent: a\nHost: x.test\nCrawl-delay: 3\nUser-agent: b\nDisallow: /b/\n");
        assert!(blocked(&txt, "a", "/b/x"));
        assert!(blocked(&txt, "b", "/b/x"));
        assert_eq!(txt.crawl_delay("a"), Some(3.0));
        // Erst nach einer Regelzeile beginnt eine neue Gruppe.
        let txt = parse("User-agent: a\nDisallow: /a/\nSitemap: https://s.test/m.xml\nUser-agent: b\nDisallow: /b/\n");
        assert!(blocked(&txt, "a", "/a/x"));
        assert!(!blocked(&txt, "a", "/b/x"));
        assert!(blocked(&txt, "b", "/b/x"));
        assert!(!blocked(&txt, "b", "/a/x"));
        // Auch eine leere Regel (`Disallow:`) ist eine Regelzeile und beendet die Aufzählung.
        let txt = parse("User-agent: a\nDisallow:\nUser-agent: b\nDisallow: /b/\n");
        assert!(!blocked(&txt, "a", "/b/x"));
        assert!(blocked(&txt, "b", "/b/x"));
    }

    // spec: CRAWL-098
    #[test]
    fn user_agent_value_is_reduced_to_product_token() {
        let txt = parse("User-agent: OpenSpider/1.0 (+https://openspider.de)\nDisallow: /x\n");
        assert!(blocked(&txt, "openspider", "/x"));
        assert!(blocked(&txt, "OpenSpider/2.0", "/x"));
    }

    // spec: CRAWL-098
    #[test]
    fn only_first_500_kib_are_evaluated() {
        let mut text = String::from("User-agent: *\nDisallow: /vorne/\n");
        while text.len() < MAX_BYTES - 80 {
            text.push_str("# Füllzeile ohne Bedeutung\n");
        }
        while text.len() < MAX_BYTES - 25 {
            text.push('\n');
        }
        text.push_str("Disallow: /hinten/\n");
        assert!(text.len() < MAX_BYTES);
        while text.len() <= MAX_BYTES {
            text.push_str("# Füllzeile ohne Bedeutung\n");
        }
        text.push_str("Disallow: /ganz-hinten/\n");
        let txt = parse(&text);
        assert!(blocked(&txt, "x", "/vorne/a"));
        assert!(blocked(&txt, "x", "/hinten/a"), "Regel vor der Grenze gilt");
        assert!(
            !blocked(&txt, "x", "/ganz-hinten/a"),
            "Regel nach 500 KiB entfällt"
        );
    }

    // spec: CRAWL-098
    #[test]
    fn truncated_last_line_is_dropped() {
        // Die 500-KiB-Grenze liegt hinter `Disallow: /priv`: das angeschnittene Muster `/priv` darf nicht
        // gelten (es sperrte mehr als `/privat/geheim`).
        let mut text = String::from(
            "User-agent: *
",
        );
        while text.len() < MAX_BYTES - "Disallow: /priv".len() {
            text.push('\n');
        }
        text.push_str("Disallow: /privat/geheim\n");
        assert!(text.len() > MAX_BYTES);
        let txt = parse(&text);
        assert_eq!(txt.rule_count(), 0);
        assert!(txt.evaluate("x", "/privat/anderes").is_none());
        // Endet die Zeile genau an der Grenze, bleibt sie erhalten.
        let mut text = String::from("User-agent: *\n");
        while text.len() < MAX_BYTES - "Disallow: /privat/geheim\n".len() {
            text.push('\n');
        }
        text.push_str("Disallow: /privat/geheim\nDisallow: /danach\n");
        let txt = parse(&text);
        assert_eq!(txt.rule_count(), 1);
    }

    // spec: CRAWL-098
    #[test]
    fn multi_megabyte_files_do_not_crash() {
        let mut text = String::from("User-agent: *\nDisallow: /a/\n");
        let filler = "Disallow: /*/*/*/x$\nAllow: /ä/ö/ü\n";
        while text.len() < 3 * 1024 * 1024 {
            text.push_str(filler);
        }
        let txt = parse(&text);
        assert!(blocked(&txt, "x", "/a/b"));
        assert!(txt.rule_count() > 0);
        // Auch ein 3-MB-Text ohne Zeilenumbruch und mit Mehrbyte-Zeichen an der Grenze.
        let one_line = "ä".repeat(3 * 1024 * 1024);
        assert_eq!(parse(&one_line).rule_count(), 0);
    }

    // spec: CRAWL-098
    #[test]
    fn html_and_empty_bodies_yield_no_rules() {
        assert_eq!(parse("").rule_count(), 0);
        let html = "<!doctype html>\n<html><head><title>Nicht gefunden</title></head>\n\
                    <body><h1>404</h1><p>Seite: nicht da</p></body></html>\n";
        let txt = parse(html);
        assert_eq!(txt.rule_count(), 0);
        assert!(txt.evaluate("x", "/seite").is_none());
    }
}
