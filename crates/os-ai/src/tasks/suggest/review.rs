//! Prüfen, Übernehmen, Bearbeiten, Ablehnen (AI-076) und Neuprüfung ohne Request nach geänderten Grenzwerten
//! (AI-075). Alles hier ist lokal: kein Request, keine Kosten.
use super::dedupe::{post_pass, SiteIndex};
use super::validate::{Checked, Checker};
use os_store::ai::suggestions::{
    AcceptedValue, ElementRecord, Origin, ReviewStatus, STATUS_VALID,
};
use std::collections::BTreeMap;

/// Aktion der Prüfung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewAction {
    /// Übernimmt den Kandidaten `candidate` (Index in Antwortreihenfolge) bzw. ohne Angabe den gewählten Kandidaten 1.
    Accept { candidate: Option<u32> },
    /// Freier Text: wird live geprüft und auch mit Verstößen gespeichert, dann als `Edited` mit Verstoßliste.
    Edit { value: String },
    Reject,
    /// Zurück auf `Open`.
    Reset,
}

/// Abgelehnte Aktion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewError {
    /// Kein gültiger Kandidat (z. B. `No valid suggestion`); bearbeitet übernehmen bleibt möglich.
    NoValidCandidate,
    UnknownCandidate(u32),
}

impl std::fmt::Display for ReviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoValidCandidate => f.write_str("no valid candidate to accept"),
            Self::UnknownCandidate(k) => write!(f, "unknown candidate {k}"),
        }
    }
}

impl std::error::Error for ReviewError {}

/// Wendet eine Aktion an. `checker` prüft bearbeitete Texte live (AI-075 inkl. Pixelbreite); `now` ist der
/// Zeitstempel der Entscheidung (ISO-8601).
pub fn apply(
    record: &mut ElementRecord,
    action: &ReviewAction,
    checker: &Checker<'_>,
    language: Option<&str>,
    now: &str,
) -> Result<(), ReviewError> {
    match action {
        ReviewAction::Accept { candidate } => {
            let k = candidate
                .or(record.selected)
                .ok_or(ReviewError::NoValidCandidate)?;
            let c = record
                .candidates
                .iter()
                .find(|c| c.index == k)
                .ok_or(ReviewError::UnknownCandidate(k))?;
            if !c.valid {
                return Err(ReviewError::NoValidCandidate);
            }
            record.accepted = Some(AcceptedValue {
                value: c.value.clone(),
                length: c.length,
                pixel_width: c.pixel_width,
                violations: Vec::new(),
                origin: Origin::Ai,
                candidate: Some(k),
                at: now.to_string(),
            });
            record.review = ReviewStatus::Accepted;
        }
        ReviewAction::Edit { value } => {
            let checked = checker.check(record.url_id, language, value);
            record.accepted = Some(AcceptedValue {
                value: checked.value,
                length: checked.length,
                pixel_width: Some(checked.pixel_width),
                violations: checked.violations,
                origin: Origin::Edited,
                candidate: None,
                at: now.to_string(),
            });
            record.review = ReviewStatus::Edited;
        }
        ReviewAction::Reject => {
            record.accepted = None;
            record.review = ReviewStatus::Rejected;
        }
        ReviewAction::Reset => {
            record.accepted = None;
            record.review = ReviewStatus::Open;
        }
    }
    Ok(())
}

/// Ergebnis der Massenaktion „Kandidat 1 übernehmen“.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BulkResult {
    pub accepted: usize,
    /// URLs ohne gültigen Kandidaten (z. B. `No valid suggestion`); sie bleiben `Open` und werden gezählt gemeldet.
    pub skipped: usize,
    pub skipped_url_ids: Vec<i64>,
}

/// „Kandidat 1 übernehmen“ für viele URLs: nur gültige Kandidaten (Status `Valid`), alle anderen bleiben unverändert.
pub fn accept_first(records: &mut [ElementRecord], now: &str) -> BulkResult {
    let mut result = BulkResult::default();
    for r in records {
        let valid = r.status == STATUS_VALID
            && r.selected_candidate().is_some_and(|c| c.valid);
        if !valid {
            result.skipped += 1;
            result.skipped_url_ids.push(r.url_id);
            continue;
        }
        let c = r.selected_candidate().cloned().expect("checked above");
        r.accepted = Some(AcceptedValue {
            value: c.value,
            length: c.length,
            pixel_width: c.pixel_width,
            violations: Vec::new(),
            origin: Origin::Ai,
            candidate: Some(c.index),
            at: now.to_string(),
        });
        r.review = ReviewStatus::Accepted;
        result.accepted += 1;
    }
    result
}

/// Live-Prüfung eines bearbeiteten Texts (AI-076).
pub fn check_text(checker: &Checker<'_>, url_id: i64, language: Option<&str>, text: &str) -> Checked {
    checker.check(url_id, language, text)
}

/// Neuprüfung aller Einträge eines Elements mit den **aktuellen** Grenzwerten und Regeln, ohne Request (AI-075):
/// Länge, Pixelbreite, Dubletten, Format, Sprache und Stil jedes Kandidaten werden neu bestimmt, danach läuft der
/// Nachlauf der Dublettenprüfung. Übernommene bzw. bearbeitete Werte erhalten ihre aktuelle Verstoßliste
/// (`Edited` bleibt `Edited`). Einträge ohne Kandidaten (Fehler, `Pending`) bleiben unverändert.
pub fn revalidate(
    records: Vec<ElementRecord>,
    checker: &Checker<'_>,
    languages: &BTreeMap<i64, Option<String>>,
    index: &SiteIndex,
) -> Vec<ElementRecord> {
    let mut out = Vec::with_capacity(records.len());
    for mut r in records {
        let lang = languages.get(&r.url_id).cloned().flatten();
        for c in &mut r.candidates {
            let checked = checker.check(r.url_id, lang.as_deref(), &c.value);
            c.length = checked.length;
            c.pixel_width = Some(checked.pixel_width);
            c.valid = checked.valid();
            c.violations = checked.violations;
        }
        if let Some(a) = &mut r.accepted {
            let checked = checker.check(r.url_id, lang.as_deref(), &a.value);
            a.length = checked.length;
            a.pixel_width = Some(checked.pixel_width);
            if a.origin == Origin::Edited {
                a.violations = checked.violations;
            }
        }
        out.push(r);
    }
    post_pass(out, index, checker.rules.duplicates)
}
