//! Abbildung der Kernfehler auf `CmdError` (`docs/plan/M2-ui-concept.md` 4.2) und Hilfen für Handler.
//!
//! `StoreError` mit Dateifehlern wird `Store`, ein SQLite-Interrupt wird `Cancelled`, Sperren der Datenbank werden
//! `Busy` (wiederholbar). Argumente von Commands werden mit [`parse_args`] gelesen: ein Fehler ist immer
//! `InvalidInput` mit der Ursache in `detail`.

use os_store::StoreError;
use serde::de::DeserializeOwned;

use crate::dto::{CmdError, ErrorCode};

/// SQLite-Fehler → `CmdError` (Interrupt = `Cancelled`, `SQLITE_BUSY`/`LOCKED` = `Busy`, sonst `Store`).
pub fn rusqlite_error(e: &rusqlite::Error) -> CmdError {
    if let rusqlite::Error::SqliteFailure(f, _) = e {
        match f.code {
            rusqlite::ErrorCode::OperationInterrupted => {
                return CmdError::new(ErrorCode::Cancelled, "err-cancelled");
            }
            rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                return CmdError::new(ErrorCode::Busy, "err-db-busy")
                    .with_detail(e.to_string())
                    .retryable();
            }
            rusqlite::ErrorCode::ReadOnly | rusqlite::ErrorCode::PermissionDenied => {
                return CmdError::new(ErrorCode::Permission, "err-db-read-only")
                    .with_detail(e.to_string());
            }
            _ => {}
        }
    }
    CmdError::new(ErrorCode::Store, "err-store").with_detail(e.to_string())
}

/// `StoreError` → `CmdError` (M2-ui-concept 4.2, „Abbildung aus M1-Fehlern“).
pub fn store_error(e: &StoreError) -> CmdError {
    match e {
        StoreError::Sqlite(inner) => rusqlite_error(inner),
        StoreError::Io { path, source } => {
            let code = if source.kind() == std::io::ErrorKind::PermissionDenied {
                ErrorCode::Permission
            } else {
                ErrorCode::Io
            };
            CmdError::new(code, "err-io")
                .with_arg("path", path.display().to_string())
                .with_detail(source.to_string())
        }
        StoreError::NotFound(path) => CmdError::new(ErrorCode::NotFound, "err-file-not-found")
            .with_arg("path", path.display().to_string()),
        StoreError::AlreadyExists(path) => {
            CmdError::new(ErrorCode::InvalidInput, "err-file-exists")
                .with_arg("path", path.display().to_string())
        }
        StoreError::NotACrawlFile { path } => {
            CmdError::new(ErrorCode::Store, "err-not-a-crawl-file")
                .with_arg("path", path.display().to_string())
        }
        StoreError::Corrupt { path, detail } => {
            CmdError::new(ErrorCode::Store, "err-crawl-corrupt")
                .with_arg("path", path.display().to_string())
                .with_detail(detail.clone())
        }
        StoreError::NewerVersion {
            path,
            found,
            supported,
            generator,
        } => CmdError::new(ErrorCode::Store, "err-crawl-newer-version")
            .with_arg("path", path.display().to_string())
            .with_arg("found", *found)
            .with_arg("supported", *supported)
            .with_arg("generator", generator.clone()),
        StoreError::MigrationRequired {
            path,
            found,
            supported,
        } => CmdError::new(ErrorCode::Store, "err-crawl-migration-required")
            .with_arg("path", path.display().to_string())
            .with_arg("found", *found)
            .with_arg("supported", *supported),
        StoreError::InvalidInput(detail) => {
            CmdError::new(ErrorCode::InvalidInput, "err-invalid-input").with_detail(detail.clone())
        }
        StoreError::NotImplemented(what) => CmdError::unsupported(what, "M2"),
        // 08 AI-020 [M4]: ein anderer Prozess führt den Crawl (M4-K11).
        StoreError::Occupied { kind, pid } => CmdError::new(ErrorCode::Busy, "err-crawl-running")
            .with_arg("kind", kind.clone())
            .with_arg("pid", pid.clone()),
        StoreError::Migration(_)
        | StoreError::MissingMeta(_)
        | StoreError::InvalidMeta { .. }
        | StoreError::MissingConfig
        | StoreError::Model(_) => {
            CmdError::new(ErrorCode::Store, "err-store").with_detail(e.to_string())
        }
    }
}

/// Fehler für „kein Crawl unter diesem Schlüssel geöffnet“.
pub fn no_crawl(key: &str) -> CmdError {
    CmdError::new(ErrorCode::NoCrawl, "err-no-crawl").with_arg("crawl", key)
}

/// Fehler für einen abgefangenen Panic (Handler, Pool-Auftrag, Operation).
pub fn internal(what: &str, detail: impl Into<String>) -> CmdError {
    CmdError::new(ErrorCode::Internal, "err-internal")
        .with_arg("where", what)
        .with_detail(detail)
}

/// Text eines Panic-Werts (`&str` oder `String`).
pub fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic".to_string()
    }
}

/// Liest die Argumente eines Commands. `null` zählt für Typen, die das zulassen (z. B. `()`), als leer.
pub fn parse_args<T: DeserializeOwned>(args: serde_json::Value) -> Result<T, CmdError> {
    serde_json::from_value(args).map_err(|e| {
        CmdError::new(ErrorCode::InvalidInput, "err-invalid-args").with_detail(e.to_string())
    })
}

/// Ergebnis eines Handlers als JSON-Wert.
pub fn to_json<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, CmdError> {
    serde_json::to_value(value)
        .map_err(|e| CmdError::new(ErrorCode::Internal, "err-internal").with_detail(e.to_string()))
}
