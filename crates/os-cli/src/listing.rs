//! `--list-crawls` (OUT-185): Tabelle oder JSON-Array aller Crawls der Crawl-Verwaltung.

use std::path::Path;

use os_model::{Clock, SystemClock, Timestamp};
use os_store::registry::{self, CrawlListing};
use serde_json::json;

use crate::exit::CliError;

/// Eine Zeile der Liste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Crawl-ID.
    pub id: String,
    /// Projekt.
    pub project: String,
    /// Name.
    pub name: String,
    /// Modus (`spider`/`list`).
    pub mode: String,
    /// Start-URL (leer im Listenmodus).
    pub start_url: String,
    /// Startzeit (Exportformat mit lokalem Offset; leer, wenn nie gestartet).
    pub started: String,
    /// Anzahl der URLs.
    pub urls: u64,
    /// Zustand (`Running`, `Paused`, `Completed`, `Incomplete`, `Failed`).
    pub status: String,
    /// Gesperrt gegen Löschen, direkt oder über das Projekt.
    pub locked: bool,
}

fn row(listing: &CrawlListing, now_ms: i64) -> Row {
    let meta = &listing.meta;
    let state = listing.effective_state(Timestamp::from_unix_millis(now_ms));
    Row {
        id: meta.crawl_id.to_string(),
        project: meta.project.clone(),
        name: meta.name.clone(),
        mode: meta.mode.as_str().to_string(),
        start_url: meta.start_url.clone().unwrap_or_default(),
        started: meta.started_at.map(|t| t.to_export()).unwrap_or_default(),
        urls: listing.url_count,
        status: state.as_str().to_string(),
        locked: listing.locked,
    }
}

/// Verwaltungsprüfung bei der Initialisierung von `--load-crawl --resume` (CRAWL-128, K2-34).
/// Nicht fortsetzbare Zustände melden Exit 1 samt Zustand, bevor die Engine gestartet wird; ein Crawl mit Endgrund
/// `stopped` meldet „gestoppt – endgültig beendet“, unabhängig von seiner Warteschlange (AGENT-020).
pub fn validate_resume(path: &Path) -> Result<(), CliError> {
    let listing = registry::read_listing(path)
        .map_err(|e| CliError::crawl(format!("Crawl-Datei nicht lesbar: {e}")))?;
    if listing.meta.end_reason == Some(os_model::EndReason::Stopped) {
        return Err(CliError::internal(format!(
            "Crawl {} kann nicht fortgesetzt werden: {}",
            listing.meta.crawl_id,
            os_engine::lifecycle::STOPPED_IS_FINAL
        )));
    }
    let state = listing.effective_state(SystemClock.now());
    if matches!(
        state,
        os_model::CrawlState::Completed | os_model::CrawlState::Failed
    ) && !listing.can_resume(SystemClock.now())
    {
        return Err(CliError::internal(format!(
            "Crawl im Zustand {} kann nicht fortgesetzt werden",
            state.as_str()
        )));
    }
    Ok(())
}

/// Zeilen und nicht lesbare Dateien (Pfad, Ursache).
pub type Collected = (Vec<Row>, Vec<(String, String)>);

/// Liest die Crawl-Verwaltung; `project` beschränkt auf ein Projekt (genaue Schreibweise).
pub fn collect(dir: &Path, project: Option<&str>) -> Result<Collected, CliError> {
    let report = registry::list_crawls_report(dir)
        .map_err(|e| CliError::crawl(format!("Crawl-Ordner nicht lesbar: {e}")))?;
    let now_ms = SystemClock.now().unix_millis();
    let rows = report
        .crawls
        .iter()
        .map(|l| row(l, now_ms))
        .filter(|r| project.is_none_or(|p| r.project == p))
        .collect();
    let skipped = report
        .skipped
        .iter()
        .map(|(path, e)| (path.display().to_string(), e.to_string()))
        .collect();
    Ok((rows, skipped))
}

/// Tabelle mit Kopfzeile, Spalten durch Leerzeichen ausgerichtet.
pub fn table(rows: &[Row]) -> String {
    let header = [
        "ID",
        "Project",
        "Name",
        "Mode",
        "Start URL",
        "Started",
        "URLs",
        "Status",
        "Locked",
    ];
    let mut lines: Vec<Vec<String>> = vec![header.iter().map(|h| h.to_string()).collect()];
    for r in rows {
        lines.push(vec![
            r.id.clone(),
            r.project.clone(),
            r.name.clone(),
            r.mode.clone(),
            r.start_url.clone(),
            r.started.clone(),
            r.urls.to_string(),
            r.status.clone(),
            if r.locked { "yes" } else { "no" }.to_string(),
        ]);
    }
    let mut widths = vec![0usize; header.len()];
    for line in &lines {
        for (w, cell) in widths.iter_mut().zip(line) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let mut out = String::new();
    for line in &lines {
        let mut text = String::new();
        for (i, cell) in line.iter().enumerate() {
            if i > 0 {
                text.push_str("  ");
            }
            text.push_str(cell);
            for _ in cell.chars().count()..widths[i] {
                text.push(' ');
            }
        }
        out.push_str(text.trim_end());
        out.push('\n');
    }
    out
}

/// JSON-Array (Schlüssel klein, `start_url` mit Unterstrich).
pub fn json(rows: &[Row]) -> String {
    let items: Vec<_> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "project": r.project,
                "name": r.name,
                "mode": r.mode,
                "start_url": r.start_url,
                "started": r.started,
                "urls": r.urls,
                "status": r.status,
                "locked": r.locked,
            })
        })
        .collect();
    let mut text = serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_string());
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Row> {
        vec![
            Row {
                id: "0199aaaa-0000-7000-8000-000000000001".into(),
                project: "Default".into(),
                name: "site.test".into(),
                mode: "spider".into(),
                start_url: "http://site.test/".into(),
                started: "2026-10-14T10:00:00+02:00".into(),
                urls: 12,
                status: "Completed".into(),
                locked: false,
            },
            Row {
                id: "0199aaaa-0000-7000-8000-000000000002".into(),
                project: "Kunde A".into(),
                name: "Liste".into(),
                mode: "list".into(),
                start_url: String::new(),
                started: String::new(),
                urls: 3,
                status: "Paused".into(),
                locked: false,
            },
        ]
    }

    // spec: OUT-185
    #[test]
    fn the_table_has_the_documented_columns() {
        let text = table(&sample());
        let mut lines = text.lines();
        let header = lines.next().unwrap();
        assert!(header.starts_with("ID "));
        for col in [
            "Project",
            "Name",
            "Mode",
            "Start URL",
            "Started",
            "URLs",
            "Status",
            "Locked",
        ] {
            assert!(header.contains(col), "{col} fehlt in {header}");
        }
        assert_eq!(lines.count(), 2);
    }

    // spec: OUT-185
    #[test]
    fn json_is_an_array_of_objects() {
        let v: serde_json::Value = serde_json::from_str(&json(&sample())).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[1]["project"], "Kunde A");
        assert_eq!(arr[0]["urls"], 12);
        assert_eq!(arr[0]["locked"], false);
    }
}
