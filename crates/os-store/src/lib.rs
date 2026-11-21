//! `os-store` – Crawl-Datei (SQLite, WAL), Schema-Migrationen und Abfragen für UI-Tabellen
//! (Paging, Filter, Sortierung).
//!
//! Kollation `OS_TEXT` (P00c): `collation`.
//!
//! Kern (P00a): `error`, `meta`, `open`, `profile`, `schema`, `sink`. Stubs (P00b): `writer`, `counters`,
//! `files`, `resume`, `registry`, `paths`, `queries` – umgesetzt von M1-P07.
//!
//! Ab M2 (M2-F02): Schema v3 (`schema`, Migration `0003_m2.sql`), Writer für die M2-Teile (`writer_m2`), öffentliche
//! Leseschnittstelle (`read_api`, `q_`-Sichten, OUT-233/234), Abfragemodule `queries/*` und die Stubs `view_cache`,
//! `readonly_sql`, `auth_profiles`.

pub mod ai;
pub mod auth_profiles;
pub(crate) mod checkpointer;
pub mod collation;
pub mod counters;
pub mod error;
pub mod files;
pub mod link_index;
pub mod meta;
pub mod open;
pub mod paths;
pub mod profile;
pub mod queries;
pub mod read_api;
pub mod readonly_sql;
pub mod registry;
pub mod resume;
pub mod schema;
pub mod sink;
pub mod url_index;
pub mod view_cache;
pub mod writer;
mod writer_m2;

pub use error::StoreError;
pub use meta::CrawlMeta;
pub use open::{backup_path, CrawlFile, CrawlInit, OpenMode};
pub use profile::StorageProfile;
pub use resume::{QueueEntry, ResumeState};
pub use schema::{Migration, APPLICATION_ID, MIGRATIONS, SCHEMA_VERSION};
pub use sink::CrawlSink;
pub use writer::SqliteSink;
pub use writer_m2::{decode_minhash, encode_minhash};
