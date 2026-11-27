//! XLSX-Schreiber (Zellarten nach `ColumnKind`, Überlauf auf Folgeblätter, OUT-003).
//!
//! * Zahlenspalten werden Zahlenzellen, Texte Textzellen, Zeitstempel Datumszellen (`yyyy-mm-dd hh:mm:ss`).
//!   [`ColumnKind::Hash`] und [`ColumnKind::Url`] sind immer Text (nie Hyperlink).
//! * Es wird nie eine Formel geschrieben: ein Wert `=SUM(A1)` steht als Text in der Zelle.
//! * Die erste Zeile ist fixiert und hat einen Autofilter über alle Spalten.
//! * Texte über 32 767 Zeichen werden gekürzt und enden auf `…[truncated]`; die Anzahl gekürzter Zellen
//!   steht als Warnung im Log (`tracing`) und in [`ExportFileInfo::warnings`].
//! * Ab [`WriterOptions::max_rows_per_sheet`] Datenzeilen (Standard 1 048 575) folgt ein Blatt
//!   `<Name> (2)` mit eigener Kopfzeile; auch das steht als Warnung in [`ExportFileInfo::warnings`].
//! * Blattnamen: höchstens 31 Zeichen, ohne `[]:*?/\`, innerhalb der Datei eindeutig ([`sheet_name`],
//!   [`unique_sheet_name`]).
//!
//! Speicher: Blätter entstehen mit `add_worksheet_with_constant_memory()` (Feature `constant_memory` von
//! `rust_xlsxwriter`). Zeilen werden dadurch beim Schreiben in eine temporäre Datei ausgelagert und müssen
//! aufsteigend geschrieben werden (Kopfzeile, dann Datenzeilen); Fixierung, Spaltenbreiten und Autofilter sind
//! Blatteigenschaften und werden erst beim Speichern ausgewertet. Eine RSS-Messung mit 1 Mio. Zeilen gehört
//! zum Benchmark (P28).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use os_analyze::ColumnKind;
use os_model::Value;
use rust_xlsxwriter::{DocProperties, ExcelDateTime, Format, Workbook, XlsxError};

use crate::csv::timestamp_export;
use crate::error::ExportError;
use crate::fs_paths;
use crate::types::{ExportFileInfo, TableWriter, WriterOptions};

/// Höchste Textlänge einer Zelle in Excel (UTF-16-Einheiten bzw. Zeichen).
pub const MAX_CELL_CHARS: usize = 32_767;
/// Ersatz für das Ende gekürzter Texte.
pub const TRUNCATION_MARKER: &str = "…[truncated]";
/// Höchste Länge eines Blattnamens.
pub const MAX_SHEET_NAME_LEN: usize = 31;
/// Excel-Obergrenze für Spalten.
const MAX_COLUMNS: usize = 16_384;
/// Excel-Obergrenze für Zeilen (inklusive Kopfzeile) abzüglich Kopfzeile.
const MAX_DATA_ROWS: u32 = 1_048_575;
/// Ganze Zahlen über 2^53 verlieren als `f64` Stellen und werden deshalb als Text geschrieben.
const MAX_EXACT_INT: i64 = 9_007_199_254_740_992;

/// Länge in UTF-16-Einheiten (so zählt Excel Zeichen).
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Schneidet `s` auf höchstens `max` UTF-16-Einheiten ab (an Zeichengrenzen).
fn take_utf16(s: &str, max: usize) -> &str {
    let mut used = 0;
    for (i, c) in s.char_indices() {
        used += c.len_utf16();
        if used > max {
            return &s[..i];
        }
    }
    s
}

/// Bildet einen gültigen Blattnamen: `[]:*?/\` werden zu Leerzeichen (mehrfache Leerzeichen zu einem),
/// Steuerzeichen entfallen, Apostrophe am Anfang/Ende entfallen, Länge ≤ 31; leer wird `Sheet`.
pub fn sheet_name(name: &str) -> String {
    let mut cleaned = String::with_capacity(name.len());
    for c in name.chars() {
        let c = if matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\') || c.is_whitespace() {
            ' '
        } else if c.is_control() {
            continue;
        } else {
            c
        };
        if c == ' ' && cleaned.ends_with(' ') {
            continue;
        }
        cleaned.push(c);
    }
    let mut s = cleaned.trim().trim_matches('\'').trim().to_string();
    s = take_utf16(&s, MAX_SHEET_NAME_LEN).to_string();
    s = s.trim().trim_matches('\'').trim().to_string();
    if s.is_empty() {
        "Sheet".to_string()
    } else {
        s
    }
}

/// Blattname, der in `existing` (ohne Beachtung der Groß-/Kleinschreibung, so vergleicht Excel) nicht
/// vorkommt: bei Kollision folgt ` (2)`, ` (3)` …, der Name wird dafür so gekürzt, dass er ≤ 31 Zeichen bleibt.
pub fn unique_sheet_name(name: &str, existing: &[String]) -> String {
    let base = sheet_name(name);
    let taken = |cand: &str| {
        let lc = cand.to_lowercase();
        existing.iter().any(|e| e.to_lowercase() == lc)
    };
    if !taken(&base) {
        return base;
    }
    let mut n = 2u32;
    loop {
        let suffix = format!(" ({n})");
        let keep = take_utf16(&base, MAX_SHEET_NAME_LEN - utf16_len(&suffix)).trim_end();
        let cand = format!("{keep}{suffix}");
        if !taken(&cand) {
            return cand;
        }
        n += 1;
    }
}

/// Kürzt einen Text auf [`MAX_CELL_CHARS`]; gekürzte Texte enden auf [`TRUNCATION_MARKER`].
/// Liefert den Text und `true`, wenn gekürzt wurde.
fn clip(s: &str) -> (std::borrow::Cow<'_, str>, bool) {
    if utf16_len(s) <= MAX_CELL_CHARS {
        return (std::borrow::Cow::Borrowed(s), false);
    }
    let keep = take_utf16(s, MAX_CELL_CHARS - utf16_len(TRUNCATION_MARKER));
    (
        std::borrow::Cow::Owned(format!("{keep}{TRUNCATION_MARKER}")),
        true,
    )
}

fn xlsx_err(path: &Path, e: XlsxError) -> ExportError {
    match e {
        XlsxError::IoError(source) => ExportError::Io {
            path: path.to_path_buf(),
            source,
        },
        other => ExportError::Write(other.to_string()),
    }
}

/// Zellformate, einmal angelegt.
struct Formats {
    header: Format,
    int: Format,
    datetime: Format,
    seconds3: Format,
    float: HashMap<u8, Format>,
}

impl Formats {
    fn new() -> Self {
        Formats {
            header: Format::new().set_bold(),
            int: Format::new().set_num_format("0"),
            datetime: Format::new().set_num_format("yyyy-mm-dd hh:mm:ss"),
            seconds3: Format::new().set_num_format("0.000"),
            float: HashMap::new(),
        }
    }

    fn float(&mut self, decimals: u8) -> &Format {
        self.float.entry(decimals).or_insert_with(|| {
            let fmt = if decimals == 0 {
                "0".to_string()
            } else {
                format!("0.{}", "0".repeat(usize::from(decimals)))
            };
            Format::new().set_num_format(fmt)
        })
    }
}

/// Eine Zelle in Zielform.
enum Cell {
    Skip,
    Text(String),
    Bool(bool),
    Number(f64, NumFmt),
    DateTime(String),
}

#[derive(Clone, Copy)]
enum NumFmt {
    Int,
    Fixed(u8),
    Seconds3,
}

fn round_to(f: f64, decimals: u8) -> f64 {
    let p = 10f64.powi(i32::from(decimals));
    (f * p).round() / p
}

/// Bestimmt die Zelle zu (Art, Wert) nach den Regeln aus OUT-003.
fn to_cell(kind: ColumnKind, v: &Value) -> Cell {
    let text_like = matches!(kind, ColumnKind::Text | ColumnKind::Url | ColumnKind::Hash);
    match v {
        Value::Null => Cell::Skip,
        Value::Bool(b) => Cell::Bool(*b),
        Value::Text(s) => {
            if kind == ColumnKind::Timestamp {
                if let Some(ts) = timestamp_export(v, None) {
                    return Cell::DateTime(ts);
                }
            }
            Cell::Text(s.clone())
        }
        Value::Int(i) => match kind {
            _ if text_like => Cell::Text(i.to_string()),
            ColumnKind::Bool if *i == 0 || *i == 1 => Cell::Bool(*i == 1),
            ColumnKind::Timestamp => match timestamp_export(v, None) {
                Some(ts) => Cell::DateTime(ts),
                None => Cell::Text(i.to_string()),
            },
            _ if i.abs() > MAX_EXACT_INT => Cell::Text(i.to_string()),
            ColumnKind::Float { decimals } => Cell::Number(*i as f64, NumFmt::Fixed(decimals)),
            ColumnKind::Seconds3 => Cell::Number(round_to(*i as f64, 3), NumFmt::Seconds3),
            _ => Cell::Number(*i as f64, NumFmt::Int),
        },
        Value::Float(f) => {
            if !f.is_finite() {
                return Cell::Skip;
            }
            match kind {
                _ if text_like => Cell::Text(format!("{f}")),
                ColumnKind::Float { decimals } => {
                    Cell::Number(round_to(*f, decimals), NumFmt::Fixed(decimals))
                }
                ColumnKind::Seconds3 => Cell::Number(round_to(*f, 3), NumFmt::Seconds3),
                ColumnKind::Timestamp => match timestamp_export(&Value::Int(*f as i64), None) {
                    Some(ts) => Cell::DateTime(ts),
                    None => Cell::Skip,
                },
                ColumnKind::Int => Cell::Number(*f, NumFmt::Int),
                _ => Cell::Number(*f, NumFmt::Int),
            }
        }
    }
}

/// Zeilenweiser XLSX-Schreiber.
pub struct XlsxWriter {
    header_labels: std::collections::BTreeMap<String, String>,
    path: PathBuf,
    workbook: Workbook,
    base_name: String,
    sheet_names: Vec<String>,
    /// Anzahl Datenzeilen je Blatt.
    sheet_rows: Vec<u32>,
    sheet_widths: Vec<usize>,
    tables: usize,
    header: Option<Vec<String>>,
    max_rows: u32,
    total_rows: u64,
    truncated: u64,
    formats: Formats,
    cancel: Option<os_model::CancelToken>,
}

impl XlsxWriter {
    /// Legt den Schreiber an. Die Datei entsteht erst in [`TableWriter::finish`]; der Elternordner wird
    /// sofort angelegt, damit Pfadfehler früh auffallen.
    pub fn create(path: &Path, sheet: &str, opts: &WriterOptions) -> Result<Self, ExportError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs_paths::create_dir_all(parent).map_err(|source| ExportError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let name = sheet_name(sheet);
        let mut w = XlsxWriter {
            header_labels: opts.header_labels.clone(),
            path: path.to_path_buf(),
            workbook: deterministic_workbook(),
            base_name: name.clone(),
            sheet_names: Vec::new(),
            sheet_rows: Vec::new(),
            sheet_widths: Vec::new(),
            tables: 1,
            header: None,
            max_rows: opts.max_rows_per_sheet.clamp(1, MAX_DATA_ROWS),
            total_rows: 0,
            truncated: 0,
            formats: Formats::new(),
            cancel: None,
        };
        w.add_sheet(name)?;
        Ok(w)
    }

    /// Keeps final ZIP serialization interruptible, including large workbooks.
    pub fn set_cancel(&mut self, cancel: os_model::CancelToken) {
        self.cancel = Some(cancel);
    }

    /// Ob bereits mindestens eine Tabellenkopfzeile geschrieben wurde (auch bei übersprungenen leeren Tabellen).
    pub fn has_tables(&self) -> bool {
        self.header.is_some() || self.tables > 1
    }

    /// Namen der bisher angelegten Blätter.
    pub fn sheet_names(&self) -> &[String] {
        &self.sheet_names
    }

    fn add_sheet(&mut self, name: String) -> Result<(), ExportError> {
        let ws = self
            .workbook
            .add_worksheet_with_constant_memory()
            .map_err(|e| xlsx_err(&self.path, e))?;
        ws.set_name(&name).map_err(|e| xlsx_err(&self.path, e))?;
        self.sheet_names.push(name);
        self.sheet_rows.push(0);
        self.sheet_widths.push(0);
        Ok(())
    }

    fn current(&self) -> usize {
        self.sheet_names.len() - 1
    }

    fn write_header_row(&mut self) -> Result<(), ExportError> {
        let idx = self.current();
        let Some(columns) = self.header.clone() else {
            return Ok(());
        };
        self.sheet_widths[idx] = columns.len();
        let mut clipped = 0u64;
        let ws = self
            .workbook
            .worksheet_from_index(idx)
            .map_err(|e| xlsx_err(&self.path, e))?;
        for (c, name) in columns.iter().enumerate() {
            let (text, was_clipped) = clip(name);
            clipped += u64::from(was_clipped);
            ws.write_string_with_format(0, c as u16, text.as_ref(), &self.formats.header)
                .map_err(|e| xlsx_err(&self.path, e))?;
            let width = (name.chars().count() as f64 + 3.0).clamp(10.0, 50.0);
            ws.set_column_width(c as u16, width)
                .map_err(|e| xlsx_err(&self.path, e))?;
        }
        ws.set_freeze_panes(1, 0)
            .map_err(|e| xlsx_err(&self.path, e))?;
        self.truncated += clipped;
        Ok(())
    }

    fn roll_over(&mut self) -> Result<(), ExportError> {
        let name = unique_sheet_name(&self.base_name, &self.sheet_names);
        // `<Name> (2)`: der Grundname ist belegt, `unique_sheet_name` hängt die nächste freie Nummer an.
        tracing::warn!(
            file = %self.path.display(),
            sheet = %name,
            rows_per_sheet = self.max_rows,
            "XLSX-Blatt voll, Fortsetzung auf neuem Blatt"
        );
        self.add_sheet(name)?;
        self.write_header_row()
    }
}

impl TableWriter for XlsxWriter {
    fn format(&self) -> Option<crate::types::Format> {
        Some(crate::types::Format::Xlsx)
    }
    fn header(&mut self, columns: &[String]) -> Result<(), ExportError> {
        if self.header.is_some() {
            return Err(ExportError::Write(
                "Kopfzeile wurde bereits geschrieben".to_string(),
            ));
        }
        if columns.len() > MAX_COLUMNS {
            return Err(ExportError::Write(format!(
                "{} Spalten überschreiten die Excel-Grenze von {MAX_COLUMNS}",
                columns.len()
            )));
        }
        self.header = Some(crate::csv::header_labels(columns, &self.header_labels));
        self.write_header_row()
    }

    fn row(&mut self, kinds: &[ColumnKind], values: &[Value]) -> Result<(), ExportError> {
        if let Some(h) = &self.header {
            if h.len() != values.len() {
                return Err(ExportError::Write(format!(
                    "Zeile hat {} Werte, die Kopfzeile {} Spalten",
                    values.len(),
                    h.len()
                )));
            }
        } else if values.len() > MAX_COLUMNS {
            return Err(ExportError::Write(format!(
                "{} Spalten überschreiten die Excel-Grenze von {MAX_COLUMNS}",
                values.len()
            )));
        }
        let idx = self.current();
        if self.sheet_rows[idx] >= self.max_rows {
            self.roll_over()?;
        }
        let idx = self.current();
        let first_data_row = u32::from(self.header.is_some());
        let r = first_data_row + self.sheet_rows[idx];
        let mut clipped = 0u64;
        {
            let ws = self
                .workbook
                .worksheet_from_index(idx)
                .map_err(|e| xlsx_err(&self.path, e))?;
            for (c, v) in values.iter().enumerate() {
                let kind = kinds.get(c).copied().unwrap_or(ColumnKind::Text);
                let col = c as u16;
                match to_cell(kind, v) {
                    Cell::Skip => {}
                    Cell::Text(s) => {
                        let (text, was_clipped) = clip(&s);
                        clipped += u64::from(was_clipped);
                        ws.write_string(r, col, text.as_ref())
                            .map_err(|e| xlsx_err(&self.path, e))?;
                    }
                    Cell::Bool(b) => {
                        ws.write_boolean(r, col, b)
                            .map_err(|e| xlsx_err(&self.path, e))?;
                    }
                    Cell::Number(n, fmt) => {
                        let format = match fmt {
                            NumFmt::Int => &self.formats.int,
                            NumFmt::Seconds3 => &self.formats.seconds3,
                            NumFmt::Fixed(d) => self.formats.float(d),
                        };
                        ws.write_number_with_format(r, col, n, format)
                            .map_err(|e| xlsx_err(&self.path, e))?;
                    }
                    Cell::DateTime(iso) => {
                        // `2026-10-13T09:35:00+02:00`: Excel kennt keine Zeitzonen, es zählt die lokale
                        // Uhrzeit des Exports.
                        let local = iso.get(..19).unwrap_or(&iso);
                        match ExcelDateTime::parse_from_str(local) {
                            Ok(dt) => {
                                ws.write_datetime_with_format(r, col, &dt, &self.formats.datetime)
                                    .map_err(|e| xlsx_err(&self.path, e))?;
                            }
                            Err(_) => {
                                ws.write_string(r, col, &iso)
                                    .map_err(|e| xlsx_err(&self.path, e))?;
                            }
                        }
                    }
                }
            }
        }
        self.truncated += clipped;
        self.sheet_rows[idx] += 1;
        self.total_rows += 1;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<ExportFileInfo, ExportError> {
        let mut this = *self;
        for idx in 0..this.sheet_names.len() {
            let width = this.sheet_widths[idx];
            if width > 0 {
                let ws = this
                    .workbook
                    .worksheet_from_index(idx)
                    .map_err(|e| xlsx_err(&this.path, e))?;
                ws.autofilter(0, 0, this.sheet_rows[idx], (width - 1) as u16)
                    .map_err(|e| xlsx_err(&this.path, e))?;
            }
        }
        let mut warnings = Vec::new();
        if this.truncated > 0 {
            tracing::warn!(
                file = %this.path.display(),
                cells = this.truncated,
                "Zellen über {MAX_CELL_CHARS} Zeichen auf {MAX_CELL_CHARS} gekürzt (…[truncated])"
            );
            warnings.push(format!(
                "{} Zelle(n) über {MAX_CELL_CHARS} Zeichen auf {MAX_CELL_CHARS} Zeichen gekürzt (…[truncated])",
                this.truncated
            ));
        }
        if this.sheet_names.len() > this.tables {
            tracing::warn!(
                file = %this.path.display(),
                sheets = this.sheet_names.len(),
                "Mehr Datenzeilen als ein Blatt fasst: Fortsetzung auf Folgeblättern"
            );
            warnings.push(format!(
                "Mehr als {} Datenzeilen: Fortsetzung auf {} Blättern ({})",
                this.max_rows,
                this.sheet_names.len(),
                this.sheet_names.join(", ")
            ));
        }
        let target = fs_paths::fs_path(&this.path);
        if let Some(cancel) = this.cancel.take() {
            let file = std::fs::File::create(&target).map_err(|source| ExportError::Io {
                path: this.path.clone(),
                source,
            })?;
            this.workbook
                .save_to_writer(CancellableOutput {
                    file: std::io::BufWriter::new(file),
                    cancel,
                })
                .map_err(|e| xlsx_err(&this.path, e))?;
        } else {
            this.workbook
                .save(&target)
                .map_err(|e| xlsx_err(&this.path, e))?;
        }
        let bytes = std::fs::metadata(&target)
            .map_err(|source| ExportError::Io {
                path: this.path.clone(),
                source,
            })?
            .len();
        Ok(ExportFileInfo {
            path: this.path,
            rows: this.total_rows,
            bytes,
            warnings,
        })
    }
}

impl crate::types::WorkbookTarget for XlsxWriter {
    fn sheet(&mut self, name: &str) -> Result<&mut dyn TableWriter, ExportError> {
        if self.header.is_none() && self.total_rows == 0 && self.sheet_names.len() == 1 {
            let name = sheet_name(name);
            self.workbook
                .worksheet_from_index(0)
                .map_err(|e| xlsx_err(&self.path, e))?
                .set_name(&name)
                .map_err(|e| xlsx_err(&self.path, e))?;
            self.sheet_names[0] = name.clone();
            self.base_name = name;
        } else {
            self.base_name = sheet_name(name);
            self.add_sheet(unique_sheet_name(name, &self.sheet_names))?;
            self.header = None;
            self.tables += 1;
        }
        Ok(self)
    }
}

struct CancellableOutput {
    file: std::io::BufWriter<std::fs::File>,
    cancel: os_model::CancelToken,
}
impl std::io::Write for CancellableOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.cancel
            .checkpoint()
            .map_err(|_| std::io::Error::other("Export cancelled"))?;
        self.file.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.cancel
            .checkpoint()
            .map_err(|_| std::io::Error::other("Export cancelled"))?;
        self.file.flush()
    }
}

/// Neue Arbeitsmappe mit fester Erstellungszeit: `rust_xlsxwriter` setzt sonst die aktuelle Uhrzeit in
/// `docProps/core.xml`, und zwei Exporte derselben Auswahl wären nicht byte-identisch (OUT-001).
pub(crate) fn deterministic_workbook() -> Workbook {
    let mut workbook = Workbook::new();
    if let Ok(epoch) = ExcelDateTime::from_ymd(2000, 1, 1) {
        workbook.set_properties(&DocProperties::new().set_creation_datetime(&epoch));
    }
    workbook
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: OUT-001
    #[test]
    fn workbooks_are_byte_identical_across_seconds() {
        let save = || {
            let mut workbook = deterministic_workbook();
            workbook.add_worksheet().write_string(0, 0, "x").unwrap();
            workbook.save_to_buffer().unwrap()
        };
        let first = save();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(first, save());
    }

    #[test]
    fn sheet_names_are_valid() {
        assert_eq!(sheet_name("Internal All"), "Internal All");
        assert_eq!(
            sheet_name("Response Codes:Client Error (4xx)"),
            "Response Codes Client Error (4x"
        );
        assert_eq!(sheet_name("a[b]c:d*e?f/g\\h"), "a b c d e f g h");
        assert_eq!(sheet_name("'quoted'"), "quoted");
        assert_eq!(sheet_name(""), "Sheet");
        assert_eq!(sheet_name("[]"), "Sheet");
        assert!(utf16_len(&sheet_name(&"x".repeat(100))) <= 31);
    }

    #[test]
    fn unique_names_stay_short() {
        let long = "x".repeat(31);
        let existing = vec![long.clone()];
        let u = unique_sheet_name(&long, &existing);
        assert_eq!(utf16_len(&u), 31);
        assert!(u.ends_with(" (2)"));
        let existing = vec!["Data".to_string(), "DATA (2)".to_string()];
        assert_eq!(unique_sheet_name("data", &existing), "data (3)");
    }

    #[test]
    fn clip_marks_truncation() {
        let long = "a".repeat(40_000);
        let (t, c) = clip(&long);
        assert!(c);
        assert_eq!(t.chars().count(), MAX_CELL_CHARS);
        assert!(t.ends_with(TRUNCATION_MARKER));
        let (t, c) = clip("kurz");
        assert!(!c);
        assert_eq!(t, "kurz");
        // Emoji zählen in Excel als zwei Einheiten.
        let emoji = "🕷".repeat(20_000);
        let (t, _) = clip(&emoji);
        assert!(utf16_len(&t) <= MAX_CELL_CHARS);
    }
}
