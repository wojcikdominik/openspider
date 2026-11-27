//! Ausgabeordner (OUT-013) und Auswahl des Tabellenschreibers je Format (`open_writer`).
//!
//! * [`OutputMode::Overwrite`]: Ordner wird bei Bedarf angelegt; die Schreiber ersetzen nur gleichnamige
//!   Dateien, andere Dateien bleiben unangetastet.
//! * [`OutputMode::Timestamped`]: Unterordner `YYYY-MM-DD_HH-MM-SS` in lokaler Zeit; existiert er schon
//!   (gleiche Sekunde), folgt `_2`, `_3` ….
//! * [`OutputMode::RequireEmpty`]: Fehler, wenn der Ordner existiert und nicht leer ist.
//!
//! Alle Fehler sind [`ExportError::Output`]; die CLI bildet sie auf Exit-Code 2 ab, bevor der Crawl beginnt.

use std::path::{Path, PathBuf};

use os_model::Clock;
use time::UtcOffset;

use crate::csv::CsvWriter;
use crate::error::ExportError;
use crate::fs_paths;
use crate::types::{Format, TableWriter, WriterOptions};
use crate::xlsx::XlsxWriter;

/// Umgang mit vorhandenen Dateien im Ausgabeordner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Gleichnamige Dateien ersetzen.
    Overwrite,
    /// Unterordner mit Zeitstempel `YYYY-MM-DD_HH-MM-SS` (lokal).
    Timestamped,
    /// Fehler, wenn der Ordner nicht leer ist.
    RequireEmpty,
}

/// Bereitet den Ausgabeordner vor und liefert den tatsächlichen Pfad.
///
/// Die Uhr kommt von außen (`Clock`), damit die Ordnernamen deterministisch testbar sind; den lokalen
/// UTC-Offset liefert `os_model::time::init_local_offset` (bzw. das System).
pub fn prepare(base: &Path, mode: OutputMode, clock: &dyn Clock) -> Result<PathBuf, ExportError> {
    prepare_impl(base, mode, |_| clock.now().to_export())
}

/// Wie [`prepare`], mit ausdrücklichem UTC-Offset statt der lokalen Zeitzone (Tests, feste Zeitzonen).
pub fn prepare_with_offset(
    base: &Path,
    mode: OutputMode,
    clock: &dyn Clock,
    offset: UtcOffset,
) -> Result<PathBuf, ExportError> {
    prepare_impl(base, mode, |_| clock.now().to_export_with(offset))
}

fn prepare_impl(
    base: &Path,
    mode: OutputMode,
    now_export: impl Fn(()) -> String,
) -> Result<PathBuf, ExportError> {
    match mode {
        OutputMode::Overwrite => {
            ensure_dir(base)?;
            Ok(base.to_path_buf())
        }
        OutputMode::RequireEmpty => {
            let real = fs_paths::fs_path(base);
            if real.exists() {
                if !real.is_dir() {
                    return Err(ExportError::Output(format!(
                        "{} ist kein Ordner",
                        base.display()
                    )));
                }
                let mut entries = std::fs::read_dir(&real).map_err(|e| {
                    ExportError::Output(format!("{} nicht lesbar: {e}", base.display()))
                })?;
                if entries.next().is_some() {
                    return Err(ExportError::Output(format!(
                        "Der Ausgabeordner {} ist nicht leer (require-empty)",
                        base.display()
                    )));
                }
                Ok(base.to_path_buf())
            } else {
                ensure_dir(base)?;
                Ok(base.to_path_buf())
            }
        }
        OutputMode::Timestamped => {
            ensure_dir(base)?;
            let stamp = dir_stamp(&now_export(()));
            let mut n = 1u32;
            loop {
                let name = if n == 1 {
                    stamp.clone()
                } else {
                    format!("{stamp}_{n}")
                };
                let candidate = base.join(&name);
                // `create_dir` (nicht `_all`) schlägt fehl, wenn der Ordner existiert: atomar gegen
                // gleichzeitige Läufe.
                match std::fs::create_dir(fs_paths::fs_path(&candidate)) {
                    Ok(()) => return Ok(candidate),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
                    Err(e) => {
                        return Err(ExportError::Output(format!(
                            "Ordner {} nicht anlegbar: {e}",
                            candidate.display()
                        )))
                    }
                }
            }
        }
    }
}

/// `2026-10-13T09:35:00+02:00` → `2026-10-13_09-35-00`.
fn dir_stamp(export: &str) -> String {
    export
        .chars()
        .take(19)
        .map(|c| match c {
            'T' => '_',
            ':' => '-',
            other => other,
        })
        .collect()
}

fn ensure_dir(dir: &Path) -> Result<(), ExportError> {
    fs_paths::create_dir_all(dir).map_err(|e| {
        ExportError::Output(format!(
            "Ausgabeordner {} nicht anlegbar: {e}",
            dir.display()
        ))
    })?;
    if !fs_paths::fs_path(dir).is_dir() {
        return Err(ExportError::Output(format!(
            "{} ist kein Ordner",
            dir.display()
        )));
    }
    Ok(())
}

/// Öffnet einen Tabellenschreiber für das Format.
pub fn open_writer(
    path: &Path,
    format: Format,
    sheet: &str,
    opts: &WriterOptions,
) -> Result<Box<dyn TableWriter>, ExportError> {
    match format {
        Format::Csv => Ok(Box::new(CsvWriter::create(path, opts)?)),
        Format::Xlsx => Ok(Box::new(XlsxWriter::create(path, sheet, opts)?)),
    }
}

/// Stages an output next to its destination. Dropping it removes incomplete data.
pub(crate) struct PendingFile {
    pub path: PathBuf,
    destination: PathBuf,
}
impl PendingFile {
    pub fn new(destination: &Path) -> Result<Self, ExportError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        if let Some(parent) = destination.parent().filter(|p| !p.as_os_str().is_empty()) {
            ensure_dir(parent)?;
        }
        loop {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = destination.with_extension(format!("{}.{}.part", std::process::id(), id));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(fs_paths::fs_path(&path))
            {
                Ok(_) => {
                    return Ok(Self {
                        path,
                        destination: destination.into(),
                    })
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err(ExportError::Io { path, source }),
            }
        }
    }
    pub fn publish(
        &self,
        mut info: crate::types::ExportFileInfo,
    ) -> Result<crate::types::ExportFileInfo, ExportError> {
        std::fs::rename(
            fs_paths::fs_path(&self.path),
            fs_paths::fs_path(&self.destination),
        )
        .map_err(|source| ExportError::Io {
            path: self.destination.clone(),
            source,
        })?;
        info.path = self.destination.clone();
        Ok(info)
    }
}
impl Drop for PendingFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(fs_paths::fs_path(&self.path));
    }
}

/// Defers opening a file or adding a worksheet until the first data row.
struct DeferredWriter<'a, 'target> {
    target: Option<&'a mut (dyn crate::types::WorkbookTarget + 'target)>,
    shared: Option<&'a mut dyn TableWriter>,
    owned: Option<Box<dyn TableWriter>>,
    pending: Option<PendingFile>,
    path: &'a Path,
    format: Format,
    sheet: &'a str,
    opts: &'a WriterOptions,
    header: Option<Vec<String>>,
    cancel: &'a os_model::CancelToken,
    progress: &'a dyn Fn(u64),
    rows: u64,
}
impl DeferredWriter<'_, '_> {
    fn start(&mut self) -> Result<(), ExportError> {
        if self.shared.is_some() || self.owned.is_some() {
            return Ok(());
        }
        if let Some(target) = self.target.take() {
            self.shared = Some(target.sheet(self.sheet)?);
        } else {
            let pending = PendingFile::new(self.path)?;
            self.owned = Some(match self.format {
                Format::Csv => {
                    Box::new(CsvWriter::create(&pending.path, self.opts)?) as Box<dyn TableWriter>
                }
                Format::Xlsx => {
                    let mut writer = XlsxWriter::create(&pending.path, self.sheet, self.opts)?;
                    writer.set_cancel(self.cancel.clone());
                    Box::new(writer)
                }
            });
            self.pending = Some(pending);
        }
        let header = self.header.clone().unwrap_or_default();
        self.writer().header(&header)
    }
    fn writer(&mut self) -> &mut dyn TableWriter {
        match self.shared.as_mut() {
            Some(w) => &mut **w,
            None => self.owned.as_mut().expect("writer opened").as_mut(),
        }
    }
}
impl TableWriter for DeferredWriter<'_, '_> {
    fn checkpoint(&mut self) -> Result<(), ExportError> {
        self.cancel
            .checkpoint()
            .map_err(|_| ExportError::Write("Export abgebrochen".into()))
    }
    fn header(&mut self, columns: &[String]) -> Result<(), ExportError> {
        if self.header.is_some() {
            return Err(ExportError::Write("Duplicate table header".into()));
        }
        self.header = Some(columns.to_vec());
        Ok(())
    }
    fn row(
        &mut self,
        kinds: &[os_analyze::ColumnKind],
        values: &[os_model::Value],
    ) -> Result<(), ExportError> {
        self.cancel
            .checkpoint()
            .map_err(|_| ExportError::Write("Export abgebrochen".into()))?;
        self.start()?;
        self.writer().row(kinds, values)?;
        self.rows += 1;
        (self.progress)(self.rows);
        Ok(())
    }
    fn finish(self: Box<Self>) -> Result<crate::types::ExportFileInfo, ExportError> {
        Err(ExportError::Write(
            "Deferred writer is finished by its owner".into(),
        ))
    }
    fn format(&self) -> Option<Format> {
        Some(self.format)
    }
}

/// Runs a table with cancellation, progress, empty suppression and optional workbook consolidation.
#[allow(clippy::too_many_arguments)]
pub fn write_table(
    path: &Path,
    format: Format,
    sheet: &str,
    opts: &WriterOptions,
    ctx: &mut crate::types::ExportCtx<'_>,
    run: impl FnOnce(&mut dyn TableWriter) -> Result<(), ExportError>,
) -> Result<Option<crate::types::ExportFileInfo>, ExportError> {
    ctx.checkpoint()?;
    let mut writer = DeferredWriter {
        target: ctx.workbook.as_deref_mut(),
        shared: None,
        owned: None,
        pending: None,
        path,
        format,
        sheet,
        opts,
        header: None,
        cancel: ctx.cancel,
        progress: ctx.progress,
        rows: 0,
    };
    run(&mut writer)?;
    writer
        .cancel
        .checkpoint()
        .map_err(|_| ExportError::Write("Export abgebrochen".into()))?;
    if writer.rows == 0 && ctx.skip_empty {
        drop(writer);
        ctx.skipped.push(sheet.into());
        return Ok(None);
    }
    writer.start()?;
    if let Some(owned) = writer.owned.take() {
        let info = owned.finish()?;
        writer
            .cancel
            .checkpoint()
            .map_err(|_| ExportError::Write("Export abgebrochen".into()))?;
        return writer
            .pending
            .as_ref()
            .expect("file target")
            .publish(info)
            .map(Some);
    }
    Ok(None)
}
