//! Persistenz der Optimierungsvorschläge `ai_suggestions` (M4-V01; 08 AI-070 … AI-076).
//!
//! Zeilenaufbau im unveränderten Schema `0006_ai.sql` (eine Zeile je (URL, Element, Kandidat)):
//!
//! * `candidate = 0` – der **Elementeintrag** einer URL: `status` (`Valid`, `No valid suggestion`, `Pending` oder ein
//!   Status aus AI-020), `review_status` (`open`, `accepted`, `edited`, `rejected`), `accepted_value`, `model`,
//!   `request_id` (letzter Request). Felder ohne eigene Spalte stehen als JSON-Objekt in `violations_json`
//!   (Verstöße des sichtbaren Kandidaten, Kennzeichen „vorläufig“, Anzeigereihenfolge, Erzeugungszeitpunkt,
//!   Fokus-Begriff, Prüfwerte der Übernahme). Der Spaltenbedarf ist im Bericht M4-V01 gemeldet.
//! * `candidate = k ≥ 1` – der k-te Kandidat in der Reihenfolge der Antworten (alle Runden nacheinander, AI-070):
//!   `value`, `status` (`valid`/`invalid`), `violations_json` = `{"violations": […], "length": n, "pixel_width": n,
//!   "round": r}`, `model`, `request_id`.
//! * Element `focus` mit `candidate = 0` – importierter Fokus-Begriff je URL (CSV `Address,Keyword`, AI-070).
//!
//! [`commit`] schreibt Protokollzeilen (`ai_requests`), Cache-Einträge (`ai_cache`) und den Elementeintrag in **einer**
//! Transaktion (AI-020, Nachtrag 26.11.2026: Verarbeitung einer Antwort ist atomar).
pub use rusqlite::Connection;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::ai::cache::{self, CacheRow};
use crate::ai::requests::{insert_with_details, RequestDetails, RequestRecord};
use crate::StoreError;

pub const TABLE: &str = "ai_suggestions";

/// Elementkennungen in `ai_suggestions.element`.
pub const ELEMENT_TITLE: &str = "title";
pub const ELEMENT_META_DESCRIPTION: &str = "meta_description";
/// Reserviert für M4-V02 (AI-074, AI-073).
pub const ELEMENT_H1: &str = "h1";
pub const ELEMENT_ALT: &str = "alt";
/// Pseudo-Element für importierte Fokus-Begriffe (kein Vorschlag).
pub const ELEMENT_FOCUS: &str = "focus";

/// Statuswerte des Elementeintrags außer denen aus AI-020.
pub const STATUS_VALID: &str = "Valid";
pub const STATUS_NO_VALID: &str = "No valid suggestion";
pub const STATUS_PENDING: &str = "Pending";

/// `Review Status` (AI-076).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewStatus {
    #[default]
    Open,
    Accepted,
    Edited,
    Rejected,
}

impl ReviewStatus {
    /// Wert in der Spalte `review_status`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Accepted => "accepted",
            Self::Edited => "edited",
            Self::Rejected => "rejected",
        }
    }

    /// Kanonischer Anzeigename (`Open`, `Accepted`, `Edited`, `Rejected`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Accepted => "Accepted",
            Self::Edited => "Edited",
            Self::Rejected => "Rejected",
        }
    }

    /// Liest die Spalte; der Schema-Standard `unreviewed` gilt als `Open`.
    pub fn parse(text: &str) -> Self {
        match text {
            "accepted" => Self::Accepted,
            "edited" => Self::Edited,
            "rejected" => Self::Rejected,
            _ => Self::Open,
        }
    }
}

/// Herkunft eines übernommenen Werts (Export-Spalte `Origin`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Ai,
    Edited,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::Edited => "edited",
        }
    }
}

/// Ein gespeicherter Kandidat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCandidate {
    /// Position in Antwortreihenfolge über alle Runden (1 …).
    pub index: u32,
    pub value: String,
    /// Zeichenzahl nach Whitespace-Normalisierung.
    pub length: u32,
    /// Pixelbreite nach UI-038; `None` für Elemente ohne Pixelregel.
    pub pixel_width: Option<u32>,
    pub valid: bool,
    pub violations: Vec<String>,
    /// Runde (1 = erster Request, 2 … = Reparaturrunden).
    pub round: u32,
}

/// Übernommener bzw. bearbeiteter Wert mit seinen Prüfwerten (AI-076).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedValue {
    pub value: String,
    pub length: u32,
    pub pixel_width: Option<u32>,
    /// Verstöße beim Speichern (nur bei `Edited` möglich; keine stille Korrektur).
    pub violations: Vec<String>,
    pub origin: Origin,
    /// Übernommener Kandidat (bei `Accepted`).
    pub candidate: Option<u32>,
    /// ISO-8601 (UTC).
    pub at: String,
}

/// Der Elementeintrag einer URL mit allen Kandidaten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementRecord {
    pub url_id: i64,
    pub element: String,
    /// `Valid`, `No valid suggestion`, `Pending` oder ein Status aus AI-020.
    pub status: String,
    /// Status und Kandidat 1 sind vorläufig, bis der Nachlauf (AI-071, Stufe 2) abgeschlossen ist.
    pub provisional: bool,
    /// Verstöße des sichtbaren Kandidaten (Spalte `… Violations`).
    pub violations: Vec<String>,
    /// Anzeigereihenfolge (Kandidat 1 = erster Eintrag); Werte sind [`StoredCandidate::index`].
    pub order: Vec<u32>,
    /// Gewählter gültiger Kandidat (`None` ohne gültigen).
    pub selected: Option<u32>,
    pub candidates: Vec<StoredCandidate>,
    pub model: Option<String>,
    pub request_id: Option<i64>,
    pub generated_at: Option<String>,
    pub focus: Option<String>,
    /// Gesendete Runden (Erstrequest + Reparaturrunden).
    pub rounds: u32,
    pub run_id: Option<i64>,
    pub review: ReviewStatus,
    pub accepted: Option<AcceptedValue>,
}

impl ElementRecord {
    /// Leerer Eintrag mit Status `Pending` (offen, AI-020).
    pub fn pending(url_id: i64, element: &str) -> Self {
        Self {
            url_id,
            element: element.to_string(),
            status: STATUS_PENDING.to_string(),
            provisional: false,
            violations: Vec::new(),
            order: Vec::new(),
            selected: None,
            candidates: Vec::new(),
            model: None,
            request_id: None,
            generated_at: None,
            focus: None,
            rounds: 0,
            run_id: None,
            review: ReviewStatus::Open,
            accepted: None,
        }
    }

    /// Kandidaten in Anzeigereihenfolge (`Suggestion 1` zuerst).
    pub fn displayed(&self) -> Vec<&StoredCandidate> {
        let mut out: Vec<&StoredCandidate> = self
            .order
            .iter()
            .filter_map(|k| self.candidates.iter().find(|c| c.index == *k))
            .collect();
        for c in &self.candidates {
            if !self.order.contains(&c.index) {
                out.push(c);
            }
        }
        out
    }

    /// Gewählter Kandidat.
    pub fn selected_candidate(&self) -> Option<&StoredCandidate> {
        self.selected
            .and_then(|k| self.candidates.iter().find(|c| c.index == k))
    }
}

fn accepted_json(a: &AcceptedValue) -> Value {
    json!({
        "length": a.length,
        "pixel_width": a.pixel_width,
        "violations": a.violations,
        "origin": a.origin.as_str(),
        "candidate": a.candidate,
        "at": a.at,
    })
}

fn record_json(r: &ElementRecord) -> Value {
    json!({
        "violations": r.violations,
        "provisional": r.provisional,
        "order": r.order,
        "selected": r.selected,
        "generated_at": r.generated_at,
        "focus": r.focus,
        "rounds": r.rounds,
        "run_id": r.run_id,
        "accepted": r.accepted.as_ref().map(accepted_json),
    })
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn u32_of(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok())
}

fn parse_json(text: &str) -> Value {
    match serde_json::from_str::<Value>(text) {
        // Ältere bzw. fremde Zeilen mit reiner Verstoßliste.
        Ok(Value::Array(list)) => json!({ "violations": list }),
        Ok(v) => v,
        Err(_) => json!({}),
    }
}

/// Meldet geänderte URLs an den Änderungsfeed der Tabellen (wie die SERP-Bearbeitung): `urls.change_seq` der URLs
/// und `crawl_meta.change_seq` steigen, damit offene Tabellen die Spalten neu lesen.
pub fn mark_changed(conn: &Connection, url_ids: &[i64]) -> Result<(), StoreError> {
    if url_ids.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO crawl_meta(key, value) VALUES('change_seq', '1') \
         ON CONFLICT(key) DO UPDATE SET value = CAST(value AS INTEGER) + 1",
        [],
    )?;
    let mut stmt = conn.prepare_cached(
        "UPDATE urls SET change_seq = (SELECT CAST(value AS INTEGER) FROM crawl_meta WHERE key = 'change_seq') \
         WHERE id = ?1",
    )?;
    for id in url_ids {
        stmt.execute([id])?;
    }
    Ok(())
}

/// Löscht und schreibt alle Zeilen eines (URL, Element).
pub fn save(conn: &Connection, r: &ElementRecord) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM ai_suggestions WHERE url_id = ?1 AND element = ?2",
        params![r.url_id, r.element],
    )?;
    let mut insert = conn.prepare_cached(
        "INSERT INTO ai_suggestions(url_id, element, candidate, value, status, violations_json, review_status, \
         accepted_value, model, request_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    insert.execute(params![
        r.url_id,
        r.element,
        0,
        Option::<String>::None,
        r.status,
        record_json(r).to_string(),
        r.review.as_str(),
        r.accepted.as_ref().map(|a| a.value.clone()),
        r.model,
        r.request_id,
    ])?;
    for c in &r.candidates {
        insert.execute(params![
            r.url_id,
            r.element,
            c.index,
            c.value,
            if c.valid { "valid" } else { "invalid" },
            json!({
                "violations": c.violations,
                "length": c.length,
                "pixel_width": c.pixel_width,
                "round": c.round,
            })
            .to_string(),
            "open",
            Option::<String>::None,
            r.model,
            r.request_id,
        ])?;
    }
    Ok(())
}

type RawRow = (
    i64,
    String,
    i64,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
);

fn assemble(rows: Vec<RawRow>) -> Vec<ElementRecord> {
    let mut out: Vec<ElementRecord> = Vec::new();
    for (url_id, element, candidate, value, status, json_text, review, accepted, model, request) in
        rows
    {
        let meta = parse_json(&json_text);
        if candidate == 0 {
            let accepted = accepted.map(|value| {
                let a = &meta["accepted"];
                AcceptedValue {
                    value,
                    length: u32_of(&a["length"]).unwrap_or(0),
                    pixel_width: u32_of(&a["pixel_width"]),
                    violations: strings(&a["violations"]),
                    origin: if a["origin"] == "edited" {
                        Origin::Edited
                    } else {
                        Origin::Ai
                    },
                    candidate: u32_of(&a["candidate"]),
                    at: a["at"].as_str().unwrap_or("").to_string(),
                }
            });
            out.push(ElementRecord {
                url_id,
                element,
                status,
                provisional: meta["provisional"].as_bool().unwrap_or(false),
                violations: strings(&meta["violations"]),
                order: meta["order"]
                    .as_array()
                    .map(|a| a.iter().filter_map(u32_of).collect())
                    .unwrap_or_default(),
                selected: u32_of(&meta["selected"]),
                candidates: Vec::new(),
                model,
                request_id: request,
                generated_at: meta["generated_at"].as_str().map(str::to_string),
                focus: meta["focus"].as_str().map(str::to_string),
                rounds: u32_of(&meta["rounds"]).unwrap_or(0),
                run_id: meta["run_id"].as_i64(),
                review: ReviewStatus::parse(&review),
                accepted,
            });
        } else if let Some(last) = out
            .last_mut()
            .filter(|r| r.url_id == url_id && r.element == element)
        {
            last.candidates.push(StoredCandidate {
                index: u32::try_from(candidate).unwrap_or(u32::MAX),
                value: value.unwrap_or_default(),
                length: u32_of(&meta["length"]).unwrap_or(0),
                pixel_width: u32_of(&meta["pixel_width"]),
                valid: status == "valid",
                violations: strings(&meta["violations"]),
                round: u32_of(&meta["round"]).unwrap_or(1),
            });
        }
    }
    out
}

const SELECT: &str = "SELECT url_id, element, candidate, value, status, violations_json, review_status, \
    accepted_value, model, request_id FROM ai_suggestions";

fn query(conn: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Result<Vec<RawRow>, StoreError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(p, |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Elementeintrag einer URL; `None`, wenn es keinen gibt.
pub fn load(conn: &Connection, url_id: i64, element: &str) -> Result<Option<ElementRecord>, StoreError> {
    let rows = query(
        conn,
        &format!("{SELECT} WHERE url_id = ?1 AND element = ?2 ORDER BY candidate"),
        &[&url_id, &element],
    )?;
    Ok(assemble(rows).into_iter().next())
}

/// Alle Elementeinträge eines Elements (ohne Fokus-Begriffe), aufsteigend nach URL-ID.
pub fn load_all(conn: &Connection, element: &str) -> Result<Vec<ElementRecord>, StoreError> {
    let rows = query(
        conn,
        &format!("{SELECT} WHERE element = ?1 ORDER BY url_id, candidate"),
        &[&element],
    )?;
    Ok(assemble(rows))
}

/// Eine Antwort bzw. ein Abschluss eines Eintrags: Protokollzeilen, Cache-Einträge und der Elementeintrag.
#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionCommit {
    pub requests: Vec<(RequestRecord, RequestDetails)>,
    pub cache: Vec<CacheRow>,
    /// `request_id` wird beim Schreiben auf die letzte neue Protokollzeile gesetzt (falls es eine gibt).
    pub record: ElementRecord,
}

/// Schreibt `c` atomar (eine Transaktion bzw. ein Savepoint innerhalb einer laufenden Transaktion). Liefert die
/// Kennungen der neuen Protokollzeilen in Reihenfolge (für `BudgetLedger::settle`).
pub fn commit(conn: &Connection, c: &SuggestionCommit) -> Result<Vec<i64>, StoreError> {
    conn.execute_batch("SAVEPOINT v01_commit")?;
    let result = (|| {
        let mut ids = Vec::with_capacity(c.requests.len());
        for (record, details) in &c.requests {
            ids.push(insert_with_details(conn, record, details)?);
        }
        for row in &c.cache {
            cache::put(conn, row)?;
        }
        let mut record = c.record.clone();
        if let Some(last) = ids.last() {
            record.request_id = Some(*last);
        }
        // Nutzerentscheidungen ändert nur die Prüfung (AI-076), nie ein Lauf.
        if let Some(old) = load(conn, record.url_id, &record.element)? {
            record.review = old.review;
            record.accepted = old.accepted;
        }
        save(conn, &record)?;
        mark_changed(conn, &[record.url_id])?;
        Ok(ids)
    })();
    match result {
        Ok(ids) => {
            conn.execute_batch("RELEASE v01_commit")?;
            Ok(ids)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO v01_commit; RELEASE v01_commit");
            Err(e)
        }
    }
}

/// Legt Einträge `Pending` für die URLs des Geltungsbereichs an (vor dem Senden, AI-020: offen). Vorhandene
/// Nutzerentscheidungen (Review, übernommener Wert) und Fokus-Begriffe bleiben erhalten.
pub fn mark_pending(
    conn: &Connection,
    element: &str,
    url_ids: &[i64],
    run_id: Option<i64>,
) -> Result<(), StoreError> {
    conn.execute_batch("SAVEPOINT v01_pending")?;
    let result = (|| {
        let focus = focus_map(conn)?;
        for &url_id in url_ids {
            let old = load(conn, url_id, element)?;
            let mut r = ElementRecord::pending(url_id, element);
            r.run_id = run_id;
            r.focus = focus.get(&url_id).cloned();
            if let Some(old) = old {
                r.review = old.review;
                r.accepted = old.accepted;
            }
            save(conn, &r)?;
        }
        mark_changed(conn, url_ids)
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("RELEASE v01_pending")?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO v01_pending; RELEASE v01_pending");
            Err(e)
        }
    }
}

/// Schreibt mehrere Einträge in einer Transaktion (Nachlauf, Neuprüfung, Massenaktionen).
pub fn save_all(conn: &Connection, records: &[ElementRecord]) -> Result<(), StoreError> {
    conn.execute_batch("SAVEPOINT v01_save_all")?;
    let result = records.iter().try_for_each(|r| save(conn, r)).and_then(|()| {
        let ids: Vec<i64> = records.iter().map(|r| r.url_id).collect();
        mark_changed(conn, &ids)
    });
    match result {
        Ok(()) => {
            conn.execute_batch("RELEASE v01_save_all")?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO v01_save_all; RELEASE v01_save_all");
            Err(e)
        }
    }
}

/// Fokus-Begriffe je URL-ID.
pub fn focus_map(conn: &Connection) -> Result<std::collections::BTreeMap<i64, String>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT url_id, value FROM ai_suggestions WHERE element = 'focus' AND candidate = 0 AND value IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Ersetzt alle Fokus-Begriffe (Import einer Datei `Address,Keyword`, AI-070) und überträgt sie auf vorhandene
/// Elementeinträge.
pub fn replace_focus(conn: &Connection, entries: &[(i64, String)]) -> Result<(), StoreError> {
    conn.execute_batch("SAVEPOINT v01_focus")?;
    let result = (|| {
        conn.execute("DELETE FROM ai_suggestions WHERE element = 'focus'", [])?;
        let mut insert = conn.prepare_cached(
            "INSERT INTO ai_suggestions(url_id, element, candidate, value, status) VALUES (?1, 'focus', 0, ?2, 'Imported')",
        )?;
        for (url_id, keyword) in entries {
            insert.execute(params![url_id, keyword])?;
        }
        let map: std::collections::BTreeMap<i64, String> = entries.iter().cloned().collect();
        for element in [ELEMENT_TITLE, ELEMENT_META_DESCRIPTION, ELEMENT_H1, ELEMENT_ALT] {
            let mut records = load_all(conn, element)?;
            for r in &mut records {
                r.focus = map.get(&r.url_id).cloned();
            }
            records.iter().try_for_each(|r| save(conn, r))?;
        }
        let ids: Vec<i64> = entries.iter().map(|(id, _)| *id).collect();
        mark_changed(conn, &ids)
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("RELEASE v01_focus")?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO v01_focus; RELEASE v01_focus");
            Err(e)
        }
    }
}

/// Neuer KI-Lauf in `ai_runs` (Protokollzeilen verweisen darauf).
pub fn begin_run(conn: &Connection, started_at: &str, config: &Value) -> Result<i64, StoreError> {
    conn.execute(
        "INSERT INTO ai_runs(started_at, config_json) VALUES (?1, ?2)",
        params![started_at, config.to_string()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Ende eines KI-Laufs.
pub fn end_run(conn: &Connection, run_id: i64, ended_at: &str) -> Result<(), StoreError> {
    conn.execute(
        "UPDATE ai_runs SET ended_at = ?2 WHERE id = ?1",
        params![run_id, ended_at],
    )?;
    Ok(())
}

/// Daten einer Seite für Vorschläge, Geltungsbereich und Dublettenprüfung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRow {
    pub url_id: i64,
    pub address: String,
    /// Interne HTML-URL mit Status 200 und indexierbar (Standard-Geltungsbereich S, 08 Konventionen).
    pub in_standard_scope: bool,
    pub indexable: bool,
    pub title: Option<String>,
    pub title_px: Option<u32>,
    pub meta_description: Option<String>,
    pub meta_description_px: Option<u32>,
    pub h1: Option<String>,
}

/// Alle internen HTML-Seiten, aufsteigend nach URL-ID.
pub fn pages(conn: &Connection) -> Result<Vec<PageRow>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, address, status_code, indexability, title_1, title_1_px, meta_desc_1, meta_desc_1_px, h1_1 \
         FROM urls WHERE is_internal = 1 AND content_class = 'HTML' ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let status: Option<i64> = r.get(2)?;
            let indexability: Option<String> = r.get(3)?;
            let indexable = indexability.as_deref() == Some("Indexable");
            let px = |v: Option<i64>| v.and_then(|n| u32::try_from(n).ok());
            Ok(PageRow {
                url_id: r.get(0)?,
                address: r.get(1)?,
                in_standard_scope: status == Some(200) && indexable,
                indexable,
                title: r.get(4)?,
                title_px: px(r.get(5)?),
                meta_description: r.get(6)?,
                meta_description_px: px(r.get(7)?),
                h1: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// URL-ID zu einer Adresse (exakt).
pub fn url_id_of(conn: &Connection, address: &str) -> Result<Option<i64>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT id FROM urls WHERE address = ?1 OR url = ?1 ORDER BY id LIMIT 1",
            [address],
            |r| r.get(0),
        )
        .optional()?)
}

/// Erfolgreiche Antworten einer Frage (`ai_answers`, z. B. `page-type`, `search-intent`, `title-matches-content`)
/// als (URL-ID, Antwort-JSON). Das Antwortformat legt M4-D01 fest; hier wird es nur durchgereicht.
pub fn answers(conn: &Connection, question_id: &str) -> Result<Vec<(i64, Value)>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT url_id, answer_json FROM ai_answers WHERE question_id = ?1 AND answer_json IS NOT NULL \
         AND status IN ('Success', 'Success (cached)') ORDER BY url_id",
    )?;
    let rows = stmt
        .query_map([question_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, text)| serde_json::from_str(&text).ok().map(|v| (id, v)))
        .collect())
}

/// Zeile der Exporte (AI-076) mit Adresse und aktuellem Wert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRecord {
    pub address: String,
    pub current_value: Option<String>,
    pub record: ElementRecord,
    /// Zeitstempel der letzten Protokollzeile (`Generated At`, falls im Eintrag nicht gespeichert).
    pub request_timestamp: Option<String>,
}

/// Alle Einträge eines Elements mit Adresse, aktuellem Wert und Zeitstempel, aufsteigend nach Adresse (Bytes).
pub fn export_records(conn: &Connection, element: &str) -> Result<Vec<ExportRecord>, StoreError> {
    let column = match element {
        ELEMENT_TITLE => "title_1",
        ELEMENT_META_DESCRIPTION => "meta_desc_1",
        ELEMENT_H1 => "h1_1",
        _ => "NULL",
    };
    let mut out = Vec::new();
    for record in load_all(conn, element)? {
        let (address, current): (String, Option<String>) = conn.query_row(
            &format!("SELECT address, {column} FROM urls WHERE id = ?1"),
            [record.url_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let request_timestamp = match record.request_id {
            Some(id) => conn
                .query_row("SELECT timestamp FROM ai_requests WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()?,
            None => None,
        };
        out.push(ExportRecord {
            address,
            current_value: current,
            record,
            request_timestamp,
        });
    }
    out.sort_by(|a, b| a.address.as_bytes().cmp(b.address.as_bytes()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{migrate, MIGRATIONS};

    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        db.pragma_update(None, "foreign_keys", true).unwrap();
        migrate(&mut db, 0, MIGRATIONS).unwrap();
        db.execute_batch(
            "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length,\
               content_class,status_code,indexability,title_1) VALUES
               (1,'http://ai.test:8080/a/','http://ai.test:8080/a/','http://ai.test:8080',1,'Done','Seed',1,23,'HTML',200,'Indexable','Alt A'),
               (2,'http://ai.test:8080/b/','http://ai.test:8080/b/','http://ai.test:8080',1,'Done','Seed',1,23,'HTML',200,'Non-Indexable','Alt B');
             INSERT INTO ai_runs(id, started_at, config_json) VALUES (1, '2026-11-26T08:00:00Z', '{}');",
        )
        .unwrap();
        db
    }

    fn record() -> ElementRecord {
        let mut r = ElementRecord::pending(1, ELEMENT_TITLE);
        r.status = STATUS_VALID.into();
        r.provisional = true;
        r.order = vec![2, 1];
        r.selected = Some(2);
        r.model = Some("gpt-6-luna".into());
        r.generated_at = Some("2026-11-26T08:00:01Z".into());
        r.rounds = 1;
        r.candidates = vec![
            StoredCandidate {
                index: 1,
                value: "X".into(),
                length: 1,
                pixel_width: Some(12),
                valid: false,
                violations: vec!["length<30".into(), "pixel_width<200".into()],
                round: 1,
            },
            StoredCandidate {
                index: 2,
                value: "Wanderschuhe für Damen günstig online bestellen".into(),
                length: 47,
                pixel_width: Some(420),
                valid: true,
                violations: Vec::new(),
                round: 1,
            },
        ];
        r
    }

    // spec: AI-070 [M4]
    // Vorschläge, Prüfstatus und Nutzerentscheidungen liegen in der Crawl-Datei und kommen unverändert zurück.
    #[test]
    fn element_record_roundtrips_with_candidates_review_and_flags() {
        let conn = db();
        let mut r = record();
        r.review = ReviewStatus::Edited;
        r.accepted = Some(AcceptedValue {
            value: "Eigener Title".into(),
            length: 13,
            pixel_width: Some(120),
            violations: vec!["length<30".into()],
            origin: Origin::Edited,
            candidate: None,
            at: "2026-11-26T09:00:00Z".into(),
        });
        save(&conn, &r).unwrap();
        let back = load(&conn, 1, ELEMENT_TITLE).unwrap().unwrap();
        assert_eq!(back, r);
        assert_eq!(
            back.displayed().iter().map(|c| c.index).collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(back.selected_candidate().unwrap().index, 2);
        assert_eq!(load_all(&conn, ELEMENT_TITLE).unwrap().len(), 1);
        assert!(load_all(&conn, ELEMENT_META_DESCRIPTION).unwrap().is_empty());
    }

    // spec: AI-020 [M4]
    // Nachtrag 26.11.2026: Protokollzeile, Cache-Eintrag und Ergebnis entstehen gemeinsam oder gar nicht.
    #[test]
    fn commit_is_atomic_and_sets_the_last_request_id() {
        let conn = db();
        let req = RequestRecord {
            run_id: 1,
            timestamp: "2026-11-26T08:00:01Z".into(),
            task: "suggest-title".into(),
            provider: "openai".into(),
            model: "gpt-6-luna".into(),
            endpoint: "responses".into(),
            url_id: Some(1),
            definition_version: Some("1.0.0".into()),
            reservation: os_model::budget::MicroUsd(10),
            redactions: 0,
            input_tokens: Some(100),
            cached_input_tokens: Some(0),
            cache_write_tokens: Some(0),
            output_tokens: Some(20),
            images: 0,
            token_cost: os_model::budget::MicroUsd(1),
            fixed_fees: os_model::budget::MicroUsd(0),
            regional_uplift: os_model::budget::MicroUsd(0),
            cost: os_model::budget::MicroUsd(1),
            price_source: crate::ai::requests::PriceSource::Table,
            batch_id: None,
            cache_hit: false,
            status: "Success".into(),
            http_status: Some(200),
            error_code: None,
            error_message: None,
            refusal_reason: None,
            duration_ms: 3,
            provider_request_id: None,
            retries: 0,
            over_reservation: false,
            confidence_mismatch: false,
            estimated_input_tokens: Some(90),
        };
        let ok = SuggestionCommit {
            requests: vec![(req.clone(), RequestDetails::default())],
            cache: Vec::new(),
            record: record(),
        };
        let ids = commit(&conn, &ok).unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(
            load(&conn, 1, ELEMENT_TITLE).unwrap().unwrap().request_id,
            Some(ids[0])
        );
        // Fehler mitten im Commit (unbekannte URL-ID verletzt den Fremdschlüssel): nichts bleibt zurück.
        let mut bad = ok.clone();
        bad.record.url_id = 99;
        assert!(commit(&conn, &bad).is_err());
        let requests: i64 = conn
            .query_row("SELECT count(*) FROM ai_requests", [], |r| r.get(0))
            .unwrap();
        assert_eq!(requests, 1);
        assert!(load(&conn, 99, ELEMENT_TITLE).unwrap().is_none());
    }

    // spec: AI-070 [M4]
    // Fokus-Begriffe (CSV `Address,Keyword`) bleiben je URL erhalten und überstehen das Markieren als `Pending`.
    #[test]
    fn focus_keywords_and_review_survive_mark_pending() {
        let conn = db();
        let mut r = record();
        r.review = ReviewStatus::Accepted;
        r.accepted = Some(AcceptedValue {
            value: "Wanderschuhe für Damen günstig online bestellen".into(),
            length: 47,
            pixel_width: Some(420),
            violations: Vec::new(),
            origin: Origin::Ai,
            candidate: Some(2),
            at: "2026-11-26T09:00:00Z".into(),
        });
        save(&conn, &r).unwrap();
        replace_focus(&conn, &[(1, "wanderschuhe damen".into())]).unwrap();
        assert_eq!(focus_map(&conn).unwrap()[&1], "wanderschuhe damen");
        mark_pending(&conn, ELEMENT_TITLE, &[1, 2], Some(1)).unwrap();
        let a = load(&conn, 1, ELEMENT_TITLE).unwrap().unwrap();
        assert_eq!(a.status, STATUS_PENDING);
        assert!(a.candidates.is_empty());
        assert_eq!(a.review, ReviewStatus::Accepted);
        assert_eq!(a.focus.as_deref(), Some("wanderschuhe damen"));
        let b = load(&conn, 2, ELEMENT_TITLE).unwrap().unwrap();
        assert_eq!(b.focus, None);
        assert_eq!(pages(&conn).unwrap().len(), 2);
        assert!(pages(&conn).unwrap()[0].in_standard_scope);
        assert!(!pages(&conn).unwrap()[1].in_standard_scope);
        assert_eq!(url_id_of(&conn, "http://ai.test:8080/b/").unwrap(), Some(2));
        let exported = export_records(&conn, ELEMENT_TITLE).unwrap();
        assert_eq!(exported[0].address, "http://ai.test:8080/a/");
        assert_eq!(exported[0].current_value.as_deref(), Some("Alt A"));
    }
}
