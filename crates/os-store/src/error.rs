//! Fehlertyp von `os-store`.

use std::path::PathBuf;

use os_model::ModelError;

/// Fehler beim Anlegen, Öffnen und Lesen einer Crawl-Datei.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Dateisystemfehler.
    #[error("Dateifehler bei {path}: {source}")]
    Io {
        /// Betroffener Pfad.
        path: PathBuf,
        /// Ursache.
        #[source]
        source: std::io::Error,
    },
    /// SQLite-Fehler.
    #[error("SQLite-Fehler: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Die Datei existiert nicht.
    #[error("Datei nicht gefunden: {}", .0.display())]
    NotFound(PathBuf),
    /// Beim Anlegen existiert die Datei schon.
    #[error("Datei existiert bereits: {}", .0.display())]
    AlreadyExists(PathBuf),
    /// Keine OpenSpider-Crawl-Datei (OUT-230): fremde SQLite-Datei oder gar keine SQLite-Datei.
    #[error("{}: keine OpenSpider-Crawl-Datei", .path.display())]
    NotACrawlFile {
        /// Betroffene Datei.
        path: PathBuf,
    },
    /// Die Datei ist beschädigt (z. B. abgeschnitten).
    #[error("{}: Datei beschädigt ({detail})", .path.display())]
    Corrupt {
        /// Betroffene Datei.
        path: PathBuf,
        /// Ursache.
        detail: String,
    },
    /// Mit einer neueren Schemaversion erstellt (CRAWL-127); die Datei bleibt unverändert.
    #[error(
        "{}: mit {generator} erstellt (Schema {found}, unterstützt bis {supported}) – bitte OpenSpider aktualisieren",
        .path.display()
    )]
    NewerVersion {
        /// Betroffene Datei.
        path: PathBuf,
        /// Version laut Datei.
        found: u32,
        /// Höchste unterstützte Version.
        supported: u32,
        /// Erzeuger laut Datei, falls lesbar (sonst ein allgemeiner Hinweis).
        generator: String,
    },
    /// Die Datei ist älter als das aktuelle Schema und wurde nur lesend geöffnet.
    #[error("{}: Schema {found} muss auf {supported} migriert werden (nicht im Nur-Lese-Modus möglich)", .path.display())]
    MigrationRequired {
        /// Betroffene Datei.
        path: PathBuf,
        /// Version laut Datei.
        found: u32,
        /// Aktuelle Version.
        supported: u32,
    },
    /// Fehlerhafte Migrationsliste (Lücke oder falsche Reihenfolge).
    #[error("ungültige Migration: {0}")]
    Migration(String),
    /// Ein Pflichtschlüssel fehlt in `crawl_meta`.
    #[error("crawl_meta: Schlüssel {0:?} fehlt")]
    MissingMeta(String),
    /// Ein Wert in `crawl_meta` ist unlesbar.
    #[error("crawl_meta: Wert {value:?} für {key:?} ungültig")]
    InvalidMeta {
        /// Schlüssel.
        key: String,
        /// Wert.
        value: String,
    },
    /// In der Datei ist keine Konfiguration gespeichert.
    #[error("keine Konfiguration in der Crawl-Datei")]
    MissingConfig,
    /// Ungültige Eingabe des Aufrufers (z. B. Batch mit unbekannter URL-ID oder doppelter Adresse,
    /// unbekannte Sortierspalte). Die Crawl-Datei bleibt unverändert.
    #[error("ungültige Eingabe: {0}")]
    InvalidInput(String),
    /// Ein anderer Prozess führt den Crawl (laufend nach CRAWL-125 oder aktiver Besitzer, 08 AI-020 [M4]); geliefert
    /// von `owner::claim` bzw. `os_ai::run_owner::RunOwner::start`. Nichts wurde geschrieben.
    #[error("Der Crawl läuft noch (Prozess {pid}, {kind}) – Steuerung nur dort")]
    Occupied {
        /// `owner_kind` des Besitzers (`app`, `cli`, `mcp-stdio`; leer, wenn nicht gesetzt).
        kind: String,
        /// `owner_pid` des Besitzers (Dezimalzahl wie gespeichert; leer, wenn nicht gesetzt).
        pid: String,
    },
    /// Der Crawl ist unterbrochen (gespeichertes `Running` mit veraltetem Heartbeat, CRAWL-125): kein KI-Lauf, bis er
    /// fortgesetzt oder gestoppt ist (08 AI-020 [M4], „Kein KI-Lauf auf einem unterbrochenen Crawl“); geliefert von
    /// `owner::claim` bzw. `os_ai::run_owner::RunOwner::start`. Die Datei bleibt unverändert; die Anzeige ist der
    /// Text der Spec (de), die Meldung in der Oberfläche (de/en) bildet der Aufrufer.
    #[error("Crawl ist unterbrochen – zuerst fortsetzen oder stoppen")]
    Interrupted,
    /// Noch nicht umgesetzt (Stub, `STUB(M1-Pxx)`).
    #[error("nicht implementiert: {0}")]
    NotImplemented(&'static str),
    /// Modellfehler (Enum, Zeitstempel, Konfiguration).
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl StoreError {
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        StoreError::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}
