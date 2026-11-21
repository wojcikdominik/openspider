//! Migration von Konfigurationsdateien älterer Versionen (OUT-226).
//!
//! * **Lesen** ändert nie eine Datei: [`CrawlConfig::load_str`] lädt jede Version, fehlende Schlüssel erhalten
//!   den Standardwert (CRAWL-132). Eine neuere `schema_version` lädt mit Warnung.
//! * **Migrieren** ([`migrate_toml`], [`CrawlConfig::migrate_file`]) schreibt die Datei im aktuellen Format neu:
//!   alle Werte der alten Datei bleiben erhalten, alle neuen Abschnitte erscheinen mit ihren Standardwerten,
//!   `schema_version` und `generator` stehen auf dem aktuellen Stand. Vorher entsteht eine Sicherung
//!   `<datei>.bak-v<alte Version>`; ein vorhandener Name wird nie überschrieben (`.bak-v1`, `.bak-v1-2`, …).
//! * **Version 1 → 2** (M2-F01): rein additiv, kein Schlüssel ändert Ort oder Bedeutung. Kommentare der Datei
//!   gehen beim Neuschreiben verloren (sie bleiben in der Sicherung); unbekannte Schlüssel werden nicht
//!   übernommen und im Bericht genannt.
//! * Dateien mit gleicher oder neuerer Version werden nicht verändert.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::io::ConfigLoadError;
use crate::config::{CrawlConfig, CONFIG_SCHEMA_VERSION};

/// Ergebnis einer Migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    /// Version der gelesenen Datei (fehlt `schema_version`, gilt 1).
    pub from_version: u32,
    /// Version nach der Migration ([`CONFIG_SCHEMA_VERSION`]).
    pub to_version: u32,
    /// Pfad der Sicherungskopie (nur bei [`CrawlConfig::migrate_file`]).
    pub backup: Option<PathBuf>,
    /// Neue Abschnitte, die mit Standardwerten ergänzt wurden.
    pub added_sections: Vec<String>,
    /// Schlüssel der alten Datei, die nicht übernommen wurden (unbekannt), mit Feldpfad.
    pub dropped_keys: Vec<String>,
}

/// Abschnitte, die Version 2 gegenüber Version 1 ergänzt (oberste Ebene).
const V2_SECTIONS: [&str; 15] = [
    "ai",
    "embeddings",
    "spelling",
    "grammar",
    "custom_search",
    "custom_extraction",
    "custom_js",
    "link_positions",
    "rewrite",
    "cdns",
    "http",
    "api",
    "segments",
    "compare",
    "sitemap_gen",
];

fn file_version(src: &str) -> u32 {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    toml::from_str::<toml::Table>(src)
        .ok()
        .and_then(|t| t.get("schema_version").and_then(toml::Value::as_integer))
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(1)
}

/// Migriert den Text einer `.osconfig` auf die aktuelle Version. Gibt den neuen Text und den Bericht zurück;
/// bei gleicher oder neuerer Version ist der Text unverändert (`from_version == to_version` bzw. größer).
pub fn migrate_toml(src: &str) -> Result<(String, MigrationReport), ConfigLoadError> {
    let from = file_version(src);
    if from >= CONFIG_SCHEMA_VERSION {
        return Ok((
            src.to_string(),
            MigrationReport {
                from_version: from,
                to_version: from,
                backup: None,
                added_sections: Vec::new(),
                dropped_keys: Vec::new(),
            },
        ));
    }
    let loaded = CrawlConfig::load_str(src)?;
    let present: Vec<String> =
        toml::from_str::<toml::Table>(src.strip_prefix('\u{feff}').unwrap_or(src))
            .map(|t| t.keys().cloned().collect())
            .unwrap_or_default();
    let added_sections = V2_SECTIONS
        .iter()
        .filter(|s| !present.iter().any(|p| p == *s))
        .map(|s| s.to_string())
        .collect();
    let dropped_keys = loaded
        .warnings
        .iter()
        .filter(|w| w.kind == crate::config::ConfigIssueKind::UnknownField)
        .map(|w| w.path.clone())
        .collect();
    let text = loaded
        .config
        .to_osconfig_string()
        .map_err(|e| ConfigLoadError {
            file: None,
            line: None,
            column: None,
            field: None,
            message: e.to_string(),
        })?;
    Ok((
        text,
        MigrationReport {
            from_version: from,
            to_version: CONFIG_SCHEMA_VERSION,
            backup: None,
            added_sections,
            dropped_keys,
        },
    ))
}

impl CrawlConfig {
    /// Migriert eine `.osconfig`-Datei auf die aktuelle Version (siehe Moduldokumentation). `Ok(None)`: die Datei
    /// hat bereits die aktuelle oder eine neuere Version und bleibt unverändert.
    pub fn migrate_file(path: &Path) -> Result<Option<MigrationReport>, ConfigLoadError> {
        let fail = |what: &str, e: std::io::Error| ConfigLoadError {
            file: Some(path.to_path_buf()),
            line: None,
            column: None,
            field: None,
            message: format!("{what}: {e}"),
        };
        let bytes = fs::read(path).map_err(|e| fail("Datei nicht lesbar", e))?;
        let text = String::from_utf8(bytes).map_err(|e| ConfigLoadError {
            file: Some(path.to_path_buf()),
            line: None,
            column: None,
            field: None,
            message: format!("keine gültige UTF-8-Datei: {e}"),
        })?;
        let (migrated, mut report) = migrate_toml(&text).map_err(|mut e| {
            e.file = Some(path.to_path_buf());
            e
        })?;
        if report.from_version >= CONFIG_SCHEMA_VERSION {
            return Ok(None);
        }
        let backup = backup_path(path, report.from_version);
        fs::copy(path, &backup).map_err(|e| fail("Sicherung nicht anlegbar", e))?;
        let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
        tmp_name.push(format!(".tmp{}", std::process::id()));
        let tmp = path.with_file_name(tmp_name);
        fs::write(&tmp, migrated.as_bytes()).map_err(|e| fail("nicht schreibbar", e))?;
        fs::rename(&tmp, path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            fail("nicht ersetzbar", e)
        })?;
        report.backup = Some(backup);
        Ok(Some(report))
    }
}

/// `<datei>.bak-v<n>`, bei Namenskollision `.bak-v<n>-2`, `-3`, …
fn backup_path(path: &Path, version: u32) -> PathBuf {
    let base = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut candidate = path.with_file_name(format!("{base}.bak-v{version}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = path.with_file_name(format!("{base}.bak-v{version}-{n}"));
        n += 1;
    }
    candidate
}
