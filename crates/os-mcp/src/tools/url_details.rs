//! Werkzeug `url_details` (AGENT-012, AGENT-005): alles zu einer URL des Crawls in wählbaren Abschnitten.
//!
//! Abschnitte: `overview` (die Spalten des Tabs Internal bzw. External, dieselben SQL-Ausdrücke wie die
//! Tab-Ansicht), `inlinks`/`outlinks` (paginiert, je Abschnitt ein eigener Cursor), `headers`, `issues`,
//! `directives`, `canonicals`, `hreflang`, `structured_data` und `rendering` (in M1 nicht erhoben).
//!
//! Eine URL, die nicht gecrawlt wurde, liefert statt Daten eine klare Meldung mit dem Grund (ausgeschlossen,
//! robots.txt, Limit, nicht gefunden …). Alle seitenstämmigen Werte stehen im Untrusted-Block, die
//! Zusammenfassung enthält nur Zähler, Zustände und Katalogdaten.

use os_analyze::issues::{self, CatalogEntry};
use os_analyze::state::{filter_state, AnalysisSnapshot};
use os_analyze::views::{self, ViewQuery};
use os_analyze::{tabs, ColumnDef, ColumnKind, ColumnSql, TabDef};
use os_model::{
    CrawlState, EndReason, FilterState, IssuePriority, IssueType, UrlId, UrlState, Value,
};
use os_store::queries::{self, LinkView, PageReq};
use serde_json::{json, Map, Value as Json};

use super::common::{internal, object, open_crawl, opt_str, reject_unknown, view_error, OpenCrawl};
use crate::paging::{self, PageArgs};
use crate::tool::{Tool, ToolClass, ToolContext, ToolError, ToolOutput};

/// Name des Werkzeugs.
pub const NAME: &str = "url_details";

/// Alle Abschnitte in der Reihenfolge der Antwort.
pub const SECTIONS: &[&str] = &[
    "overview",
    "inlinks",
    "outlinks",
    "headers",
    "issues",
    "directives",
    "canonicals",
    "hreflang",
    "structured_data",
    "rendering",
];

/// Hinweis der Abschnitte, die M1 nicht erhebt (Rendering und strukturierte Daten kommen mit M2).
pub const NOT_COLLECTED: &str = "in M1 nicht erhoben";

/// Das Werkzeug `url_details`.
pub struct UrlDetails;

impl Tool for UrlDetails {
    fn name(&self) -> &'static str {
        NAME
    }

    fn class(&self) -> ToolClass {
        ToolClass::Read
    }

    fn description(&self) -> &'static str {
        "Details of one URL of a crawl. The URL is given as url (address as in query_urls) or url_id (from \
         query_urls). sections selects sections: overview (columns of the Internal or External tab), \
         inlinks, outlinks (paginated with limit and a cursor per section: cursor if only one link section \
         is requested, otherwise inlinks_cursor or outlinks_cursor; the summary gives total and next_cursor \
         per section), headers, issues (which catalog issues affect the URL), directives, canonicals, \
         hreflang, structured_data and rendering (both not collected in M1). Without sections all are \
         returned. If the URL was not crawled, reason and message give the cause (excluded, robots_txt, \
         url_limit, depth_limit, queued, not_found and others)."
    }

    fn input_schema(&self) -> Json {
        json!({
            "type": "object",
            "properties": {
                "crawl_id": {"type": "string", "description": "ID from list_crawls"},
                "url": {"type": "string", "description": "Address of the URL"},
                "url_id": {"type": "integer", "description": "ID of the URL (url_ids from query_urls)"},
                "sections": {
                    "description": "Sections (default: all)",
                    "oneOf": [
                        {"type": "string"},
                        {"type": "array", "items": {"type": "string", "enum": SECTIONS}}
                    ]
                },
                "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 50,
                          "description": "Rows per page of the link sections"},
                "cursor": {"type": "string", "description": "Continuation if only inlinks OR outlinks is requested"},
                "inlinks_cursor": {"type": "string", "description": "Continuation of the inlinks"},
                "outlinks_cursor": {"type": "string", "description": "Continuation of the outlinks"}
            },
            "required": ["crawl_id"],
            "additionalProperties": false
        })
    }

    fn call(&self, ctx: &ToolContext, args: Json) -> Result<ToolOutput, ToolError> {
        reject_unknown(
            &args,
            &[
                "crawl_id",
                "url",
                "url_id",
                "sections",
                "limit",
                "cursor",
                "inlinks_cursor",
                "outlinks_cursor",
            ],
        )?;
        let page = PageArgs::from_args(&args)?;
        let sections = parse_sections(&args)?;
        let wants = |s: &str| sections.iter().any(|x| x == s);
        let cursors = LinkCursors::parse(
            &args,
            page.cursor.as_deref(),
            wants("inlinks"),
            wants("outlinks"),
        )?;
        let lookup = parse_target(&args)?;
        let crawl = open_crawl(ctx, &args)?;
        let crawl_id = crawl.id.to_string();

        let Some(url_id) = find_url(&crawl, &lookup)? else {
            return Ok(ToolOutput {
                summary: object(vec![
                    ("crawl_id", json!(crawl_id)),
                    ("found", json!(false)),
                    ("crawled", json!(false)),
                    ("reason", json!("not_found")),
                    ("message", json!(reason_text("not_found"))),
                ]),
                untrusted: None,
                total: None,
                next_cursor: None,
            });
        };

        let facts = UrlFacts::load(&crawl, url_id)?;
        let status = crawl_status(&crawl, &facts);
        let mut summary: Vec<(&str, Json)> = vec![
            ("crawl_id", json!(crawl_id)),
            ("url_id", json!(url_id.0)),
            ("found", json!(true)),
            ("crawled", json!(status.crawled)),
            ("state", json!(facts.state.as_str())),
            ("sections", json!(sections)),
        ];
        if let Some(reason) = status.reason {
            summary.push(("reason", json!(reason)));
            summary.push(("message", json!(reason_text(reason))));
        }
        let mut untrusted = Map::new();

        if wants("overview") {
            untrusted.insert("overview".into(), overview(&crawl, &facts)?);
        }

        let tab_defs = tabs::configured_for_file(&crawl.cfg, &crawl.file);
        let mut list_sections = 0;
        for (name, direction) in [("inlinks", Direction::In), ("outlinks", Direction::Out)] {
            if !wants(name) {
                continue;
            }
            list_sections += 1;
            let binding = format!("url_details|{crawl_id}|{}|{name}", url_id.0);
            let cursor = cursors.for_section(direction);
            let offset = match cursor {
                Some(c) => paging::decode_cursor(c, &binding)?,
                None => 0,
            };
            let req = PageReq::new(offset, page.limit);
            let result = match direction {
                Direction::In => queries::inlinks(crawl.file.conn(), url_id, req),
                Direction::Out => queries::outlinks(crawl.file.conn(), url_id, req),
            }
            .map_err(internal)?;
            let end = offset + result.items.len() as u64;
            let next = (end < result.total).then(|| paging::encode_cursor(&binding, end));
            summary.push((
                name,
                json!({"total": result.total, "returned": result.items.len(), "next_cursor": next}),
            ));
            untrusted.insert(
                name.into(),
                Json::Array(result.items.iter().map(link_json).collect()),
            );
        }

        if wants("headers") {
            let headers = queries::headers(crawl.file.conn(), url_id).map_err(internal)?;
            untrusted.insert(
                "headers".into(),
                Json::Array(
                    headers
                        .iter()
                        .map(|h| {
                            json!({
                                "direction": if h.request { "request" } else { "response" },
                                "position": h.position,
                                "name": h.name,
                                "value": h.value,
                            })
                        })
                        .collect(),
                ),
            );
        }

        if wants("issues") {
            if status.crawled {
                let found = url_issues(&crawl, &tab_defs, url_id)?;
                summary.push(("issues", Json::Array(found.issues)));
                summary.push(("issues_not_evaluated", json!(found.not_evaluated)));
            } else {
                summary.push(("issues", json!([])));
                summary.push(("issues_not_evaluated", json!(0)));
            }
        }

        if wants("directives") {
            untrusted.insert("directives".into(), directives(&crawl, url_id)?);
        }
        if wants("canonicals") {
            untrusted.insert("canonicals".into(), canonicals(&crawl, url_id)?);
        }
        if wants("hreflang") {
            untrusted.insert("hreflang".into(), hreflang(&crawl, url_id)?);
        }
        if wants("structured_data") {
            let details =
                os_store::queries::structured_data::detail_rows(&crawl.file, &[url_id], 0, 500)
                    .map_err(internal)?;
            if details.total > 0 {
                summary.push((
                    "structured_data",
                    json!({"status":"collected","total":details.total,"columns":details.columns}),
                ));
                untrusted.insert(
                    "structured_data".into(),
                    json!(details
                        .rows
                        .iter()
                        .map(|r| r.iter().map(super::common::value_json).collect::<Vec<_>>())
                        .collect::<Vec<_>>()),
                );
            } else {
                summary.push((
                    "structured_data",
                    json!({"status":"not_collected","message":NOT_COLLECTED}),
                ));
            }
        }
        if wants("rendering") {
            {
                let id = url_id;
                let mut stmt = crawl.file.conn().prepare("SELECT status,word_count,load_ms,wait_ms,total_ms FROM render_pages WHERE url_id=?1").map_err(internal)?;
                let rows = stmt.query_map([id.0], |r| Ok(json!({"status":r.get::<_,String>(0)?,"word_count":r.get::<_,Option<i64>>(1)?,"load_ms":r.get::<_,i64>(2)?,"wait_ms":r.get::<_,i64>(3)?,"total_ms":r.get::<_,i64>(4)?}))).map_err(internal)?.collect::<Result<Vec<_>,_>>().map_err(internal)?;
                if let Some(row) = rows.first() {
                    summary.push(("rendering", row.clone()));
                    let raw = os_store::queries::stored_html(&crawl.file, id)
                        .map_err(internal)?
                        .map(|v| String::from_utf8_lossy(&v.bytes).into_owned());
                    let dom = os_store::queries::stored_rendered_html(&crawl.file, id)
                        .map_err(internal)?
                        .map(|v| String::from_utf8_lossy(&v.bytes).into_owned());
                    untrusted.insert("rendering".into(), json!({"html_changed":raw.as_ref().zip(dom.as_ref()).map(|(a,b)| a != b),"original_html":raw,"rendered_html":dom}));
                } else {
                    summary.push((
                        "rendering",
                        json!({"status":"not_collected","message":NOT_COLLECTED}),
                    ));
                }
            }
        }

        // Top-Level-Paging nur, wenn genau ein Linkabschnitt angefragt ist (AGENT-005); sonst steht es je
        // Abschnitt in der Zusammenfassung.
        let (total, next_cursor) = if list_sections == 1 {
            let key = if wants("inlinks") {
                "inlinks"
            } else {
                "outlinks"
            };
            let section = summary
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(Json::Null);
            (
                section["total"].as_u64(),
                section["next_cursor"].as_str().map(str::to_string),
            )
        } else {
            (None, None)
        };

        Ok(ToolOutput {
            summary: object(summary),
            untrusted: (!untrusted.is_empty()).then_some(Json::Object(untrusted)),
            total,
            next_cursor,
        })
    }
}

#[derive(Clone, Copy)]
enum Direction {
    In,
    Out,
}

/// Fortsetzungen der beiden Linkabschnitte.
struct LinkCursors<'a> {
    inlinks: Option<&'a str>,
    outlinks: Option<&'a str>,
}

impl<'a> LinkCursors<'a> {
    fn parse(
        args: &'a Json,
        generic: Option<&'a str>,
        wants_in: bool,
        wants_out: bool,
    ) -> Result<Self, ToolError> {
        let mut inlinks = opt_str(args, "inlinks_cursor")?;
        let mut outlinks = opt_str(args, "outlinks_cursor")?;
        if let Some(c) = generic {
            match (wants_in, wants_out) {
                (true, false) => inlinks = inlinks.or(Some(c)),
                (false, true) => outlinks = outlinks.or(Some(c)),
                _ => {
                    return Err(ToolError::InvalidArguments(
                        "cursor passt nur, wenn genau einer der Abschnitte inlinks oder outlinks angefragt \
                         ist. Bei beiden Abschnitten inlinks_cursor und outlinks_cursor (je next_cursor des \
                         Abschnitts) verwenden."
                            .to_string(),
                    ))
                }
            }
        }
        Ok(LinkCursors { inlinks, outlinks })
    }

    fn for_section(&self, direction: Direction) -> Option<&'a str> {
        match direction {
            Direction::In => self.inlinks,
            Direction::Out => self.outlinks,
        }
    }
}

/// Wie die URL angegeben wurde.
enum Target {
    Address(String),
    Id(i64),
}

fn parse_target(args: &Json) -> Result<Target, ToolError> {
    let by_url = opt_str(args, "url")?;
    let by_id = match args.get("url_id") {
        None | Some(Json::Null) => None,
        Some(v) => Some(v.as_i64().ok_or_else(|| {
            ToolError::InvalidArguments(format!(
                "url_id muss eine ganze Zahl sein (url_ids aus query_urls), erhalten: {v}"
            ))
        })?),
    };
    match (by_url, by_id) {
        (Some(_), Some(_)) => Err(ToolError::InvalidArguments(
            "url und url_id schließen sich aus: eines von beiden angeben".to_string(),
        )),
        (Some(u), None) => Ok(Target::Address(u.to_string())),
        (None, Some(id)) => Ok(Target::Id(id)),
        (None, None) => Err(ToolError::InvalidArguments(
            "url oder url_id fehlt (eines von beiden angeben)".to_string(),
        )),
    }
}

fn parse_sections(args: &Json) -> Result<Vec<String>, ToolError> {
    let normalize = |s: &str| s.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    let wanted: Vec<String> = match args.get("sections") {
        None | Some(Json::Null) => return Ok(SECTIONS.iter().map(|s| (*s).to_string()).collect()),
        Some(Json::String(s)) => vec![normalize(s)],
        Some(Json::Array(items)) => items
            .iter()
            .map(|i| {
                i.as_str().map(normalize).ok_or_else(|| {
                    ToolError::InvalidArguments(format!(
                        "sections darf nur Abschnittsnamen (Zeichenketten) enthalten, erhalten: {i}"
                    ))
                })
            })
            .collect::<Result<_, _>>()?,
        Some(other) => {
            return Err(ToolError::InvalidArguments(format!(
                "sections muss eine Liste von Abschnittsnamen sein, erhalten: {other}"
            )))
        }
    };
    if wanted.iter().any(|w| w == "all") {
        return Ok(SECTIONS.iter().map(|s| (*s).to_string()).collect());
    }
    if wanted.is_empty() {
        return Ok(SECTIONS.iter().map(|s| (*s).to_string()).collect());
    }
    for w in &wanted {
        if !SECTIONS.contains(&w.as_str()) {
            return Err(ToolError::InvalidArguments(format!(
                "Unbekannter Abschnitt \"{w}\". Gültige Abschnitte: {}, oder all.",
                SECTIONS.join(", ")
            )));
        }
    }
    // In der Reihenfolge der Antwort, ohne Dubletten.
    Ok(SECTIONS
        .iter()
        .filter(|s| wanted.iter().any(|w| w == **s))
        .map(|s| (*s).to_string())
        .collect())
}

/// Sucht die Kennung der URL (Adresse in kodierter oder Anzeigeform; ohne Pfad auch mit `/`).
fn find_url(crawl: &OpenCrawl, target: &Target) -> Result<Option<UrlId>, ToolError> {
    let conn = crawl.file.conn();
    let lookup_id = |id: i64| -> Result<Option<UrlId>, ToolError> {
        let n: i64 = conn
            .query_row(
                "SELECT COALESCE((SELECT id FROM urls WHERE id = ?1), -1)",
                [id],
                |r| r.get(0),
            )
            .map_err(internal)?;
        Ok((n >= 0).then_some(UrlId(n)))
    };
    match target {
        Target::Id(id) => lookup_id(*id),
        Target::Address(raw) => {
            for candidate in address_candidates(raw) {
                let n: i64 = conn
                    .query_row(
                        "SELECT COALESCE((SELECT id FROM urls WHERE url = ?1 OR address = ?1 \
                         ORDER BY (url = ?1) DESC LIMIT 1), -1)",
                        [candidate.as_str()],
                        |r| r.get(0),
                    )
                    .map_err(internal)?;
                if n >= 0 {
                    return Ok(Some(UrlId(n)));
                }
            }
            Ok(None)
        }
    }
}

fn address_candidates(raw: &str) -> Vec<String> {
    let mut out = vec![raw.to_string()];
    let without_fragment = raw.split('#').next().unwrap_or(raw).to_string();
    if without_fragment != raw {
        out.push(without_fragment.clone());
    }
    // `http://host` ohne Pfad ist im Crawl `http://host/`.
    if let Some((_, rest)) = without_fragment.split_once("://") {
        if !rest.contains(['/', '?']) {
            out.push(format!("{without_fragment}/"));
        }
    }
    out
}

/// Zeile der Tabelle `urls`, soweit für Zustand und Grund nötig.
struct UrlFacts {
    id: UrlId,
    state: UrlState,
    not_crawled_reason: Option<String>,
    robots_blocked: bool,
    blocked_without_response: bool,
    is_internal: bool,
    in_tables: bool,
}

impl UrlFacts {
    fn load(crawl: &OpenCrawl, id: UrlId) -> Result<Self, ToolError> {
        let row = queries::url_by_id(crawl.file.conn(), id)
            .map_err(internal)?
            .ok_or_else(|| ToolError::NotFound("URL nicht gefunden".to_string()))?;
        let text = json_query(
            crawl,
            "SELECT json_object('robots_blocked', robots_blocked, 'no_response_reason', no_response_reason, \
             'in_tables', in_tables) FROM urls WHERE id = ?1",
            id,
        )?;
        let blocked_without_response = matches!(
            text["no_response_reason"].as_str(),
            Some("blocked_by_robots" | "blocked_by_robots_unreachable")
        );
        Ok(UrlFacts {
            id,
            state: row.state,
            not_crawled_reason: row.not_crawled_reason.map(|r| r.as_str().to_string()),
            robots_blocked: text["robots_blocked"].as_i64().unwrap_or(0) != 0,
            blocked_without_response,
            is_internal: row.is_internal,
            in_tables: text["in_tables"].as_i64().unwrap_or(1) != 0,
        })
    }
}

/// Ob und warum eine URL (nicht) abgerufen wurde.
struct CrawlStatus {
    crawled: bool,
    reason: Option<&'static str>,
}

fn crawl_status(crawl: &OpenCrawl, facts: &UrlFacts) -> CrawlStatus {
    if facts.state == UrlState::Done {
        if facts.robots_blocked && facts.blocked_without_response {
            return CrawlStatus {
                crawled: false,
                reason: Some("robots_txt"),
            };
        }
        return CrawlStatus {
            crawled: true,
            reason: None,
        };
    }
    let reason = match facts.not_crawled_reason.as_deref() {
        Some("excluded") => "excluded",
        Some("not_included") => "not_included",
        Some("outside_scope") => "outside_scope",
        Some("store_only") => "store_only",
        Some("crawl_disabled") => "crawl_disabled",
        Some("nofollow") => "nofollow",
        Some("depth_limit") => "depth_limit",
        Some("url_limit") => "url_limit",
        Some("redirect_limit") => "redirect_limit",
        Some("external_not_checked") => "external_not_checked",
        Some("list_mode_depth") => "list_mode_depth",
        _ => {
            // Kein ausdrücklicher Grund: Die URL wartete noch in der Warteschlange.
            if crawl.meta.end_reason == Some(EndReason::LimitReached) {
                "url_limit"
            } else if matches!(
                crawl.meta.state,
                CrawlState::Running | CrawlState::Paused | CrawlState::Incomplete
            ) {
                "queued"
            } else {
                "not_crawled"
            }
        }
    };
    CrawlStatus {
        crawled: false,
        reason: Some(reason),
    }
}

/// Klartext zu einem Grund.
pub fn reason_text(reason: &str) -> &'static str {
    match reason {
        "not_found" => {
            "Die URL kommt in diesem Crawl nicht vor: sie wurde nie entdeckt (nicht verlinkt und nicht in \
             Sitemap oder URL-Liste) oder gehörte zu einem verworfenen Sitemap-Eintrag eines fremden Hosts."
        }
        "excluded" => "Von einer Exclude-Regel der Crawl-Konfiguration ausgeschlossen; sie wurde gefunden, aber nicht abgerufen.",
        "not_included" => "Passt auf keine Include-Regel der Crawl-Konfiguration und wurde deshalb nicht abgerufen.",
        "outside_scope" => "Liegt außerhalb des Crawl-Bereichs (Start-URL, Subdomains, Ordner) und wurde nicht abgerufen.",
        "store_only" => "Gefunden und gespeichert, aber laut Konfiguration nicht abgerufen (speichern ohne crawlen).",
        "crawl_disabled" => "Das Crawlen dieser URL-Art ist in der Konfiguration ausgeschaltet; sie wurde nicht abgerufen.",
        "nofollow" => "Nur über nofollow-Links erreichbar; nofollow-Links werden in dieser Konfiguration nicht verfolgt.",
        "depth_limit" => "Das Tiefenlimit des Crawls verhinderte den Abruf.",
        "url_limit" => "Das URL-Limit des Crawls war erreicht, bevor die URL abgerufen wurde.",
        "redirect_limit" => "Das Weiterleitungslimit des Crawls verhinderte den Abruf.",
        "external_not_checked" => "Externe URL; externe Links werden in dieser Konfiguration nicht geprüft.",
        "list_mode_depth" => "Listenmodus: Nur die URLs der Liste werden abgerufen, nicht deren Verweise.",
        "queued" => "Noch nicht abgerufen: Die URL wartet in der Warteschlange, der Crawl ist nicht zu Ende gelaufen.",
        "robots_txt" => "Durch die robots.txt gesperrt und deshalb nicht abgerufen.",
        _ => "Die URL wurde nicht abgerufen; der Crawl nennt keinen Grund.",
    }
}

/// Führt eine Abfrage aus, die genau einen JSON-Text liefert (`?1` = URL-Kennung).
fn json_query(crawl: &OpenCrawl, sql: &str, id: UrlId) -> Result<Json, ToolError> {
    let text: String = crawl
        .file
        .conn()
        .query_row(sql, [id.0], |r| r.get(0))
        .map_err(internal)?;
    serde_json::from_str(&text).map_err(internal)
}

// ---------------------------------------------------------------------------------------------------------
// Übersicht

/// Die Spalten des Tabs Internal bzw. External (alle, Referenzspalten zuerst) mit den SQL-Ausdrücken der
/// Tab-Ansicht; URLs außerhalb der Tabellen (`in_tables = 0`) zeigen die Kerndaten.
fn overview(crawl: &OpenCrawl, facts: &UrlFacts) -> Result<Json, ToolError> {
    let tab_id = match (facts.in_tables, facts.is_internal) {
        (true, true) => "Internal",
        (true, false) => "External",
        (false, _) => return core_overview(crawl, facts),
    };
    let tab_defs = tabs::configured_for_file(&crawl.cfg, &crawl.file);
    let Some(tab) = tab_defs.iter().find(|t| t.id == tab_id) else {
        return core_overview(crawl, facts);
    };
    let mut ordered: Vec<&ColumnDef> = tab.columns.iter().filter(|c| !c.extension).collect();
    ordered.extend(tab.columns.iter().filter(|c| c.extension));

    let scalars: Vec<&ColumnDef> = ordered
        .iter()
        .copied()
        .filter(|c| matches!(c.sql, ColumnSql::Expr(_)))
        .collect();
    let exprs: Vec<String> = scalars
        .iter()
        .filter_map(|c| match &c.sql {
            ColumnSql::Expr(e) => Some(format!("({e})")),
            ColumnSql::Multi { .. }
            | ColumnSql::MultiCrawlWide { .. }
            | ColumnSql::MultiPaired { .. } => None,
        })
        .collect();
    let sql = format!(
        "SELECT json_array({}) FROM {} WHERE u.id = ?1",
        exprs.join(", "),
        views::URLS_FROM
    );
    // K2-61: Konfigurationsparameter zusätzlicher Internal-Spalten (M2-D02).
    let mut params = (tab.params)(&crawl.cfg)
        .into_iter()
        .map(|(name, value)| (format!(":{name}"), value))
        .collect::<Vec<_>>();
    tabs::content::bind_analysis_params(&mut params, &crawl.file, &crawl.cfg)
        .map_err(view_error)?;
    let sql = inline_params(&sql, &params).map_err(internal)?;
    let values = json_query(crawl, &sql, facts.id)?;
    let values = values.as_array().cloned().unwrap_or_default();

    let mut out = Map::new();
    for (col, value) in scalars.iter().zip(values) {
        out.insert(col.key.to_string(), cell_json(col.kind, value));
    }
    for col in ordered {
        match &col.sql {
            ColumnSql::Multi { list_sql, .. } | ColumnSql::MultiCrawlWide { list_sql, .. } => {
                let sql = format!("WITH l(v) AS ({list_sql}) SELECT json_group_array(v) FROM l");
                out.insert(col.key.to_string(), json_query(crawl, &sql, facts.id)?);
            }
            ColumnSql::MultiPaired { base, parts } => {
                let mut obj = Map::new();
                for part in *parts {
                    let sql = format!(
                        "WITH l(v) AS ({}) SELECT json_group_array(v) FROM l",
                        part.list_sql
                    );
                    let name = if part.suffix.is_empty() {
                        (*base).to_string()
                    } else {
                        format!("{base} {}", part.suffix)
                    };
                    obj.insert(name, json_query(crawl, &sql, facts.id)?);
                }
                out.insert(col.key.to_string(), Json::Object(obj));
            }
            ColumnSql::Expr(_) => {}
        }
    }
    Ok(Json::Object(out))
}

/// Kerndaten einer URL, die in keinem Tab erscheint.
fn core_overview(crawl: &OpenCrawl, facts: &UrlFacts) -> Result<Json, ToolError> {
    let row = queries::url_by_id(crawl.file.conn(), facts.id)
        .map_err(internal)?
        .ok_or_else(|| ToolError::NotFound("URL nicht gefunden".to_string()))?;
    Ok(json!({
        "Address": row.address,
        "URL Encoded Address": row.url,
        "Internal": row.is_internal,
        "State": row.state.as_str(),
        "Not Crawled Reason": row.not_crawled_reason.map(|r| r.as_str()),
        "Crawl Depth": row.crawl_depth,
        "Status Code": row.status_code,
        "Status": row.status,
        "Content Class": row.content_class.as_str(),
        "Indexability": row.indexability.map(|i| i.as_str()),
        "Indexability Status": row.indexability_status.map(|i| i.as_str()),
    }))
}

fn round_to(x: f64, decimals: u8) -> f64 {
    let factor = 10f64.powi(i32::from(decimals));
    (x * factor).round() / factor
}

/// Zellwert wie in der Tab-Ansicht (`views`): Wahrheitswerte, Sekunden (3 Stellen) statt Millisekunden,
/// gerundete Dezimalzahlen, Text bleibt Text.
fn cell_json(kind: ColumnKind, value: Json) -> Json {
    let Json::Number(n) = &value else {
        return value;
    };
    match kind {
        ColumnKind::Bool => json!(n.as_f64().is_some_and(|x| x != 0.0)),
        ColumnKind::Seconds3 => json!(round_to(n.as_f64().unwrap_or(0.0) / 1000.0, 3)),
        ColumnKind::Float { decimals } => json!(round_to(n.as_f64().unwrap_or(0.0), decimals)),
        ColumnKind::Int => match n.as_i64() {
            Some(i) => json!(i),
            None => json!(n.as_f64().map_or(0, |x| x as i64)),
        },
        ColumnKind::Text | ColumnKind::Url | ColumnKind::Hash | ColumnKind::Timestamp => {
            json!(n.to_string())
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// Links

fn link_json(link: &LinkView) -> Json {
    json!({
        "type": link.link_type.as_str(),
        "source_id": link.source_id.0,
        "source": link.source_address,
        "destination_id": link.target_id.map(|t| t.0),
        "destination": link.target_address,
        "anchor": link.anchor,
        "alt_text": link.alt_text,
        "follow": link.follow,
        "status_code": link.status_code,
        "status": link.status,
    })
}

// ---------------------------------------------------------------------------------------------------------
// Directives, Canonicals, hreflang

/// Wandelt Ganzzahlfelder (0/1) der genannten Schlüssel in Wahrheitswerte.
fn flags_to_bool(rows: &mut Json, keys: &[&str]) {
    let Some(items) = rows.as_array_mut() else {
        return;
    };
    for item in items {
        let Some(map) = item.as_object_mut() else {
            continue;
        };
        for key in keys {
            if let Some(n) = map.get(*key).and_then(Json::as_i64) {
                map.insert((*key).to_string(), json!(n != 0));
            }
        }
    }
}

fn directives(crawl: &OpenCrawl, id: UrlId) -> Result<Json, ToolError> {
    let mut rows = json_query(
        crawl,
        "SELECT COALESCE(json_group_array(json_object('source', source, 'position', position, \
         'agent', agent, 'raw', raw, 'tokens', tokens, 'applies', applies, 'in_head', in_head)), '[]') \
         FROM (SELECT * FROM directives WHERE url_id = ?1 ORDER BY source, position)",
        id,
    )?;
    flags_to_bool(&mut rows, &["applies", "in_head"]);
    let mut effective = json_query(
        crawl,
        "SELECT json_object('index', effective_index, 'follow', effective_follow) FROM urls WHERE id = ?1",
        id,
    )?;
    for key in ["index", "follow"] {
        if let Some(n) = effective[key].as_i64() {
            effective[key] = json!(n != 0);
        }
    }
    Ok(json!({"directives": rows, "effective": effective}))
}

fn canonicals(crawl: &OpenCrawl, id: UrlId) -> Result<Json, ToolError> {
    let mut rows = json_query(
        crawl,
        "SELECT COALESCE(json_group_array(json_object('source', source, 'position', position, \
         'raw', raw, 'resolved', resolved, 'target_id', target_id, 'in_head', in_head)), '[]') \
         FROM (SELECT * FROM canonicals WHERE url_id = ?1 ORDER BY source, position)",
        id,
    )?;
    flags_to_bool(&mut rows, &["in_head"]);
    let mut summary = json_query(
        crawl,
        "SELECT json_object('count', canonical_count, 'url', canonical_url, 'self_referencing', canonical_self, \
         'non_indexable_canonical', EXISTS(SELECT 1 FROM url_flags f WHERE f.url_id = u.id AND \
         f.flag = 'non_indexable_canonical')) FROM urls u WHERE u.id = ?1",
        id,
    )?;
    for key in ["self_referencing", "non_indexable_canonical"] {
        if let Some(n) = summary[key].as_i64() {
            summary[key] = json!(n != 0);
        }
    }
    Ok(json!({"canonicals": rows, "summary": summary}))
}

fn hreflang(crawl: &OpenCrawl, id: UrlId) -> Result<Json, ToolError> {
    // Die gespeicherten Zeilen (HTML-Kopf, HTTP-Header, Sitemap) in Dokument-Reihenfolge.
    let mut rows = json_query(
        crawl,
        "SELECT COALESCE(json_group_array(json_object('source', source, 'position', position, \
         'code', code_raw, 'code_normalized', code_norm, 'code_valid', code_valid, \
         'invalid_reason', invalid_reason, 'href', href_raw, 'url', url, 'target_id', target_id, \
         'in_head', in_head)), '[]') \
         FROM (SELECT * FROM hreflang WHERE url_id = ?1 ORDER BY source, position)",
        id,
    )?;
    flags_to_bool(&mut rows, &["code_valid", "in_head"]);
    Ok(rows)
}

// ---------------------------------------------------------------------------------------------------------
// Issues der URL

struct UrlIssues {
    issues: Vec<Json>,
    not_evaluated: usize,
}

fn type_rank(t: IssueType) -> u8 {
    match t {
        IssueType::Issue => 0,
        IssueType::Warning => 1,
        IssueType::Opportunity => 2,
    }
}

fn priority_rank(p: IssuePriority) -> u8 {
    match p {
        IssuePriority::High => 0,
        IssuePriority::Medium => 1,
        IssuePriority::Low => 2,
    }
}

/// Die Issues des Katalogs, in deren Filter die URL liegt (derselbe Plan wie Zähler und Tab-Ansicht, auf die
/// eine URL eingeschränkt). Nicht auswertbare Filter (Crawl-Analyse fehlt …) zählt `not_evaluated`.
fn url_issues(crawl: &OpenCrawl, tab_defs: &[TabDef], id: UrlId) -> Result<UrlIssues, ToolError> {
    let cfg_json = serde_json::to_value(&crawl.cfg).unwrap_or(Json::Null);
    let snapshot = AnalysisSnapshot::load(&crawl.file).map_err(internal)?;
    let mut found: Vec<(&'static CatalogEntry, &TabDef, String)> = Vec::new();
    let mut not_evaluated = 0;
    for tab in tab_defs {
        for filter in &tab.filters {
            let Some(entry) = issues::entry_for(tab.id, filter.id) else {
                continue;
            };
            if filter_state(filter, &crawl.cfg, &cfg_json, &snapshot) != FilterState::Ready {
                not_evaluated += 1;
                continue;
            }
            let plan =
                views::plan(&ViewQuery::new(tab.id, filter.id), &crawl.cfg).map_err(view_error)?;
            let sql = format!(
                "SELECT EXISTS(SELECT 1 FROM {} WHERE {} AND {} AND u.id = ?1)",
                views::URLS_FROM,
                inline_params(&plan.population_sql, &plan.params).map_err(internal)?,
                inline_params(&plan.filter_sql, &plan.params).map_err(internal)?
            );
            let hit: i64 = crawl
                .file
                .conn()
                .query_row(&sql, [id.0], |r| r.get(0))
                .map_err(internal)?;
            if hit != 0 {
                let label = os_analyze::names::label_with(filter.id, &cfg_json);
                found.push((entry, tab, label));
            }
        }
    }
    found.sort_by_key(|(e, _, _)| (type_rank(e.issue_type), priority_rank(e.priority)));
    let issues = found
        .into_iter()
        .map(|(entry, tab, label)| {
            json!({
                "id": entry.id,
                "type": entry.issue_type.as_str(),
                "priority": entry.priority.as_str(),
                "tab": tab.id,
                "filter": label,
            })
        })
        .collect();
    Ok(UrlIssues {
        issues,
        not_evaluated,
    })
}

/// Setzt die benannten Parameter eines Plans als Literale in das SQL ein (Zeichenketten in einfachen
/// Anführungszeichen werden übersprungen). Die Werte stammen aus der Crawl-Konfiguration, nie aus
/// Argumenten des Aufrufers.
///
/// Dieser Mini-Ersatz ist nötig, weil `os-mcp` rusqlite nicht als Abhängigkeit hat und der Plan aus
/// `os-analyze` (M1-N-Paket: `views::url_matches`) noch keine Einzel-URL-Prüfung mit `bind_named` anbietet.
/// Ein Parameter ohne Wert im Plan ist ein Fehler mit dem Namen des Parameters (statt eines stehen
/// gebliebenen `:name`, das erst die Datenbank als Syntaxfehler gemeldet hätte).
fn inline_params(sql: &str, params: &[(String, Value)]) -> Result<String, UnboundParameter> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            in_string = !in_string;
            out.push(c);
            i += 1;
            continue;
        }
        let starts_name = !in_string
            && c == ':'
            && chars
                .get(i + 1)
                .is_some_and(|n| n.is_ascii_alphabetic() || *n == '_');
        if !starts_name {
            out.push(c);
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
            j += 1;
        }
        let name: String = chars[i..j].iter().collect();
        match params.iter().find(|(n, _)| *n == name) {
            Some((_, value)) => out.push_str(&literal(value)),
            None => return Err(UnboundParameter(name)),
        }
        i = j;
    }
    Ok(out)
}

/// Ein benannter Parameter des Plans hat keinen Wert in der Crawl-Konfiguration.
#[derive(Debug, PartialEq, Eq)]
struct UnboundParameter(String);

impl std::fmt::Display for UnboundParameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Der Filterplan verwendet den Parameter {}, die Crawl-Konfiguration liefert keinen Wert dafür",
            self.0
        )
    }
}

fn literal(v: &Value) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => i32::from(*b).to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) if f.is_finite() => format!("{f:?}"),
        Value::Float(_) => "NULL".to_string(),
        Value::Text(s) => format!("'{}'", s.replace('\'', "''")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AGENT-012
    #[test]
    fn named_parameters_are_inlined_outside_of_strings_only() {
        let params = vec![
            (":title_max".to_string(), Value::Int(60)),
            (":title_max_px".to_string(), Value::Int(561)),
            (":label".to_string(), Value::Text("it's".into())),
        ];
        let sql = "u.len > :title_max AND u.px > :title_max_px AND u.s = :label AND u.x LIKE 'a:title_max' AND u.y = 1";
        assert_eq!(
            inline_params(sql, &params).unwrap(),
            "u.len > 60 AND u.px > 561 AND u.s = 'it''s' AND u.x LIKE 'a:title_max' AND u.y = 1"
        );
    }

    // spec: AGENT-012
    #[test]
    fn a_parameter_without_a_value_is_named_in_the_error() {
        let params = vec![(":title_max".to_string(), Value::Int(60))];
        let err = inline_params("u.len > :title_max AND u.y = :unbekannt", &params).unwrap_err();
        assert_eq!(err, UnboundParameter(":unbekannt".to_string()));
        assert!(err.to_string().contains(":unbekannt"), "{err}");
    }

    // spec: AGENT-012
    #[test]
    fn sections_default_to_all_and_unknown_ones_name_the_valid_ones() {
        assert_eq!(parse_sections(&json!({})).unwrap().len(), SECTIONS.len());
        assert_eq!(
            parse_sections(&json!({"sections": ["outlinks", "overview", "Structured Data"]}))
                .unwrap(),
            ["overview", "outlinks", "structured_data"]
        );
        assert_eq!(
            parse_sections(&json!({"sections": "all"})).unwrap().len(),
            SECTIONS.len()
        );
        let ToolError::InvalidArguments(msg) =
            parse_sections(&json!({"sections": ["inlinks", "kopf"]})).unwrap_err()
        else {
            panic!("falscher Fehlertyp");
        };
        assert!(msg.contains("kopf") && msg.contains("hreflang"), "{msg}");
    }

    // spec: AGENT-012
    #[test]
    fn urls_without_a_path_also_match_with_the_slash() {
        assert_eq!(
            address_candidates("http://site.test#frag"),
            [
                "http://site.test#frag",
                "http://site.test",
                "http://site.test/"
            ]
        );
        assert_eq!(
            address_candidates("http://site.test/a?b=1"),
            ["http://site.test/a?b=1"]
        );
    }

    // spec: AGENT-012
    #[test]
    fn every_reason_has_its_own_text() {
        let reasons = [
            "not_found",
            "excluded",
            "not_included",
            "outside_scope",
            "store_only",
            "crawl_disabled",
            "nofollow",
            "depth_limit",
            "url_limit",
            "redirect_limit",
            "external_not_checked",
            "list_mode_depth",
            "queued",
            "robots_txt",
        ];
        let texts: std::collections::BTreeSet<&str> =
            reasons.iter().map(|r| reason_text(r)).collect();
        assert_eq!(texts.len(), reasons.len());
    }

    // spec: AGENT-012
    #[test]
    fn cells_follow_the_view_conventions() {
        assert_eq!(cell_json(ColumnKind::Seconds3, json!(1234)), json!(1.234));
        assert_eq!(cell_json(ColumnKind::Bool, json!(1)), json!(true));
        assert_eq!(
            cell_json(ColumnKind::Float { decimals: 2 }, json!(1.2345)),
            json!(1.23)
        );
        assert_eq!(cell_json(ColumnKind::Text, json!(7)), json!("7"));
        assert_eq!(cell_json(ColumnKind::Int, json!(7.0)), json!(7));
        assert_eq!(cell_json(ColumnKind::Text, Json::Null), Json::Null);
    }
}
