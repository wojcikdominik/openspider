//! Gemeinsame Hilfen der unabhängigen Abnahme M4-K11 (aktiver Besitzer außerhalb von `Running`).
//!
//! Grundlage ausschließlich: 01 CRAWL-125, Nachtrag 26.11.2026 (Klärung K4-22) und 08 AI-020, Nachtrag 26.11.2026,
//! Unterpunkt „Aktiver Besitzer außerhalb von `Running`“. Die Hilfen hängen nur an `os_store`, `os_model`,
//! `serde_json` und `std`, damit jede Crate sie per `#[path]` einbinden kann (os-ai hat kein `tempfile`).
//!
//! * [`V10Case`]: Crawl-Dateien in allen Zuständen, wie 1.0 sie schreibt (Fixture
//!   `tests/fixtures/crawl-files/k11-acceptance/v1_0_states.json`), samt einem eigenen Orakel „CRAWL-125 allein“.
//! * [`new_crawl`], [`set_owner`], [`meta_dump`]: Crawl-Dateien anlegen und den Heartbeat simulieren (Metadaten
//!   setzen, keine Wanduhr).
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use os_model::{CrawlConfig, CrawlId, CrawlMode, CrawlState, IdSource, SeededIds, Timestamp};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};
use serde_json::Value;

/// Grenze aus CRAWL-125 („aktuell“ bis einschließlich 10 000 ms).
pub const LIMIT_MS: i64 = 10_000;

/// Eine PID, die nie die des Testprozesses ist (u32::MAX − 1).
pub const FOREIGN_PID: &str = "4294967294";

/// Eigene PID als Text.
pub fn own_pid() -> String {
    std::process::id().to_string()
}

/// Fester Bezugszeitpunkt der Tests.
pub fn base_now() -> Timestamp {
    Timestamp::from_db("2026-11-26T12:00:00.000Z").unwrap()
}

/// `t` minus `age_ms` (negativ = Zukunft).
pub fn ago(t: Timestamp, age_ms: i64) -> Timestamp {
    Timestamp::from_unix_millis(t.unix_millis() - age_ms)
}

/// Temporäres Verzeichnis ohne zusätzliche Abhängigkeit; wird beim Drop gelöscht.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "os-k11-acc-{tag}-{}-{nanos}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Crawl-ID aus einem Seed.
pub fn crawl_id(seed: u64) -> CrawlId {
    SeededIds::new(seed).new_crawl_id(base_now())
}

/// Legt `<dir>/<id>.ospider` an (Dateiname wie in der Crawl-Verwaltung) und setzt den Zustand.
pub fn new_crawl(dir: &Path, seed: u64, state: CrawlState) -> (CrawlId, PathBuf) {
    let id = crawl_id(seed);
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(format!("{id}.ospider"));
    let init = CrawlInit {
        crawl_id: id,
        mode: CrawlMode::Spider,
        start_url: Some("https://k11.test/".into()),
        list_source: None,
        project: String::new(),
        name: format!("K11 Abnahme {seed}"),
        config: CrawlConfig::default(),
        created_at: ago(base_now(), 3_600_000),
    };
    let file = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    file.set_meta("state", state.as_str()).unwrap();
    file.set_meta("started_at", &ago(base_now(), 3_000_000).to_db())
        .unwrap();
    if matches!(state, CrawlState::Completed | CrawlState::Failed) {
        file.set_meta("ended_at", &ago(base_now(), 2_000_000).to_db())
            .unwrap();
        if state == CrawlState::Completed {
            file.set_meta("end_reason", "queue_empty").unwrap();
        }
    }
    if state == CrawlState::Paused {
        file.set_meta("paused_at", &ago(base_now(), 2_000_000).to_db())
            .unwrap();
    }
    // Geordnetes Ende eines 1.0-Laufs: Heartbeat = Ende, `owner_pid` leer, `owner_kind` nennt den letzten Besitzer.
    file.set_meta("heartbeat_at", &ago(base_now(), 2_000_000).to_db())
        .unwrap();
    file.set_meta("owner_kind", "cli").unwrap();
    file.set_meta("owner_pid", "").unwrap();
    drop(file);
    (id, path)
}

/// Simulierter Besitzer: `owner_kind`, `owner_pid` und `heartbeat_at` (keine laufende Aufgabe).
pub fn set_owner(path: &Path, kind: &str, pid: &str, heartbeat: Option<Timestamp>) {
    let f = CrawlFile::open(path, OpenMode::ReadWrite).unwrap();
    f.set_meta("owner_kind", kind).unwrap();
    f.set_meta("owner_pid", pid).unwrap();
    match heartbeat {
        Some(t) => f.set_meta("heartbeat_at", &t.to_db()).unwrap(),
        None => delete_meta(&f, "heartbeat_at"),
    }
}

/// Setzt einen Metadatenschlüssel.
pub fn set(path: &Path, key: &str, value: &str) {
    CrawlFile::open(path, OpenMode::ReadWrite)
        .unwrap()
        .set_meta(key, value)
        .unwrap();
}

/// Entfernt einen Metadatenschlüssel.
pub fn delete_meta(f: &CrawlFile, key: &str) {
    f.conn()
        .execute("DELETE FROM crawl_meta WHERE key = ?1", [key])
        .unwrap();
}

/// Liest einen Schlüssel.
pub fn get(path: &Path, key: &str) -> Option<String> {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .get_meta(key)
        .unwrap()
}

/// Alle Schlüssel von `crawl_meta` (zum Vergleich „nichts geschrieben“).
#[allow(clippy::let_and_return)]
pub fn meta_dump(path: &Path) -> BTreeMap<String, String> {
    let f = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    let mut stmt = f
        .conn()
        .prepare("SELECT key, value FROM crawl_meta ORDER BY key")
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .collect::<Result<BTreeMap<_, _>, _>>()
        .unwrap();
    rows
}

/// Anzahl der Zeilen einer Tabelle.
pub fn count(path: &Path, table: &str) -> i64 {
    let f = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    f.conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// Unterschiedliche Schlüssel zweier Abzüge.
pub fn changed_keys(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Vec<String> {
    let mut keys: Vec<String> = a
        .keys()
        .chain(b.keys())
        .filter(|k| a.get(*k) != b.get(*k))
        .cloned()
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

// ---------------------------------------------------------------------------------------------------------------
// Dateien aus 1.0 (Fixture) und Orakel „CRAWL-125 allein“
// ---------------------------------------------------------------------------------------------------------------

/// Pfad der Fixture.
pub fn v10_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/crawl-files/k11-acceptance/v1_0_states.json")
}

/// `owner_pid` eines Falls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pid {
    /// Schlüssel fehlt.
    Absent,
    /// Wert (leer, eigene PID oder fremd).
    Value(String),
}

/// Ein Fall der Fixture.
#[derive(Debug, Clone)]
pub struct V10Case {
    pub name: String,
    pub state: CrawlState,
    pub end_reason: Option<String>,
    pub heartbeat_age_ms: Option<i64>,
    pub started_age_ms: Option<i64>,
    pub created_age_ms: Option<i64>,
    pub owner_pid: Pid,
    pub owner_kind: Option<String>,
    pub queued: u32,
    pub expect_state: CrawlState,
    pub expect_resumable: bool,
}

fn state_of(s: &str) -> CrawlState {
    s.parse()
        .unwrap_or_else(|_| panic!("unbekannter Zustand {s}"))
}

fn opt_i64(v: &Value, key: &str) -> Option<i64> {
    match &v[key] {
        Value::Null => None,
        x => Some(x.as_i64().unwrap_or_else(|| panic!("{key}: {x}"))),
    }
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v[key].as_str().map(str::to_string)
}

/// Liest die Fixture (Bezugszeitpunkt, Fälle).
pub fn v10_cases() -> (Timestamp, Vec<V10Case>) {
    let text = std::fs::read_to_string(v10_fixture_path()).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    let now = Timestamp::from_db(v["now"].as_str().unwrap()).unwrap();
    let cases = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| V10Case {
            name: c["name"].as_str().unwrap().to_string(),
            state: state_of(c["state"].as_str().unwrap()),
            end_reason: opt_str(c, "end_reason"),
            heartbeat_age_ms: opt_i64(c, "heartbeat_age_ms"),
            started_age_ms: opt_i64(c, "started_age_ms"),
            created_age_ms: opt_i64(c, "created_age_ms"),
            owner_pid: match &c["owner_pid"] {
                Value::Null => Pid::Absent,
                Value::String(s) if s == "own" => Pid::Value(own_pid()),
                Value::String(s) => Pid::Value(s.clone()),
                x => panic!("owner_pid: {x}"),
            },
            owner_kind: opt_str(c, "owner_kind"),
            queued: c["queued"].as_u64().unwrap() as u32,
            expect_state: state_of(c["expect"]["state"].as_str().unwrap()),
            expect_resumable: c["expect"]["resumable"].as_bool().unwrap(),
        })
        .collect();
    (now, cases)
}

impl V10Case {
    /// Zustand nach CRAWL-125 allein: nur ein gespeichertes `Running` wird ausgewertet; Bezugszeit `heartbeat_at`,
    /// ersatzweise `started_at`, ersatzweise `created_at`; ≤ 10 000 ms (Zukunft eingeschlossen) = laufend.
    pub fn oracle_state(&self) -> CrawlState {
        if self.state != CrawlState::Running {
            return self.state;
        }
        let age = self
            .heartbeat_age_ms
            .or(self.started_age_ms)
            .or(self.created_age_ms);
        match age {
            Some(a) if a <= LIMIT_MS => CrawlState::Running,
            _ => CrawlState::Incomplete,
        }
    }

    /// Laufend nach CRAWL-125 allein.
    pub fn oracle_running(&self) -> bool {
        self.oracle_state() == CrawlState::Running
    }

    /// Fortsetzbarkeit nach CRAWL-128 (1.0): pausiert, unterbrochen oder abgeschlossen mit Restwarteschlange, nie
    /// `stopped`.
    pub fn oracle_resumable(&self) -> bool {
        if self.end_reason.as_deref() == Some("stopped") {
            return false;
        }
        matches!(
            self.oracle_state(),
            CrawlState::Paused | CrawlState::Incomplete
        ) || (self.state == CrawlState::Completed && self.queued > 0)
    }

    /// `owner_pid` nennt einen fremden Prozess (eine leere oder fehlende Angabe gilt bei laufendem Crawl wie in 1.0
    /// als fremd).
    pub fn foreign(&self) -> bool {
        match &self.owner_pid {
            Pid::Value(p) => *p != own_pid(),
            Pid::Absent => true,
        }
    }

    /// Ob der Fall von der verstreichenden Zeit abhängt (Bezugszeit 1 … 10 000 ms alt: wird mit der Systemuhr
    /// während des Tests veraltet). Ältere Bezugszeiten bleiben veraltet, frische werden in die Zukunft verschoben.
    pub fn near_boundary(&self) -> bool {
        self.state == CrawlState::Running
            && self
                .heartbeat_age_ms
                .or(self.started_age_ms)
                .or(self.created_age_ms)
                .is_some_and(|a| (1..=LIMIT_MS).contains(&a))
    }

    /// Variante für Leser mit Systemuhr: frische Zeitpunkte (Alter ≤ 0) liegen 60 s in der Zukunft (gilt nach
    /// CRAWL-125 ebenso als aktuell), Fälle nahe der Grenze entfallen (`None`).
    pub fn for_system_clock(&self) -> Option<V10Case> {
        if self.near_boundary() {
            return None;
        }
        let shift = |a: Option<i64>| a.map(|a| if a <= 0 { -60_000 } else { a });
        let mut c = self.clone();
        c.heartbeat_age_ms = shift(c.heartbeat_age_ms);
        c.started_age_ms = shift(c.started_age_ms);
        c.created_age_ms = shift(c.created_age_ms);
        Some(c)
    }

    /// Schreibt die Datei `<dir>/<id>.ospider` wie 1.0 relativ zu `now`.
    pub fn build(&self, dir: &Path, seed: u64, now: Timestamp) -> (CrawlId, PathBuf) {
        let (id, path) = new_crawl(dir, seed, self.state);
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        for key in [
            "end_reason",
            "ended_at",
            "paused_at",
            "heartbeat_at",
            "started_at",
            "created_at",
            "owner_pid",
            "owner_kind",
        ] {
            delete_meta(&f, key);
        }
        if let Some(r) = &self.end_reason {
            f.set_meta("end_reason", r).unwrap();
        }
        let ts = |age: Option<i64>| age.map(|a| ago(now, a).to_db());
        if let Some(t) = ts(self.heartbeat_age_ms) {
            f.set_meta("heartbeat_at", &t).unwrap();
        }
        if let Some(t) = ts(self.started_age_ms) {
            f.set_meta("started_at", &t).unwrap();
        }
        if let Some(t) = ts(self.created_age_ms) {
            f.set_meta("created_at", &t).unwrap();
        }
        match self.state {
            CrawlState::Paused => f.set_meta("paused_at", &ago(now, 30_000).to_db()).unwrap(),
            CrawlState::Completed | CrawlState::Failed => {
                f.set_meta("ended_at", &ago(now, 30_000).to_db()).unwrap()
            }
            _ => {}
        }
        if let Pid::Value(p) = &self.owner_pid {
            f.set_meta("owner_pid", p).unwrap();
        }
        if let Some(k) = &self.owner_kind {
            f.set_meta("owner_kind", k).unwrap();
        }
        for i in 0..self.queued {
            f.conn()
                .execute(
                    "INSERT INTO queue(url_id, seq, depth, priority) VALUES (?1, ?1, 0, 0)",
                    [i64::from(i) + 1],
                )
                .unwrap();
        }
        (id, path)
    }
}

/// Prüft die Fixture selbst: der erwartete Wert in der Datei stimmt mit dem eigenen Orakel überein.
pub fn assert_fixture_consistent(cases: &[V10Case]) {
    assert!(cases.len() >= 15, "Fixture unvollständig");
    for c in cases {
        assert_eq!(c.oracle_state(), c.expect_state, "Fixture {}", c.name);
        assert_eq!(
            c.oracle_resumable(),
            c.expect_resumable,
            "Fixture {}",
            c.name
        );
        // 1.0 leert `owner_pid` bei jedem geordneten Ende: gesetzt nur bei gespeichertem `Running`.
        if c.state != CrawlState::Running {
            assert!(
                matches!(&c.owner_pid, Pid::Absent) || c.owner_pid == Pid::Value(String::new()),
                "Fixture {}: 1.0 schreibt owner_pid nur bei Running",
                c.name
            );
        }
    }
}

/// Inhalt der Crawl-Datei und ihres WAL (für „byte-gleich“); nur bei geschlossenen Verbindungen aufrufen.
pub fn file_bytes(path: &Path) -> (Vec<u8>, Option<Vec<u8>>) {
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    (
        std::fs::read(path).unwrap(),
        std::fs::read(PathBuf::from(wal))
            .ok()
            .filter(|w| !w.is_empty()),
    )
}

/// „Stoppen“ eines unterbrochenen Crawls ohne Engine, wie CRAWL-125 es beschreibt: ein Commit mit `state` =
/// `Completed`, Endgrund `stopped`, `ended_at`/`heartbeat_at` = jetzt und leerem `owner_pid`.
pub fn stop_without_engine(path: &Path, at: Timestamp) {
    let f = CrawlFile::open(path, OpenMode::ReadWrite).unwrap();
    f.conn().execute_batch("BEGIN IMMEDIATE").unwrap();
    for (k, v) in [
        ("state", "Completed".to_string()),
        ("end_reason", "stopped".to_string()),
        ("ended_at", at.to_db()),
        ("heartbeat_at", at.to_db()),
        ("owner_pid", String::new()),
    ] {
        f.set_meta(k, &v).unwrap();
    }
    f.conn().execute_batch("COMMIT").unwrap();
}
