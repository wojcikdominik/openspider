//! CSV-Schreiber (RFC 4180, UTF-8, BOM und Zeilenende nach `export.*`, OUT-002).
//!
//! * Felder mit Trennzeichen, `"`, CR oder LF stehen in `"`, enthaltene `"` werden verdoppelt; Zeilenumbrüche
//!   in Feldern bleiben erhalten.
//! * UTF-8, mit [`WriterOptions::utf8_bom`] beginnt die Datei mit `EF BB BF`; Zeilenende nach
//!   [`WriterOptions::line_ending`].
//! * Zahlen stehen ohne Anführungszeichen und mit Dezimalpunkt. Der Code liest weder `LANG` noch `LC_*`;
//!   formatiert wird ausschließlich mit der locale-unabhängigen Rust-Formatierung.
//! * Leere Zellen für `Null` (06 §0), Prozentwerte/Dezimalzahlen mit den festen Nachkommastellen aus
//!   [`ColumnKind::Float`], Zeitstempel als ISO 8601 mit Offset, Sekunden (`Seconds3`; `views` liefert sie bereits in Sekunden) mit drei Stellen.
//! * [`ColumnKind::Url`] wird wie Text geschrieben (nie als Formel oder Link).
//! * Ungültiges UTF-8 aus Quelldaten ersetzt [`utf8_lossy`] durch U+FFFD; `Value::Text` ist per Typ immer
//!   gültig, die Ersetzung geschieht beim Einlesen der Quelle.

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use os_analyze::ColumnKind;
use os_model::{Timestamp, Value};
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};

use crate::error::ExportError;
use crate::fs_paths;
use crate::types::{ExportFileInfo, TableWriter, WriterOptions};

/// Maps canonical keys using the presentation labels of this writer.
pub(crate) fn header_labels(
    columns: &[String],
    labels: &std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    columns
        .iter()
        .map(|key| labels.get(key).unwrap_or(key).clone())
        .collect()
}

/// UTF-8-BOM.
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Wandelt Bytes verlustbehaftet in UTF-8: ungültige Folgen werden durch U+FFFD ersetzt (OUT-002).
pub fn utf8_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Zeilenweiser CSV-Schreiber.
pub struct CsvWriter {
    header_labels: std::collections::BTreeMap<String, String>,
    path: PathBuf,
    writer: ::csv::Writer<BufWriter<std::fs::File>>,
    rows: u64,
    columns: Option<usize>,
}

impl CsvWriter {
    /// Legt die Datei an (vorhandene gleichnamige Datei wird ersetzt, fehlende Elternordner werden angelegt)
    /// und schreibt bei Bedarf die BOM.
    pub fn create(path: &Path, opts: &WriterOptions) -> Result<Self, ExportError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs_paths::create_dir_all(parent).map_err(|source| ExportError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let file = fs_paths::create_file(path).map_err(|source| ExportError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut buf = BufWriter::with_capacity(256 * 1024, file);
        if opts.utf8_bom {
            buf.write_all(&BOM).map_err(|source| ExportError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        }
        let terminator = match opts.line_ending {
            os_model::LineEnding::Crlf => ::csv::Terminator::CRLF,
            os_model::LineEnding::Lf => ::csv::Terminator::Any(b'\n'),
        };
        let writer = ::csv::WriterBuilder::new()
            .delimiter(b',')
            .terminator(terminator)
            .quote_style(::csv::QuoteStyle::Necessary)
            .flexible(true)
            .from_writer(buf);
        Ok(CsvWriter {
            header_labels: opts.header_labels.clone(),
            path: path.to_path_buf(),
            writer,
            rows: 0,
            columns: None,
        })
    }

    fn map_err(&self, e: ::csv::Error) -> ExportError {
        match e.into_kind() {
            ::csv::ErrorKind::Io(source) => ExportError::Io {
                path: self.path.clone(),
                source,
            },
            other => ExportError::Write(format!("{other:?}")),
        }
    }
}

impl TableWriter for CsvWriter {
    fn format(&self) -> Option<crate::types::Format> {
        Some(crate::types::Format::Csv)
    }
    fn header(&mut self, columns: &[String]) -> Result<(), ExportError> {
        if self.columns.is_some() || self.rows > 0 {
            return Err(ExportError::Write(
                "Kopfzeile wurde bereits geschrieben".to_string(),
            ));
        }
        self.columns = Some(columns.len());
        self.writer
            .write_record(
                header_labels(columns, &self.header_labels)
                    .iter()
                    .map(|c| c.as_bytes()),
            )
            .map_err(|e| self.map_err(e))
    }

    fn row(&mut self, kinds: &[ColumnKind], values: &[Value]) -> Result<(), ExportError> {
        if let Some(n) = self.columns {
            if n != values.len() {
                return Err(ExportError::Write(format!(
                    "Zeile hat {} Werte, die Kopfzeile {n} Spalten",
                    values.len()
                )));
            }
        }
        let fields: Vec<String> = values
            .iter()
            .enumerate()
            .map(|(i, v)| cell_text(kinds.get(i).copied().unwrap_or(ColumnKind::Text), v))
            .collect();
        self.writer
            .write_record(fields.iter().map(|f| f.as_bytes()))
            .map_err(|e| self.map_err(e))?;
        self.rows += 1;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<ExportFileInfo, ExportError> {
        let CsvWriter {
            path, writer, rows, ..
        } = *self;
        let mut inner = writer.into_inner().map_err(|e| ExportError::Io {
            path: path.clone(),
            source: e.into_error(),
        })?;
        inner.flush().map_err(|source| ExportError::Io {
            path: path.clone(),
            source,
        })?;
        drop(inner);
        let bytes = std::fs::metadata(fs_paths::fs_path(&path))
            .map_err(|source| ExportError::Io {
                path: path.clone(),
                source,
            })?
            .len();
        // CSV kennt weder Zelllängen- noch Zeilengrenzen: nichts wird gekürzt, es gibt keine Warnungen.
        Ok(ExportFileInfo {
            path,
            rows,
            bytes,
            warnings: Vec::new(),
        })
    }
}

/// Textform einer Zelle nach 06 §0 (gemeinsame Regeln für CSV; XLSX nutzt dieselben Zeitstempel- und
/// Zahlenregeln, schreibt aber typisierte Zellen).
pub(crate) fn cell_text(kind: ColumnKind, v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Text(s) => match kind {
            ColumnKind::Timestamp => timestamp_export(v, None).unwrap_or_else(|| s.clone()),
            _ => s.clone(),
        },
        Value::Int(i) => match kind {
            ColumnKind::Bool if *i == 0 || *i == 1 => (*i == 1).to_string(),
            ColumnKind::Float { decimals } => fixed(*i as f64, decimals),
            ColumnKind::Seconds3 => fixed(*i as f64, 3),
            ColumnKind::Timestamp => timestamp_export(v, None).unwrap_or_else(|| i.to_string()),
            _ => i.to_string(),
        },
        Value::Float(f) => {
            if !f.is_finite() {
                return String::new();
            }
            match kind {
                ColumnKind::Float { decimals } => fixed(*f, decimals),
                ColumnKind::Seconds3 => fixed(*f, 3),
                ColumnKind::Int => {
                    if f.fract() == 0.0 && f.abs() < 9.0e15 {
                        format!("{}", *f as i64)
                    } else {
                        format!("{f}")
                    }
                }
                ColumnKind::Timestamp => {
                    timestamp_export(v, None).unwrap_or_else(|| format!("{f}"))
                }
                _ => format!("{f}"),
            }
        }
    }
}

/// Dezimalzahl mit festen Nachkommastellen und Punkt; `-0.00` wird zu `0.00`.
pub(crate) fn fixed(f: f64, decimals: u8) -> String {
    if !f.is_finite() {
        return String::new();
    }
    let s = format!("{:.*}", usize::from(decimals), f);
    if let Some(rest) = s.strip_prefix('-') {
        if rest.bytes().all(|b| b == b'0' || b == b'.') {
            return rest.to_string();
        }
    }
    s
}

/// Zeitstempel in Exportform (ISO 8601 mit Offset, ohne Sekundenbruchteile), 06 §0.
///
/// * `Value::Text` mit RFC-3339-Inhalt: endet er auf `Z` (Datenbankform, UTC), wird er in den lokalen
///   Offset umgerechnet (`offset` oder, bei `None`, der lokale Offset des Systems); trägt er einen
///   ausdrücklichen Offset, bleibt dieser erhalten.
/// * `Value::Int`: Unix-Millisekunden (UTC), umgerechnet wie die Datenbankform.
/// * sonst `None` – der Aufrufer schreibt den Wert unverändert als Text.
pub(crate) fn timestamp_export(v: &Value, offset: Option<UtcOffset>) -> Option<String> {
    let render = |ts: Timestamp| match offset {
        Some(o) => ts.to_export_with(o),
        None => ts.to_export(),
    };
    match v {
        Value::Int(ms) => Some(render(Timestamp::from_unix_millis(*ms))),
        Value::Text(s) => {
            let s = s.trim();
            let t = OffsetDateTime::parse(s, &Rfc3339).ok()?;
            if s.ends_with('Z') || s.ends_with('z') {
                Some(render(Timestamp(t)))
            } else {
                Some(Timestamp(t).to_export_with(t.offset()))
            }
        }
        _ => None,
    }
}

/// An owned, streaming table reader for alternate export targets.
pub type CsvRows = Box<dyn Iterator<Item = Result<Vec<String>, ExportError>> + Send>;

/// Reads a staged CSV table without collecting its rows (CLI target adapters, OUT-195).
/// Owns the file, so consumers can stream rows across asynchronous upload boundaries.
pub fn read_table(path: &Path) -> Result<(Vec<String>, CsvRows), ExportError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(fs_paths::fs_path(path))
        .map_err(|e| ExportError::Write(e.to_string()))?;
    let header = reader
        .headers()
        .map_err(|e| ExportError::Write(e.to_string()))?
        .iter()
        .map(str::to_owned)
        .collect();
    let rows = reader.into_records().map(|row| {
        row.map(|r| r.iter().map(str::to_owned).collect())
            .map_err(|e| ExportError::Write(e.to_string()))
    });
    Ok((header, Box::new(rows)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_use_dot() {
        assert_eq!(
            cell_text(ColumnKind::Float { decimals: 2 }, &Value::Float(12.5)),
            "12.50"
        );
        assert_eq!(
            cell_text(ColumnKind::Float { decimals: 0 }, &Value::Float(0.4)),
            "0"
        );
        assert_eq!(
            cell_text(ColumnKind::Float { decimals: 2 }, &Value::Float(-0.001)),
            "0.00"
        );
        // Seconds3: Werte kommen von `views` bereits in Sekunden, keine Umrechnung im Schreiber.
        assert_eq!(
            cell_text(ColumnKind::Seconds3, &Value::Float(1.234)),
            "1.234"
        );
        assert_eq!(
            cell_text(ColumnKind::Seconds3, &Value::Float(-0.005)),
            "-0.005"
        );
        assert_eq!(cell_text(ColumnKind::Seconds3, &Value::Int(2)), "2.000");
        assert_eq!(cell_text(ColumnKind::Int, &Value::Int(-7)), "-7");
        assert_eq!(cell_text(ColumnKind::Int, &Value::Null), "");
        assert_eq!(cell_text(ColumnKind::Bool, &Value::Bool(true)), "true");
        assert_eq!(cell_text(ColumnKind::Bool, &Value::Int(0)), "false");
        assert_eq!(
            cell_text(ColumnKind::Float { decimals: 2 }, &Value::Float(f64::NAN)),
            ""
        );
    }

    #[test]
    fn timestamps_with_explicit_offset() {
        let plus2 = UtcOffset::from_hms(2, 0, 0).unwrap();
        let v = Value::Text("2026-10-13T07:35:00.123Z".into());
        assert_eq!(
            timestamp_export(&v, Some(plus2)).as_deref(),
            Some("2026-10-13T09:35:00+02:00")
        );
        let kept = Value::Text("2026-10-13T09:35:00+05:30".into());
        assert_eq!(
            timestamp_export(&kept, Some(plus2)).as_deref(),
            Some("2026-10-13T09:35:00+05:30")
        );
        assert_eq!(timestamp_export(&Value::Text("gestern".into()), None), None);
        assert_eq!(
            timestamp_export(&Value::Int(0), Some(UtcOffset::UTC)).as_deref(),
            Some("1970-01-01T00:00:00+00:00")
        );
    }

    #[test]
    fn lossy_replaces_invalid_bytes() {
        assert_eq!(utf8_lossy(b"caf\xe9"), "caf\u{fffd}");
    }
}
