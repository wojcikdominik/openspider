//! `app.info`: Version, Plattform und wirksame Pfade (OUT-227, OUT-222, UI-127).

use std::path::{Path, PathBuf};

use os_model::settings::AppSettings;

use crate::dto::{AppInfo, AppPaths};

use super::AppConfig;

/// Unterordner des Exportordners unter „Dokumente“ (OUT-222).
pub const DEFAULT_EXPORT_SUBDIR: [&str; 2] = ["OpenSpider", "Exports"];

/// Wirksamer Exportordner: `export.directory`, sonst `<Dokumente>/OpenSpider/Exports`. `<Dokumente>` ist der vom
/// Betriebssystem gemeldete Dokumente-Ordner (`documents`; unter Windows der bekannte Ordner auch bei Umleitung),
/// ersatzweise `~/Documents`. Der Ordner wird hier nicht angelegt, sondern beim ersten Export.
pub fn effective_export_dir(settings: &AppSettings, documents: Option<&Path>) -> PathBuf {
    if let Some(dir) = settings
        .export
        .directory
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        return PathBuf::from(dir);
    }
    let base = documents
        .map(Path::to_path_buf)
        .or_else(dirs::document_dir)
        .or_else(|| dirs::home_dir().map(|h| h.join("Documents")))
        .unwrap_or_else(|| PathBuf::from("Documents"));
    DEFAULT_EXPORT_SUBDIR
        .iter()
        .fold(base, |acc, part| acc.join(part))
}

/// Der Ordner der Crawls: `storage.crawls_dir`, sonst der Standardort aus OUT-227.
pub fn effective_crawls_dir(settings: &AppSettings, config: &AppConfig) -> PathBuf {
    settings
        .storage
        .crawls_dir
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map_or_else(|| config.paths.crawls.clone(), PathBuf::from)
}

/// Build-Kennung: `OPENSPIDER_BUILD` zur Bauzeit (Commit, Datum), sonst `dev`.
pub fn build_id() -> &'static str {
    option_env!("OPENSPIDER_BUILD").unwrap_or("dev")
}

/// Baut die Antwort von `app.info`.
pub fn app_info(config: &AppConfig, settings: &AppSettings) -> AppInfo {
    let s = |p: &Path| p.display().to_string();
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build: build_id().to_string(),
        build_number: os_model::build_number::BUILD_NUMBER.map(str::to_owned),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        paths: AppPaths {
            settings: s(&config.paths.settings),
            crawls: s(&effective_crawls_dir(settings, config)),
            logs: s(&config.paths.logs),
            cache: s(&config.paths.cache),
            exports: s(&effective_export_dir(
                settings,
                config.documents_dir.as_deref(),
            )),
            home_override: config.home_override.clone(),
        },
    }
}
