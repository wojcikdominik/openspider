//! Exportierbare Seitenleisten-Tabellen (OUT-025). Die App reicht für sitzungsgebundene
//! Panels den sichtbaren Snapshot an `write_snapshot` weiter.
use std::collections::BTreeMap;
use std::path::Path;

use os_analyze::{
    issues,
    views::{self, ViewQuery},
    ColumnKind,
};
use os_model::Value;
use url::Url;

use super::common::{db, pct2};
use crate::catalog::{TableEntry, TableJob};
use crate::error::ExportError;
use crate::types::{ExportFileInfo, Format, ReportResult, TableWriter};

/// Vollständige, bereits sortierte Panel-Daten. Keine Seitengrenze der UI wird übernommen.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelTable {
    pub columns: Vec<(String, ColumnKind)>,
    pub rows: Vec<Vec<Value>>,
}
impl PanelTable {
    fn new(names: &[&str], kinds: &[ColumnKind]) -> Self {
        Self {
            columns: names
                .iter()
                .zip(kinds)
                .map(|(n, k)| ((*n).into(), *k))
                .collect(),
            rows: Vec::new(),
        }
    }
    pub fn write(&self, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
        writer.header(
            &self
                .columns
                .iter()
                .map(|(n, _)| n.clone())
                .collect::<Vec<_>>(),
        )?;
        self.write_rows(writer)
    }
    fn write_rows(&self, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
        let kinds = self.columns.iter().map(|(_, k)| *k).collect::<Vec<_>>();
        for row in &self.rows {
            if row.len() != kinds.len() {
                return Err(ExportError::Write("Panel-Spaltenzahl stimmt nicht".into()));
            }
            writer.row(&kinds, row)?;
        }
        Ok(self.rows.len() as u64)
    }
}

pub fn write_snapshot(
    table: &PanelTable,
    job: &TableJob<'_>,
    path: &Path,
    format: Format,
) -> Result<ExportFileInfo, ExportError> {
    let mut writer = crate::output::open_writer(
        path,
        format,
        job.entry_name,
        &crate::pipeline::writer_options(job.cfg),
    )?;
    table.write(&mut *writer)?;
    writer.finish()
}

pub fn entries() -> &'static [TableEntry] {
    ENTRIES
}
macro_rules! entry {
    ($name:literal, $stem:literal, $header:expr, $run:ident) => {
        TableEntry {
            name: $name,
            aliases: &[],
            file_stem: $stem,
            header: $header,
            header_fn: None,
            requires: &[],
            owner: "M2-D17",
            run: $run,
        }
    };
}
const ENTRIES: &[TableEntry] = &[
    entry!(
        "Sidebar:Overview",
        "Sidebar:Overview",
        &super::overview::HEADER,
        overview
    ),
    entry!(
        "Sidebar:Issues",
        "Sidebar:Issues",
        &crate::catalog::bulk_issues::OVERVIEW_HEADER,
        issue_rows
    ),
    entry!(
        "Sidebar:Site Structure",
        "Sidebar:Site Structure",
        &["Path", "URLs", "Indexable", "Non-Indexable"],
        directories
    ),
    entry!(
        "Sidebar:Site Structure Depth",
        "Sidebar:Site Structure Depth",
        &[
            "Crawl Depth",
            "2xx",
            "3xx",
            "4xx",
            "5xx",
            "No Response",
            "Blocked",
            "Total"
        ],
        depths
    ),
    entry!(
        "Sidebar:Response Times",
        "Sidebar:Response Times",
        &["Response Time Range", "URLs", "% of Total"],
        response_times
    ),
    entry!(
        "Sidebar:Segments",
        "Sidebar:Segments",
        &[
            "Segment",
            "URLs",
            "Indexable",
            "Non-Indexable",
            "Issues",
            "Warnings",
            "Opportunities"
        ],
        segments
    ),
    entry!(
        "Sidebar:Spelling & Grammar",
        "Sidebar:Spelling & Grammar",
        &["Error", "Type", "URLs", "Instances"],
        spelling
    ),
    entry!(
        "Sidebar:API",
        "Sidebar:API",
        &[
            "API",
            "Status",
            "Requested",
            "Received",
            "Errors",
            "Progress"
        ],
        api
    ),
];
fn overview(job: &TableJob<'_>, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    let overview = super::overview::build(job.file, job.cfg, job.tabs)?;
    for row in &overview.rows {
        let (k, v) = row.to_values();
        writer.row(&k, &v)?;
    }
    Ok(overview.rows.len() as u64)
}
fn issue_rows(job: &TableJob<'_>, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    let cancel = os_model::CancelToken::new();
    let mut ctx = crate::types::ExportCtx {
        cancel: &cancel,
        progress: &|_| {},
        skip_empty: false,
        workbook: None,
        skipped: Vec::new(),
    };
    // Die gemeinsame Quelle schreibt auch die identische Kopfzeile.
    // Adapter unterdrückt nur diese zweite Kopfzeile.
    struct RowsOnly<'a>(&'a mut dyn TableWriter);
    impl TableWriter for RowsOnly<'_> {
        fn header(&mut self, _: &[String]) -> Result<(), ExportError> {
            Ok(())
        }
        fn row(&mut self, k: &[ColumnKind], v: &[Value]) -> Result<(), ExportError> {
            self.0.row(k, v)
        }
        fn finish(self: Box<Self>) -> Result<ExportFileInfo, ExportError> {
            Err(ExportError::Write("borrowed writer".into()))
        }
    }
    crate::catalog::bulk_issues::export_to(job, &mut RowsOnly(writer), &mut ctx)
}

/// Beide Site-Structure-Tabellen, identische Grundmenge: interne URLs in den Tabellen.
pub fn site_structure(job: &TableJob<'_>) -> Result<(PanelTable, PanelTable), ExportError> {
    let mut dirs: BTreeMap<String, [i64; 3]> = BTreeMap::new();
    let mut by_depth = [[0i64; 7]; 12];
    let mut stmt=db(job.file.conn().prepare("SELECT address,indexability,crawl_depth,status_code,robots_blocked FROM urls WHERE is_internal=1 ORDER BY id"))?;
    let mut rows = db(stmt.query([]))?;
    while let Some(row) = db(rows.next())? {
        let address: String = db(row.get(0))?;
        let index: Option<String> = db(row.get(1))?;
        if let Ok(url) = Url::parse(&address) {
            let path = url.path();
            let end = path.rfind('/').unwrap_or(0);
            let directory = &path[..=end];
            for (i, c) in directory.char_indices() {
                if c == '/' {
                    let key = format!("{}{}", url.origin().ascii_serialization(), &directory[..=i]);
                    let counts = dirs.entry(key).or_default();
                    counts[0] += 1;
                    counts[if index.as_deref() == Some("Indexable") {
                        1
                    } else {
                        2
                    }] += 1;
                }
            }
        }
        if let Some(depth) = db(row.get::<_, Option<u64>>(2))? {
            let status: Option<u16> = db(row.get(3))?;
            let blocked: bool = db(row.get(4))?;
            let col = if blocked {
                5
            } else {
                match status.map(|s| s / 100) {
                    Some(2) => 0,
                    Some(3) => 1,
                    Some(4) => 2,
                    Some(5) => 3,
                    _ => 4,
                }
            };
            let counts = &mut by_depth[depth.min(11) as usize];
            counts[col] += 1;
            counts[6] += 1;
        }
    }
    let mut directories = PanelTable::new(
        ENTRIES[2].header,
        &[
            ColumnKind::Text,
            ColumnKind::Int,
            ColumnKind::Int,
            ColumnKind::Int,
        ],
    );
    for (path, counts) in dirs {
        let mut row = vec![Value::Text(path)];
        row.extend(counts.map(Value::Int));
        directories.rows.push(row);
    }
    let mut depths = PanelTable::new(
        ENTRIES[3].header,
        &[
            ColumnKind::Text,
            ColumnKind::Int,
            ColumnKind::Int,
            ColumnKind::Int,
            ColumnKind::Int,
            ColumnKind::Int,
            ColumnKind::Int,
            ColumnKind::Int,
        ],
    );
    for (depth, counts) in by_depth.into_iter().enumerate() {
        let mut row = vec![Value::Text(if depth == 11 {
            "10+".into()
        } else {
            depth.to_string()
        })];
        row.extend(counts.map(Value::Int));
        depths.rows.push(row);
    }
    Ok((directories, depths))
}
fn directories(job: &TableJob<'_>, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    site_structure(job)?.0.write_rows(writer)
}
fn depths(job: &TableJob<'_>, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    site_structure(job)?.1.write_rows(writer)
}

/// Site Structure: zwei CSV-Dateien oder ein Workbook mit zwei Blättern.
pub fn export_site_structure(
    job: &TableJob<'_>,
    path: &Path,
    format: Format,
) -> Result<ReportResult, ExportError> {
    let (dirs, depths) = site_structure(job)?;
    if format == Format::Csv {
        return Ok(ReportResult {
            files: vec![
                write_snapshot(
                    &dirs,
                    job,
                    &path.join("site_structure_directories.csv"),
                    format,
                )?,
                write_snapshot(&depths, job, &path.join("site_structure_depth.csv"), format)?,
            ],
            warnings: vec![],
        });
    }
    // Nur Zahlen und Texte; Daten werden mit konstantem Speicher je Blatt geschrieben.
    if let Some(parent) = path.parent() {
        crate::fs_paths::create_dir_all(parent).map_err(|source| ExportError::Io {
            path: parent.into(),
            source,
        })?;
    }
    let mut workbook = crate::xlsx::deterministic_workbook();
    let mut count = 0;
    for (name, table) in [("Directories", dirs), ("Crawl Depth", depths)] {
        let sheet = workbook
            .add_worksheet_with_constant_memory()
            .map_err(|e| ExportError::Write(e.to_string()))?;
        sheet
            .set_name(name)
            .map_err(|e| ExportError::Write(e.to_string()))?;
        for (c, (name, _)) in table.columns.iter().enumerate() {
            sheet
                .write_string(0, c as u16, name)
                .map_err(|e| ExportError::Write(e.to_string()))?;
        }
        for (r, row) in table.rows.iter().enumerate() {
            for (c, value) in row.iter().enumerate() {
                match value {
                    Value::Text(s) => {
                        sheet
                            .write_string(r as u32 + 1, c as u16, s)
                            .map_err(|e| ExportError::Write(e.to_string()))?;
                    }
                    Value::Int(n) => {
                        sheet
                            .write_number(r as u32 + 1, c as u16, *n as f64)
                            .map_err(|e| ExportError::Write(e.to_string()))?;
                    }
                    _ => {}
                }
            }
            count += 1;
        }
    }
    workbook
        .save(path)
        .map_err(|e| ExportError::Write(e.to_string()))?;
    let bytes = std::fs::metadata(path)
        .map_err(|source| ExportError::Io {
            path: path.into(),
            source,
        })?
        .len();
    Ok(ReportResult {
        files: vec![ExportFileInfo {
            path: path.into(),
            rows: count,
            bytes,
            warnings: vec![],
        }],
        warnings: vec![],
    })
}
fn response_times(job: &TableJob<'_>, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    let mut buckets = [0u64; 11];
    let mut stmt=db(job.file.conn().prepare("SELECT MIN(response_time_ms/1000,10),COUNT(*) FROM urls WHERE in_tables=1 AND state='done' AND robots_blocked=0 AND status_code IS NOT NULL AND response_time_ms>=0 GROUP BY 1"))?;
    for row in db(stmt.query_map([], |r| Ok((r.get::<_, usize>(0)?, r.get::<_, u64>(1)?))))? {
        let (i, n) = db(row)?;
        buckets[i] = n;
    }
    let total = buckets.iter().sum();
    for (i, n) in buckets.into_iter().enumerate() {
        writer.row(
            &[
                ColumnKind::Text,
                ColumnKind::Int,
                ColumnKind::Float { decimals: 2 },
            ],
            &[
                Value::Text(if i == 10 {
                    "Over 10 s".into()
                } else {
                    format!("{i}–{} s", i + 1)
                }),
                Value::Int(n as i64),
                pct2(n, total).map_or(Value::Null, Value::Float),
            ],
        )?;
    }
    Ok(11)
}
fn spelling(job: &TableJob<'_>, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    let mut stmt=db(job.file.conn().prepare("SELECT text,CASE lower(kind) WHEN 'spelling' THEN 'Spelling' ELSE 'Grammar' END,COUNT(DISTINCT url_id),COUNT(*) FROM spelling_findings GROUP BY text,lower(kind) ORDER BY COUNT(DISTINCT url_id) DESC,text COLLATE OS_TEXT,text COLLATE BINARY,lower(kind) LIMIT 100"))?;
    let mut rows = db(stmt.query([]))?;
    let mut n = 0;
    while let Some(r) = db(rows.next())? {
        writer.row(
            &[
                ColumnKind::Text,
                ColumnKind::Text,
                ColumnKind::Int,
                ColumnKind::Int,
            ],
            &[
                Value::Text(db(r.get(0))?),
                Value::Text(db(r.get(1))?),
                Value::Int(db(r.get(2))?),
                Value::Int(db(r.get(3))?),
            ],
        )?;
        n += 1;
    }
    Ok(n)
}
fn api(job: &TableJob<'_>, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    let mut stmt = db(job.file.conn().prepare(
        "SELECT source,state,urls_total,urls_done,urls_failed FROM api_status
         ORDER BY CASE source WHEN 'ga4' THEN 0 WHEN 'gsc' THEN 1 WHEN 'psi' THEN 2
           WHEN 'majestic' THEN 3 WHEN 'ahrefs' THEN 4 WHEN 'moz' THEN 5 ELSE 6 END,source",
    ))?;
    let mut rows = db(stmt.query([]))?;
    let mut n = 0;
    while let Some(r) = db(rows.next())? {
        let requested: u64 = db(r.get(2))?;
        let received: u64 = db(r.get(3))?;
        writer.row(
            &[
                ColumnKind::Text,
                ColumnKind::Text,
                ColumnKind::Int,
                ColumnKind::Int,
                ColumnKind::Int,
                ColumnKind::Float { decimals: 2 },
            ],
            &[
                Value::Text(db(r.get(0))?),
                Value::Text(db(r.get(1))?),
                Value::Int(requested as i64),
                Value::Int(received as i64),
                Value::Int(db(r.get(4))?),
                pct2(received, requested).map_or(Value::Null, Value::Float),
            ],
        )?;
        n += 1;
    }
    Ok(n)
}
fn segments(job: &TableJob<'_>, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    let counts = views::filter_counts_in(job.tabs, job.file, job.cfg)?;
    let issues = issues::issues_from_counts(&counts);
    for (id, segment) in job.cfg.segments.list.iter().enumerate() {
        let scope = format!(
            "u.in_tables=1 AND u.id IN (SELECT url_id FROM url_segments WHERE segment_id={id})"
        );
        let mut row = vec![Value::Text(segment.name.clone())];
        for extra in [
            "1",
            "u.indexability='Indexable'",
            "COALESCE(u.indexability,'')<>'Indexable'",
        ] {
            row.push(Value::Int(super::common::count(
                job.file,
                &format!("SELECT COUNT(*) FROM urls u WHERE {scope} AND {extra}"),
            )? as i64));
        }
        for kind in ["Issue", "Warning", "Opportunity"] {
            let mut n = 0;
            for issue in issues.iter().filter(|i| i.issue_type.as_str() == kind) {
                let plan = views::plan_in(
                    job.tabs,
                    &ViewQuery::new(&issue.tab, &issue.filter),
                    job.cfg,
                )?;
                let sql=format!("SELECT EXISTS(SELECT 1 FROM urls u LEFT JOIN url_counters c ON c.url_id=u.id WHERE {scope} AND ({}) AND ({}))",plan.population_sql,plan.filter_sql);
                let mut stmt = db(job.file.conn().prepare(&sql))?;
                for (name, value) in plan.params {
                    if let Some(i) = db(stmt.parameter_index(&name))? {
                        match value {
                            Value::Int(v) => db(stmt.raw_bind_parameter(i, v))?,
                            Value::Float(v) => db(stmt.raw_bind_parameter(i, v))?,
                            Value::Text(v) => db(stmt.raw_bind_parameter(i, v))?,
                            Value::Bool(v) => db(stmt.raw_bind_parameter(i, v))?,
                            _ => db(stmt.raw_bind_parameter(i, Option::<String>::None))?,
                        }
                    }
                }
                n += db(stmt.raw_query().next())?
                    .map(|r| r.get::<_, i64>(0))
                    .transpose()
                    .map_err(|e| ExportError::Store(e.into()))?
                    .unwrap_or(0);
            }
            row.push(Value::Int(n));
        }
        writer.row(
            &[
                ColumnKind::Text,
                ColumnKind::Int,
                ColumnKind::Int,
                ColumnKind::Int,
                ColumnKind::Int,
                ColumnKind::Int,
                ColumnKind::Int,
            ],
            &row,
        )?;
    }
    Ok(job.cfg.segments.list.len() as u64)
}
