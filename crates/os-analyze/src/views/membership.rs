//! Common live memberships, shared by tab sessions without retaining cells (UI-141, UI-147).
//!
//! One pass over `urls` prepares the Internal, External and Response Codes populations, the simple
//! content-class filters of Internal/External and the ONPAGE-015 denominator. Large crawls split the
//! pass by ID range over a few additional read connections of the same data version. A finished pass
//! is a derivable cache (K-1a): a writer may persist it in `view_cache`, so reopening a saved crawl
//! reads a few bitmaps instead of the URL table. Read-only hosts keep it in memory only.
use super::{build, session::BuildControl};
use crate::error::ViewError;
use os_store::{queries, view_cache, CrawlFile, OpenMode, StoreError};
use roaring::{RoaringBitmap, RoaringTreemap};
use rusqlite::OptionalExtension;
use std::{
    collections::HashMap,
    io::{Cursor, Read},
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};

/// Populations prepared by the shared pass: External, Internal, Response Codes.
const POPULATIONS: [&str; 3] = [
    "(u.is_internal = 0 AND u.in_tables = 1)",
    "(u.is_internal = 1 AND u.in_tables = 1)",
    "(u.in_tables = 1 AND u.status_code IS NOT NULL)",
];
const FACTS: &str = "SELECT id, is_internal, in_tables, content_class, status_code FROM urls";
const CACHE_KEY: &str = "n21:membership";

/// Immutable facts from one data version. Views own their lifetime; the registry
/// holds only weak references, so idle/budget release also releases these facts.
pub(super) struct Membership {
    seq: u64,
    stamp: String,
    html_ids: RoaringTreemap,
    all: [RoaringBitmap; 3],
    classes: [HashMap<String, RoaringBitmap>; 2],
    pub html_2xx: i64,
    persisted: AtomicBool,
    /// `PRAGMA data_version` of the file's watcher when these facts were read (see [`changed`]).
    baseline: AtomicI64,
    /// The watcher `baseline` belongs to. The facts keep it alive: the watcher's connection is a handle on the
    /// crawl file and lives exactly as long as facts that compare against it.
    watcher: Option<Arc<Watcher>>,
}

impl Membership {
    fn empty(seq: u64, stamp: &str) -> Self {
        Self {
            seq,
            stamp: stamp.to_owned(),
            html_ids: RoaringTreemap::new(),
            all: Default::default(),
            classes: Default::default(),
            html_2xx: 0,
            persisted: AtomicBool::new(false),
            baseline: AtomicI64::new(i64::MIN),
            watcher: None,
        }
    }

    /// A repaired copy for a newer data version; it has not been persisted yet.
    fn derived(&self, seq: u64) -> Self {
        Self {
            seq,
            stamp: self.stamp.clone(),
            html_ids: self.html_ids.clone(),
            all: self.all.clone(),
            classes: self.classes.clone(),
            html_2xx: self.html_2xx,
            persisted: AtomicBool::new(false),
            baseline: AtomicI64::new(i64::MIN),
            watcher: None,
        }
    }

    pub fn bytes(&self) -> u64 {
        self.html_ids.serialized_size() as u64
            + self
                .all
                .iter()
                .chain(self.classes.iter().flat_map(|c| c.values()))
                .map(|b| b.serialized_size() as u64)
                .sum::<u64>()
    }

    pub fn bitmap(&self, built: &build::Built<'_>) -> Option<RoaringBitmap> {
        let scope = scope(built)?;
        if built.filter.predicate == "1" {
            Some(self.all[scope].clone())
        } else {
            Some(
                self.classes[scope]
                    .get(class(built.filter.predicate)?)
                    .cloned()
                    .unwrap_or_default(),
            )
        }
    }

    fn insert(&mut self, row: &rusqlite::Row<'_>) -> Result<(), ViewError> {
        let raw_id: i64 = row.get(0)?;
        let internal: i64 = row.get(1)?;
        let visible: i64 = row.get(2)?;
        let content: Option<&str> = row.get_ref(3)?.as_str_or_null().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?;
        let status: Option<i64> = row.get(4)?;
        // ONPAGE-015 denominator: internal HTML 2xx, including hidden pages.
        if internal == 1
            && content == Some("HTML")
            && status.is_some_and(|s| (200..=299).contains(&s))
            && self.html_ids.insert(raw_id as u64)
        {
            self.html_2xx += 1;
        }
        if visible != 1 {
            return Ok(());
        }
        let id = super::session::url_index(raw_id)?;
        if status.is_some() {
            self.all[2].insert(id);
        }
        if !(0..=1).contains(&internal) {
            return Ok(());
        }
        let scope = internal as usize;
        self.all[scope].insert(id);
        if let Some(content) = content {
            if let Some(bitmap) = self.classes[scope].get_mut(content) {
                bitmap.insert(id);
            } else {
                let mut bitmap = RoaringBitmap::new();
                bitmap.insert(id);
                self.classes[scope].insert(content.to_owned(), bitmap);
            }
        }
        Ok(())
    }

    /// Disjoint ID ranges of the same data version.
    fn merge(&mut self, other: Membership) {
        self.html_ids |= other.html_ids;
        self.html_2xx += other.html_2xx;
        for (mine, theirs) in self.all.iter_mut().zip(other.all) {
            *mine |= theirs;
        }
        for (mine, theirs) in self.classes.iter_mut().zip(other.classes) {
            for (class, bitmap) in theirs {
                *mine.entry(class).or_default() |= bitmap;
            }
        }
    }

    fn encode(&self) -> Result<Vec<u8>, ViewError> {
        let fail = |_| invalid("Mitgliedschaften nicht serialisierbar");
        let mut out = b"OSM1".to_vec();
        self.html_ids.serialize_into(&mut out).map_err(fail)?;
        for bitmap in &self.all {
            bitmap.serialize_into(&mut out).map_err(fail)?;
        }
        for classes in &self.classes {
            let mut names: Vec<_> = classes.keys().collect();
            names.sort();
            out.extend_from_slice(&(names.len() as u32).to_le_bytes());
            for name in names {
                out.extend_from_slice(&(name.len() as u32).to_le_bytes());
                out.extend_from_slice(name.as_bytes());
                classes[name].serialize_into(&mut out).map_err(fail)?;
            }
        }
        Ok(out)
    }

    /// `None` for any foreign or truncated blob: the cache is then rebuilt.
    fn decode(data: &[u8], seq: u64, stamp: &str) -> Option<Self> {
        fn u32_le(r: &mut Cursor<&[u8]>) -> Option<u32> {
            let mut b = [0; 4];
            r.read_exact(&mut b).ok()?;
            Some(u32::from_le_bytes(b))
        }
        let mut r = Cursor::new(data.strip_prefix(b"OSM1")?);
        let mut facts = Self::empty(seq, stamp);
        facts.html_ids = RoaringTreemap::deserialize_from(&mut r).ok()?;
        facts.html_2xx = i64::try_from(facts.html_ids.len()).ok()?;
        for bitmap in &mut facts.all {
            *bitmap = RoaringBitmap::deserialize_from(&mut r).ok()?;
        }
        for classes in &mut facts.classes {
            for _ in 0..u32_le(&mut r)? {
                let mut name = vec![0; u32_le(&mut r)? as usize];
                r.read_exact(&mut name).ok()?;
                let bitmap = RoaringBitmap::deserialize_from(&mut r).ok()?;
                classes.insert(String::from_utf8(name).ok()?, bitmap);
            }
        }
        (r.position() as usize == r.get_ref().len()).then_some(facts)
    }
}

fn invalid(message: &str) -> ViewError {
    StoreError::InvalidInput(message.into()).into()
}

fn class(predicate: &str) -> Option<&str> {
    predicate
        .strip_prefix("u.content_class = '")?
        .strip_suffix('\'')
        .filter(|s| !s.contains('\''))
}

fn scope(built: &build::Built<'_>) -> Option<usize> {
    if !built.filter.requires.is_empty()
        || built.filter.ignore_non_indexable
        || !matches!(built.filter.kind, crate::defs::FilterKind::Live)
        || (built.filter.predicate != "1" && class(built.filter.predicate).is_none())
    {
        return None;
    }
    let scope = POPULATIONS
        .iter()
        .position(|p| *p == built.plan.population_sql)?;
    // Response Codes filters are status rules, not content classes.
    (scope < 2 || built.filter.predicate == "1").then_some(scope)
}

pub(super) fn applicable(built: &build::Built<'_>) -> bool {
    scope(built).is_some()
}

type Registry = Mutex<HashMap<String, Weak<Membership>>>;
fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}
fn cache_key(file: &CrawlFile, seq: u64, stamp: &str) -> Result<String, ViewError> {
    Ok(format!(
        "{:?}:{}:{seq}:{stamp}",
        file.path(),
        file.get_meta("crawl_id")?.unwrap_or_default()
    ))
}
fn params_hash() -> String {
    crate::cursor::query_hash(&[
        "membership-v1",
        env!("CARGO_PKG_VERSION"),
        FACTS,
        &POPULATIONS.join("|"),
    ])
}
fn remember(key: String, facts: &Arc<Membership>) {
    let mut entries = registry().lock().unwrap_or_else(|e| e.into_inner());
    entries.retain(|_, v| v.strong_count() > 0);
    entries.insert(key, Arc::downgrade(facts));
}

pub(super) fn load(
    file: &CrawlFile,
    seq: u64,
    stamp: &str,
    control: &BuildControl,
) -> Result<Arc<Membership>, ViewError> {
    let key = cache_key(file, seq, stamp)?;
    // Read before the facts: a commit during the pass is detected by the next lookup.
    let watch = watcher(file.path());
    let version = watch.as_ref().and_then(|w| w.version());
    let shared = {
        let mut entries = registry().lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, v| v.strong_count() > 0);
        entries.get(&key).and_then(Weak::upgrade)
    };
    // A commit without a new change_seq (another connection writing directly) makes
    // the shared and the stored pass of this sequence unusable.
    let mut bypassed = false;
    if let Some(facts) = shared {
        if version.is_some_and(|v| facts.baseline.load(Ordering::Relaxed) == v) {
            return Ok(facts);
        }
        bypassed = true;
    }
    // K-1a: a persisted pass of exactly this data version replaces the scan.
    let stored = if bypassed {
        None
    } else {
        view_cache::load(file, CACHE_KEY, &params_hash())?
            .filter(|e| e.built_seq == seq && e.step_stamp == stamp)
            .and_then(|e| Membership::decode(&e.data, seq, stamp))
    };
    let mut facts = match stored {
        Some(facts) => {
            facts.persisted.store(true, Ordering::Relaxed);
            facts
        }
        None => scan(file, seq, stamp, control)?,
    };
    facts
        .baseline
        .store(version.unwrap_or(i64::MIN), Ordering::Relaxed);
    facts.watcher = watch;
    let facts = Arc::new(facts);
    remember(key, &facts);
    Ok(facts)
}

/// A dedicated read-only connection per file whose `PRAGMA data_version` changes with every commit of any other
/// connection. Its values only compare with each other, so facts keep the watcher they took their baseline from;
/// the registry holds it weakly. Once no facts of the file are left (views released, crawl cleared or closed), the
/// connection closes and no handle on the file remains (UI-003: a cleared crawl stays deletable, OUT-228).
pub(super) struct Watcher(Mutex<rusqlite::Connection>);

impl Watcher {
    /// `None` if the version cannot be read (no sharing then).
    fn version(&self) -> Option<i64> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .query_row("PRAGMA data_version", [], |r| r.get(0))
            .ok()
    }
}

type Watchers = Mutex<HashMap<std::path::PathBuf, Weak<Watcher>>>;
fn watchers() -> &'static Watchers {
    static WATCHERS: OnceLock<Watchers> = OnceLock::new();
    WATCHERS.get_or_init(Default::default)
}

/// The file's live watcher, opened if there is none. `None` if the file cannot be watched (no sharing then).
fn watcher(path: &std::path::Path) -> Option<Arc<Watcher>> {
    let mut watchers = watchers().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(live) = watchers.get(path).and_then(Weak::upgrade) {
        return Some(live);
    }
    watchers.retain(|_, w| w.strong_count() > 0);
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let live = Arc::new(Watcher(Mutex::new(conn)));
    watchers.insert(path.to_path_buf(), Arc::downgrade(&live));
    Some(live)
}

/// Our own derivable-cache writes (`view_cache`) do not change the data: shared passes of the
/// file stay valid. Call right after such a write. Without live facts there is nothing to keep valid,
/// so no watcher is opened.
pub(crate) fn note_cache_write(path: &std::path::Path) {
    let prefix = format!("{path:?}:");
    let live: Vec<_> = registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(key, _)| key.starts_with(&prefix))
        .filter_map(|(_, v)| v.upgrade())
        .collect();
    for facts in live {
        if let Some(version) = facts.watcher.as_ref().and_then(|w| w.version()) {
            facts.baseline.store(version, Ordering::Relaxed);
        }
    }
}

fn scan(
    file: &CrawlFile,
    seq: u64,
    stamp: &str,
    control: &BuildControl,
) -> Result<Membership, ViewError> {
    let max_id: i64 = file
        .conn()
        .query_row("SELECT COALESCE(MAX(id), 0) FROM urls", [], |r| r.get(0))?;
    // The user waits for this pass: all cores, at most four connections.
    let workers = super::parallel_workers(max_id, 1);
    if workers > 1 {
        if let Some(facts) = parallel_scan(file, seq, stamp, max_id, workers, control)? {
            super::parallel_pass_done();
            return Ok(facts);
        }
    }
    let mut facts = Membership::empty(seq, stamp);
    scan_range(file.conn(), i64::MIN, i64::MAX, &mut facts, control)?;
    Ok(facts)
}

fn scan_range(
    conn: &rusqlite::Connection,
    after: i64,
    last: i64,
    facts: &mut Membership,
    control: &BuildControl,
) -> Result<(), ViewError> {
    let mut stmt = conn.prepare(&format!("{FACTS} WHERE id > ?1 AND id <= ?2"))?;
    let mut rows = stmt.query([after, last])?;
    let mut seen = 0u32;
    while let Some(row) = rows.next()? {
        seen = seen.wrapping_add(1);
        if seen.is_multiple_of(1024) {
            control.check()?;
        }
        facts.insert(row)?;
    }
    control.check()
}

/// Each worker reads on its own connection and verifies the data version first. Any
/// difference (a concurrent writer batch) falls back to the caller's snapshot.
fn parallel_scan(
    file: &CrawlFile,
    seq: u64,
    stamp: &str,
    max_id: i64,
    workers: usize,
    control: &BuildControl,
) -> Result<Option<Membership>, ViewError> {
    let path = file.path();
    let step = max_id / workers as i64 + 1;
    let parts: Vec<Result<Option<Membership>, ViewError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|i| {
                let after = if i == 0 { i64::MIN } else { i as i64 * step };
                let last = if i + 1 == workers {
                    i64::MAX
                } else {
                    (i as i64 + 1) * step
                };
                scope.spawn(move || -> Result<Option<Membership>, ViewError> {
                    let worker = CrawlFile::open(path, OpenMode::ReadOnly)?;
                    let _snapshot = worker.conn().unchecked_transaction()?;
                    if queries::change_seq(worker.conn())? != seq
                        || super::session::stamp(&worker)? != stamp
                    {
                        return Ok(None);
                    }
                    let mut facts = Membership::empty(seq, stamp);
                    scan_range(worker.conn(), after, last, &mut facts, control)?;
                    Ok(Some(facts))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(invalid("Mitgliedschaftsdurchlauf abgebrochen")))
            })
            .collect()
    });
    let mut merged = Membership::empty(seq, stamp);
    for part in parts {
        match part? {
            Some(part) => merged.merge(part),
            None => return Ok(None),
        }
    }
    Ok(Some(merged))
}

/// Persists the shared facts of the file's current data version, if a view prepared them and
/// they are not stored yet. Read-only and running crawls store nothing (`view_cache::save`).
pub(super) fn persist(file: &CrawlFile) -> Result<bool, ViewError> {
    let seq = queries::change_seq(file.conn())?;
    let stamp = super::session::stamp(file)?;
    let key = cache_key(file, seq, &stamp)?;
    let facts = registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .and_then(Weak::upgrade);
    match facts {
        Some(facts) => save(file, &facts),
        None => Ok(false),
    }
}

fn save(file: &CrawlFile, facts: &Membership) -> Result<bool, ViewError> {
    if facts.persisted.load(Ordering::Relaxed) || file.conn().is_readonly("main")? {
        return Ok(false);
    }
    let saved = view_cache::save(
        file,
        CACHE_KEY,
        &params_hash(),
        &view_cache::Entry {
            step_stamp: facts.stamp.clone(),
            built_seq: facts.seq,
            data: facts.encode()?,
        },
    )?;
    if saved {
        facts.persisted.store(true, Ordering::Relaxed);
        note_cache_write(file.path());
    }
    Ok(saved)
}

/// The feed names every changed/deleted ID. Repair common facts before the next
/// materialization, preserving the incremental path under live crawl updates.
pub(super) fn repair(
    file: &CrawlFile,
    previous: &Membership,
    ids: &[os_model::UrlId],
    stamp: &str,
) -> Result<Option<Arc<Membership>>, ViewError> {
    let seq = queries::change_seq(file.conn())?;
    if previous.stamp != stamp || previous.seq > seq {
        return Ok(None);
    }
    let mut facts = previous.derived(seq);
    facts.watcher = watcher(file.path());
    facts.baseline.store(
        facts
            .watcher
            .as_ref()
            .and_then(|w| w.version())
            .unwrap_or(i64::MIN),
        Ordering::Relaxed,
    );
    let mut stmt = file.conn().prepare(&format!("{FACTS} WHERE id=?1"))?;
    for id in ids {
        if facts.html_ids.remove(id.0 as u64) {
            facts.html_2xx -= 1;
        }
        if let Ok(id) = u32::try_from(id.0) {
            for bitmap in facts
                .all
                .iter_mut()
                .chain(facts.classes.iter_mut().flat_map(|c| c.values_mut()))
            {
                bitmap.remove(id);
            }
        }
        // query_row's closure preserves SQL errors; insertion can also reject a
        // visible ID outside the bitmap range, so keep its Result nested.
        if let Some(inserted) = stmt
            .query_row([id.0], |row| Ok(facts.insert(row)))
            .optional()?
        {
            inserted?;
        }
    }
    let facts = Arc::new(facts);
    remember(cache_key(file, seq, stamp)?, &facts);
    Ok(Some(facts))
}

pub(super) fn pending(facts: &Membership) -> bool {
    !facts.persisted.load(Ordering::Relaxed)
}
