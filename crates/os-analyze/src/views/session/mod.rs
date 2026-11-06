//! Lazy, versioned view materialization. No crawl-wide cell values are retained.
mod control;
mod permutation;
mod projection;

#[cfg(test)]
mod n21_tests;

use super::{build, exec, ColumnSelection, PageStart, SearchSpec, ViewQuery, URLS_FROM};
use crate::{
    defs::SortKey,
    error::ViewError,
    search::{FilterSource, SearchError},
};
use os_model::{
    time::{Clock, SystemClock},
    CancelToken, CrawlConfig, UrlId, Value,
};
use os_store::{queries, view_cache, CrawlFile, StoreError};
use permutation::Permutation;
use projection::Projection;
use roaring::RoaringBitmap;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, OnceLock, Weak,
    },
    time::Duration,
};

/// Was eine Sitzung zeigt: Tab, Filter, Suche, Sortierung, Spalten und optionale Einschränkungen.
pub struct SessionSpec {
    /// Tab, z. B. `"Internal"`.
    pub tab: String,
    /// Filter, z. B. `"HTML"`.
    pub filter: String,
    /// Suche.
    pub search: Option<SearchSpec>,
    /// Sortierung (leer = Standard des Tabs).
    pub sort: Vec<SortKey>,
    /// Spalten.
    pub columns: ColumnSelection,
    /// Weitere Einschränkungen (Segment, Vergleichsklasse), ODER-frei verknüpft mit UND.
    pub restrict: Vec<std::sync::Arc<dyn FilterSource>>,
}

/// Speicherbudget einer Sitzung bzw. aller Sitzungen (K-1g).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Höchstzahl Bytes für Positionslisten, Umkehrabbildungen und Permutationen (LRU-begrenzt).
    pub max_bytes: u64,
    /// Nach dieser Leerlaufzeit gibt die Sitzung ihren Speicher frei.
    pub idle_release: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            max_bytes: 512 * 1024 * 1024,
            idle_release: Duration::from_secs(60),
        }
    }
}

/// Anfrage eines Zeilenfensters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowsReq {
    /// Monotone Sitzungsgeneration; eine andere Version wird verworfen.
    pub version: u64,
    /// Erste Zeile.
    pub offset: u64,
    /// Zeilenzahl (höchstens 500).
    pub limit: u32,
}

/// Ein Zeilenfenster.
#[derive(Debug, Clone, PartialEq)]
pub struct Rows {
    /// Spaltennamen.
    pub columns: Vec<String>,
    /// Zeilen.
    pub rows: Vec<Vec<Value>>,
    /// Zeilenschlüssel (`UrlId`) in Zeilenreihenfolge.
    pub row_keys: Vec<UrlId>,
    /// Gesamtzahl der Zeilen der Ansicht.
    pub total: u64,
    /// Version der Ansicht.
    pub version: u64,
}

/// Build control shared with the caller's operation; SQLite can also be interrupted from another thread.
#[derive(Clone, Default)]
pub struct BuildControl {
    pub cancel: CancelToken,
}
impl BuildControl {
    pub(super) fn check(&self) -> Result<(), ViewError> {
        self.cancel
            .checkpoint()
            .map_err(|_| SearchError::Cancelled.into())
    }
}

fn invalid(message: &str) -> ViewError {
    StoreError::InvalidInput(message.into()).into()
}
pub(super) fn url_index(id: i64) -> Result<u32, ViewError> {
    u32::try_from(id).map_err(|_| invalid("URL-Kennung außerhalb des Bitmap-Bereichs"))
}

struct FilterBitmap {
    ids: RoaringBitmap,
    seq: u64,
    stamp: String,
}
enum Inverse {
    Dense(Vec<u32>),
    Sparse(HashMap<u32, u32>),
}
impl Inverse {
    fn build(ids: &[u32]) -> Self {
        let len = ids.iter().copied().max().map_or(0, |id| id as usize + 1);
        if len <= ids.len().saturating_mul(4) {
            let mut positions = vec![u32::MAX; len];
            for (pos, id) in ids.iter().enumerate() {
                positions[*id as usize] = pos as u32;
            }
            Self::Dense(positions)
        } else {
            Self::Sparse(
                ids.iter()
                    .enumerate()
                    .map(|(p, id)| (*id, p as u32))
                    .collect(),
            )
        }
    }
    fn locate(&self, id: u32) -> Option<u64> {
        match self {
            Self::Dense(v) => v.get(id as usize).copied().filter(|p| *p != u32::MAX),
            Self::Sparse(v) => v.get(&id).copied(),
        }
        .map(u64::from)
    }
    fn bytes(&self) -> u64 {
        match self {
            Self::Dense(v) => v.capacity() as u64 * 4,
            Self::Sparse(v) => v.capacity() as u64 * 16,
        }
    }
}
struct Materialized {
    built_seq: u64,
    ids: Vec<u32>,
    inverse: Inverse,
    projection: Arc<Projection>,
}
struct Cache {
    membership: Option<Arc<super::membership::Membership>>,
    filters: HashMap<String, FilterBitmap>,
    permutations: HashMap<String, Permutation>,
    projections: HashMap<String, Arc<Projection>>,
    view: Option<Arc<Materialized>>,
    used: Option<Duration>,
    recency: u64,
    clock: Arc<dyn Clock>,
    idle: Duration,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            membership: None,
            filters: HashMap::new(),
            permutations: HashMap::new(),
            projections: HashMap::new(),
            view: None,
            used: None,
            recency: 0,
            idle: Duration::from_secs(60),
            clock: Arc::new(SystemClock),
        }
    }
}
impl Cache {
    fn touch(&mut self) {
        static ACCESS: AtomicU64 = AtomicU64::new(1);
        self.used = Some(self.clock.monotonic());
        self.recency = ACCESS.fetch_add(1, Ordering::Relaxed);
    }
    fn bytes(&self) -> u64 {
        self.membership.as_ref().map_or(0, |m| m.bytes())
            + self
                .filters
                .values()
                .map(|f| f.ids.serialized_size() as u64)
                .sum::<u64>()
            + self
                .permutations
                .values()
                .map(Permutation::bytes)
                .sum::<u64>()
            + self
                .view
                .as_ref()
                .map_or(0, |v| v.ids.capacity() as u64 * 4 + v.inverse.bytes())
    }
    fn release(&mut self) {
        self.membership = None;
        self.filters.clear();
        self.permutations.clear();
        self.projections.clear();
        self.view = None;
    }
}
fn pool() -> &'static Mutex<Vec<Weak<Mutex<Cache>>>> {
    static POOL: OnceLock<Mutex<Vec<Weak<Mutex<Cache>>>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(Vec::new()))
}
/// Releases idle views and then least recently used views until the shared budget is met.
/// Callers may also invoke this from their own maintenance tick.
pub fn release_unused(max_bytes: u64) {
    let mut pool = pool().lock().unwrap_or_else(|e| e.into_inner());
    pool.retain(|p| p.strong_count() > 0);
    // Never wait for a builder: one busy view must not delay another view's row window.
    let mut live = Vec::new();
    for weak in pool.iter() {
        if let Some(cache) = weak.upgrade() {
            let used = match cache.try_lock() {
                Ok(c) => c.recency,
                Err(_) => continue,
            };
            live.push((used, cache));
        }
    }
    live.sort_by_key(|(used, _)| *used);
    let mut total = 0u64;
    for (_, c) in &live {
        if let Ok(mut c) = c.try_lock() {
            if c.used
                .is_some_and(|used| c.clock.monotonic().saturating_sub(used) >= c.idle)
            {
                c.release();
            }
            total = total.saturating_add(c.bytes());
        }
    }
    for (_, c) in &live {
        if total <= max_bytes {
            break;
        }
        if let Ok(mut c) = c.try_lock() {
            total = total.saturating_sub(c.bytes());
            c.release();
        }
    }
}
fn register(cache: &Arc<Mutex<Cache>>) {
    pool()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Arc::downgrade(cache));
}

/// Opt-in maintenance for hosts without their own tick. The supplied budget is global.
/// K05 should normally call `release_unused` from its existing tick instead.
pub fn start_maintenance(budget: Budget) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        release_unused(budget.max_bytes);
    });
}

/// A session generation is independent of the writer's sequence: filter/sort updates also advance it.
pub struct ViewSession {
    spec: SessionSpec,
    cfg: CrawlConfig,
    budget: Budget,
    file: PathBuf,
    crawl_id: String,
    version: u64,
    identity: u64,
    cache: Arc<Mutex<Cache>>,
}
impl ViewSession {
    pub fn open(
        file: &CrawlFile,
        cfg: &CrawlConfig,
        spec: SessionSpec,
        budget: Budget,
    ) -> Result<Self, ViewError> {
        Self::open_with_clock(file, cfg, spec, budget, Arc::new(SystemClock))
    }
    /// Injected monotonic clock for deterministic idle expiration.
    pub fn open_with_clock(
        file: &CrawlFile,
        cfg: &CrawlConfig,
        spec: SessionSpec,
        budget: Budget,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, ViewError> {
        // Validation only: no bitmap, permutation or position-list construction here.
        let query = query(&spec);
        build::build(&crate::tabs::configured_for_file(cfg, file), &query, cfg)?;
        let cache = Arc::new(Mutex::new(Cache {
            idle: budget.idle_release,
            clock,
            ..Cache::default()
        }));
        register(&cache);
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Ok(Self {
            identity: NEXT.fetch_add(1, Ordering::Relaxed),
            spec,
            cfg: cfg.clone(),
            budget,
            file: file.path().into(),
            crawl_id: file.get_meta("crawl_id")?.unwrap_or_default(),
            version: 1,
            cache,
        })
    }
    fn check_file(&self, file: &CrawlFile) -> Result<(), ViewError> {
        if file.path() != self.file || file.get_meta("crawl_id")?.as_deref() != Some(&self.crawl_id)
        {
            return Err(ViewError::ForeignCursor);
        }
        Ok(())
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn memory_bytes(&self) -> u64 {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).bytes()
    }
    /// Shared memberships prepared by this session that a writer has not persisted yet (K-1a).
    pub fn shared_pending(&self) -> bool {
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .membership
            .as_ref()
            .is_some_and(|m| super::membership::pending(m))
    }
    pub fn built_seq(&self) -> Option<u64> {
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .view
            .as_ref()
            .map(|v| v.built_seq)
    }
    pub fn update(
        &mut self,
        file: &CrawlFile,
        cfg: &CrawlConfig,
        spec: SessionSpec,
    ) -> Result<(), ViewError> {
        self.update_controlled(file, cfg, spec, &BuildControl::default())
    }
    /// Builds a replacement privately. Failure or cancellation leaves the published view and generation intact.
    pub fn update_controlled(
        &mut self,
        file: &CrawlFile,
        cfg: &CrawlConfig,
        spec: SessionSpec,
        control: &BuildControl,
    ) -> Result<(), ViewError> {
        self.check_file(file)?;
        control.check()?;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let replacement = materialize(file, cfg, &spec, &mut cache, control)?;
        control.check()?;
        cache.view = Some(Arc::new(replacement));
        cache.touch();
        drop(cache);
        self.spec = spec;
        self.cfg = cfg.clone();
        self.version = self
            .version
            .checked_add(1)
            .ok_or_else(|| invalid("View-Version erschöpft"))?;
        release_unused(self.budget.max_bytes);
        Ok(())
    }
    fn cache_ready<'a>(
        &'a self,
        file: &CrawlFile,
        control: &BuildControl,
    ) -> Result<MutexGuard<'a, Cache>, ViewError> {
        self.check_file(file)?;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache
            .used
            .is_some_and(|t| cache.clock.monotonic().saturating_sub(t) >= self.budget.idle_release)
        {
            cache.release();
        }
        if cache.view.is_none() {
            let view = materialize(file, &self.cfg, &self.spec, &mut cache, control)?;
            cache.view = Some(Arc::new(view));
        }
        cache.touch();
        Ok(cache)
    }
    pub fn prepare(&self, file: &CrawlFile, control: &BuildControl) -> Result<u64, ViewError> {
        let cache = self.cache_ready(file, control)?;
        let total = cache.view.as_ref().unwrap().ids.len() as u64;
        drop(cache);
        release_unused(self.budget.max_bytes);
        Ok(total)
    }
    pub fn rows(&self, file: &CrawlFile, req: RowsReq) -> Result<Rows, ViewError> {
        if req.limit == 0 || req.limit > 500 {
            return Err(ViewError::InvalidLimit(req.limit));
        }
        if req.version != self.version {
            return Err(ViewError::ForeignCursor);
        }
        let snapshot = self.snapshot(file)?;
        snapshot.rows(file, req)
    }
    /// Pins the published generation while a replacement is built on a worker. Read it using a
    /// separate read connection; release the pin when the replacement is published or cancelled.
    pub fn snapshot(&self, file: &CrawlFile) -> Result<ViewSnapshot, ViewError> {
        let cache = self.cache_ready(file, &BuildControl::default())?;
        Ok(ViewSnapshot {
            view: cache.view.as_ref().unwrap().clone(),
            version: self.version,
            path: self.file.clone(),
            crawl_id: self.crawl_id.clone(),
        })
    }
    pub fn locate(&self, file: &CrawlFile, id: UrlId) -> Result<Option<u64>, ViewError> {
        let cache = self.cache_ready(file, &BuildControl::default())?;
        let result = u32::try_from(id.0)
            .ok()
            .and_then(|id| cache.view.as_ref().unwrap().inverse.locate(id));
        Ok(result)
    }
    /// Rechecks membership and repairs cached sorts for batches up to one percent of the crawl.
    pub fn apply_changes(&mut self, file: &CrawlFile, ids: &[UrlId]) -> Result<(), ViewError> {
        self.check_file(file)?;
        if ids.is_empty() {
            return Ok(());
        }
        // Membership and binary sort probes must observe the same writer snapshot.
        let _pragmas = control::BuildPragmas::new(file.conn())?;
        let _tx = file
            .conn()
            .is_autocommit()
            .then(|| file.conn().unchecked_transaction())
            .transpose()?;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mut q = query(&self.spec);
        q.search = None;
        let tabs = crate::tabs::configured_for_file(&self.cfg, file);
        let built = build::build(&tabs, &q, &self.cfg)?;
        let ready = super::is_ready(file, &self.cfg, &built)?;
        let key = filter_key(&built, ready);
        if let Some(filter) = cache.filters.get_mut(&key).filter(|_| ready) {
            let reset = file
                .get_meta("reset_seq")?
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            if filter.seq >= reset && filter.stamp == stamp(file)? {
                repair(file, &built, filter, ids)?;
            }
        }
        let seq = queries::change_seq(file.conn())?;
        let reset = file
            .get_meta("reset_seq")?
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let current_stamp = stamp(file)?;
        if let Some(previous) = &cache.membership {
            cache.membership = super::membership::repair(file, previous, ids, &current_stamp)?;
        }
        let mut repaired = HashMap::new();
        for (key, mut permutation) in cache.permutations.drain() {
            let previous_seq = key
                .rsplit_once(':')
                .and_then(|(_, seq)| seq.parse::<u64>().ok())
                .unwrap_or(0);
            if previous_seq >= reset
                && key.ends_with(&format!(":{current_stamp}:{previous_seq}"))
                && permutation.repair(file, ids)?
            {
                let signature = key.rsplit_once(':').unwrap().0;
                repaired.insert(format!("{signature}:{seq}"), permutation);
            }
        }
        cache.permutations = repaired;
        let replacement = materialize(
            file,
            &self.cfg,
            &self.spec,
            &mut cache,
            &BuildControl::default(),
        )?;
        cache.view = Some(Arc::new(replacement));
        cache.touch();
        drop(cache);
        self.version = self
            .version
            .checked_add(1)
            .ok_or_else(|| invalid("View-Version erschöpft"))?;
        release_unused(self.budget.max_bytes);
        Ok(())
    }
    /// Process-local cursor/offset adapter, tied to this generation and session.
    /// K05 must retain the M1 cursor format for externally persisted cursors.
    pub fn page(
        &self,
        file: &CrawlFile,
        start: &PageStart,
        limit: u32,
    ) -> Result<(Rows, Option<String>), ViewError> {
        let signature = format!(
            "{}:{}:{}:{}:{}",
            self.identity, self.crawl_id, self.spec.tab, self.spec.filter, self.version
        );
        let offset = match start {
            PageStart::Offset(n) => *n,
            PageStart::Cursor(s) => {
                let (prefix, offset) = s.rsplit_once('|').ok_or(ViewError::ForeignCursor)?;
                if prefix != signature {
                    return Err(ViewError::ForeignCursor);
                }
                offset.parse().map_err(|_| ViewError::ForeignCursor)?
            }
        };
        let rows = self.rows(
            file,
            RowsReq {
                version: self.version,
                offset,
                limit,
            },
        )?;
        let next = offset.saturating_add(u64::from(limit)).min(rows.total);
        let cursor = (next < rows.total).then(|| format!("{signature}|{next}"));
        Ok((rows, cursor))
    }
    pub fn budget(&self) -> Budget {
        self.budget
    }
    pub fn spec(&self) -> &SessionSpec {
        &self.spec
    }
    pub fn close(self) {}
}
/// Persists the shared memberships of the file's current data version (K-1a, UI-147) after a
/// read-only view prepared them. Call with a writable connection outside any read transaction;
/// read-only and running crawls store nothing. Returns whether a cache entry was written.
pub fn persist_shared(file: &CrawlFile) -> Result<bool, ViewError> {
    super::membership::persist(file)
}

/// Immutable published positions/projection for double-buffered builds.
#[derive(Clone)]
pub struct ViewSnapshot {
    view: Arc<Materialized>,
    version: u64,
    path: PathBuf,
    crawl_id: String,
}
impl ViewSnapshot {
    pub fn rows(&self, file: &CrawlFile, req: RowsReq) -> Result<Rows, ViewError> {
        if req.limit == 0 || req.limit > 500 {
            return Err(ViewError::InvalidLimit(req.limit));
        }
        if req.version != self.version
            || file.path() != self.path
            || file.get_meta("crawl_id")?.as_deref() != Some(&self.crawl_id)
        {
            return Err(ViewError::ForeignCursor);
        }
        let v = &self.view;
        let start = req.offset.min(v.ids.len() as u64) as usize;
        let ids = &v.ids[start..start.saturating_add(req.limit as usize).min(v.ids.len())];
        let (row_keys, rows) = v.projection.rows(file.conn(), ids)?;
        Ok(Rows {
            columns: v.projection.columns.clone(),
            rows,
            row_keys,
            total: v.ids.len() as u64,
            version: self.version,
        })
    }
    pub fn locate(&self, id: UrlId) -> Option<u64> {
        u32::try_from(id.0)
            .ok()
            .and_then(|id| self.view.inverse.locate(id))
    }
    /// Zeilen dieses Stands.
    pub fn total(&self) -> u64 {
        self.view.ids.len() as u64
    }
    /// URL-Kennungen der Zeilen `first..=last` dieses Stands (ohne Datenbankzugriff, K-1).
    pub fn keys(&self, first: u64, last: u64) -> Vec<i64> {
        let end = last.saturating_add(1).min(self.total()) as usize;
        let start = (first as usize).min(end);
        self.view.ids[start..end]
            .iter()
            .map(|id| i64::from(*id))
            .collect()
    }
}
fn query(spec: &SessionSpec) -> ViewQuery {
    let mut q = ViewQuery::new(&spec.tab, &spec.filter);
    q.columns = spec.columns.clone();
    q.search = spec.search.clone();
    q.sort = spec.sort.clone();
    q
}
fn filter_key(built: &build::Built<'_>, ready: bool) -> String {
    // Build without search: predicates encode filter-relevant configuration and params
    // encode thresholds. Presentation, network and search configuration do not belong here.
    crate::cursor::query_hash(&[
        built.tab.id,
        built.filter.id,
        &built.plan.population_sql,
        &built.plan.filter_sql,
        &format!("{:?}", built.plan.params),
        if ready { "ready" } else { "unavailable" },
    ])
}

pub(super) fn stamp(file: &CrawlFile) -> Result<String, ViewError> {
    let mut stmt = file.conn().prepare(
        "SELECT step, state, finished_at, config_hash FROM analysis_steps ORDER BY step",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    Ok(format!(
        "{}:{:?}",
        file.get_meta("reset_seq")?.unwrap_or_default(),
        rows.collect::<Result<Vec<_>, _>>()?
    ))
}
fn repair(
    file: &CrawlFile,
    built: &build::Built<'_>,
    filter: &mut FilterBitmap,
    ids: &[UrlId],
) -> Result<(), ViewError> {
    let sql = format!(
        "SELECT u.id FROM {URLS_FROM} WHERE u.id = :changed_id AND {}",
        build::where_sql(&built.plan)
    );
    let mut stmt = file.conn().prepare(&sql)?;
    for id in ids {
        let index = url_index(id.0)?;
        filter.ids.remove(index);
        exec::bind_named(
            &mut stmt,
            &built.plan.params,
            &[(":changed_id".into(), rusqlite::types::Value::Integer(id.0))],
        )?;
        if stmt.raw_query().next()?.is_some() {
            filter.ids.insert(index);
        }
    }
    filter.seq = queries::change_seq(file.conn())?;
    Ok(())
}
fn materialize(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    spec: &SessionSpec,
    cache: &mut Cache,
    control: &BuildControl,
) -> Result<Materialized, ViewError> {
    control.check()?;
    let _pragmas = control::BuildPragmas::new(file.conn())?;
    let _interrupt = control::InterruptGuard::new(file, control.cancel.clone());
    control.check()?;
    let tx = file
        .conn()
        .is_autocommit()
        .then(|| file.conn().unchecked_transaction())
        .transpose()?;
    let mut q = query(spec);
    q.search = None;
    let tabs = crate::tabs::configured_for_file(cfg, file);
    let built = build::build(&tabs, &q, cfg)?;
    let ready = super::is_ready(file, cfg, &built)?;
    let seq = queries::change_seq(file.conn())?;
    let reset = file
        .get_meta("reset_seq")?
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let stamp = stamp(file)?;
    cache
        .permutations
        .retain(|key, _| key.ends_with(&format!(":{stamp}:{seq}")));
    cache
        .projections
        .retain(|key, _| key.ends_with(&format!(":{seq}")));
    cache
        .filters
        .retain(|_, f| f.seq == seq && f.seq >= reset && f.stamp == stamp);
    let key = filter_key(&built, ready);
    // Common Internal/External live facts are shared across sessions. The stamp
    // covers resets and analysis changes, including resets at the same sequence.
    let common = if ready && super::membership::applicable(&built) {
        Some(super::membership::load(file, seq, &stamp, control)?)
    } else {
        None
    };
    cache.membership = common.clone();
    if let Some(common) = &common {
        for filter in &built.tab.filters {
            let sibling = build::build(&tabs, &ViewQuery::new(built.tab.id, filter.id), cfg)?;
            if let Some(ids) = common.bitmap(&sibling) {
                cache.filters.insert(
                    filter_key(&sibling, true),
                    FilterBitmap {
                        ids,
                        seq,
                        stamp: stamp.clone(),
                    },
                );
            }
        }
    }
    let disk_key = format!("{}:{}", built.tab.id, built.filter.id);
    if !cache
        .filters
        .get(&key)
        .is_some_and(|f| f.seq == seq && f.seq >= reset && f.stamp == stamp)
    {
        let entry = view_cache::load(file, &disk_key, &key)?;
        let bitmap = entry
            .filter(|e| e.built_seq == seq && e.built_seq >= reset && e.step_stamp == stamp)
            .and_then(|e| RoaringBitmap::deserialize_from(e.data.as_slice()).ok());
        let ids = if let Some(bitmap) = common.as_ref().and_then(|m| m.bitmap(&built)).or(bitmap) {
            bitmap
        } else {
            let mut ids = RoaringBitmap::new();
            if ready {
                // UI-141: an All-population pass can prepare its simple content
                // class filters without another crawl scan per subsequent switch.
                let mut classes = HashMap::new();
                if built.filter.predicate == "1"
                    && built.filter.requires.is_empty()
                    && !built.filter.ignore_non_indexable
                {
                    for filter in &built.tab.filters {
                        if !filter.requires.is_empty()
                            || filter.ignore_non_indexable
                            || !matches!(filter.kind, crate::defs::FilterKind::Live)
                        {
                            continue;
                        }
                        if let Some(class) = filter
                            .predicate
                            .strip_prefix("u.content_class = '")
                            .and_then(|s| s.strip_suffix('\''))
                        {
                            let sibling =
                                build::build(&tabs, &ViewQuery::new(built.tab.id, filter.id), cfg)?;
                            classes
                                .insert(class, (filter_key(&sibling, true), RoaringBitmap::new()));
                        }
                    }
                }
                let mut stmt = file.conn().prepare(&format!(
                    "SELECT u.id, u.content_class FROM {URLS_FROM} WHERE {}",
                    build::where_sql(&built.plan)
                ))?;
                exec::bind_named(&mut stmt, &built.plan.params, &[])?;
                let mut rows = stmt.raw_query();
                while let Some(row) = rows.next()? {
                    control.check()?;
                    let id = url_index(row.get(0)?)?;
                    ids.insert(id);
                    if !classes.is_empty() {
                        let class: String = row.get(1)?;
                        if let Some((_, bitmap)) = classes.get_mut(class.as_str()) {
                            bitmap.insert(id);
                        }
                    }
                }
                for (_, (key, ids)) in classes {
                    cache.filters.insert(
                        key,
                        FilterBitmap {
                            ids,
                            seq,
                            stamp: stamp.clone(),
                        },
                    );
                }
            }
            ids
        };
        cache.filters.insert(
            key.clone(),
            FilterBitmap {
                ids,
                seq,
                stamp: stamp.clone(),
            },
        );
    }
    control.cancel.set_progress(0.35);
    let mut allowed = cache.filters[&key].ids.clone();
    for source in &spec.restrict {
        control.check()?;
        allowed &= source.bitmap(file)?;
    }
    if spec.search.is_some() {
        let searched = build::build(&tabs, &query(spec), cfg)?;
        let mut stmt = file.conn().prepare(&format!(
            "SELECT u.id FROM {URLS_FROM} WHERE {}",
            build::where_sql(&searched.plan)
        ))?;
        exec::bind_named(&mut stmt, &searched.plan.params, &[])?;
        let mut rows = stmt.raw_query();
        let mut matches = RoaringBitmap::new();
        while let Some(row) = rows.next()? {
            control.check()?;
            matches.insert(url_index(row.get(0)?)?);
        }
        allowed &= matches;
    }
    let mut positions = Vec::with_capacity(allowed.len() as usize);
    if allowed.is_empty() {
        // A filter waiting for analysis must not sort the crawl merely to return no rows.
    } else if built.sort.is_empty() {
        positions.extend(allowed.iter());
    } else {
        let signature = format!("{}:{stamp}:{seq}", permutation::signature(&built.sort));
        if !cache.permutations.contains_key(&signature) {
            let permutation = Permutation::build(file, &built.sort, &control.cancel)?;
            cache.permutations.insert(signature.clone(), permutation);
        }
        cache.permutations[&signature].visit(built.sort.len() == 1 && built.sort[0].desc, |id| {
            if allowed.contains(id) {
                positions.push(id);
            }
        });
    }
    control.check()?;
    control.cancel.set_progress(0.85);
    let inverse = Inverse::build(&positions);
    let projection_key = format!("{}:{:?}:{ready}:{stamp}:{seq}", built.tab.id, spec.columns);
    let projection = if let Some(projection) = cache.projections.get(&projection_key) {
        projection.clone()
    } else {
        // A01-01: die Spaltenausdrücke tragen benannte Parameter; Bereitschaft aus dem Dateizustand.
        let mut params = built.plan.params.clone();
        crate::tabs::content::bind_analysis_params(&mut params, file, cfg)?;
        let projection = Arc::new(Projection::build(
            file.conn(),
            &built,
            params,
            ready,
            common.as_ref().map(|m| m.html_2xx),
        )?);
        cache.projections.insert(projection_key, projection.clone());
        projection
    };
    control.check()?;
    drop(tx); // Cache writes must not upgrade a WAL read snapshot.
    let filter = &cache.filters[&key];
    let mut data = Vec::with_capacity(filter.ids.serialized_size());
    filter
        .ids
        .serialize_into(&mut data)
        .map_err(|_| invalid("Bitmap-Serialisierung fehlgeschlagen"))?;
    // Cache is optional: lock contention or disk exhaustion must not prevent reading a crawl.
    let _ = view_cache::save(
        file,
        &disk_key,
        &key,
        &view_cache::Entry {
            step_stamp: stamp,
            built_seq: seq,
            data,
        },
    );
    control.check()?;
    control.cancel.set_progress(1.0);
    Ok(Materialized {
        built_seq: seq,
        ids: positions,
        inverse,
        projection,
    })
}
