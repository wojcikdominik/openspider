//! View-Sitzungen der App auf den Bitmaps und Permutationen von `os-analyze` (K-1, K-20).
//!
//! Jede Tab-View (`ViewSpec::Tab`) besitzt eine [`ViewSession`] aus `os-analyze`. Aufbau und Pflege laufen auf den
//! schreibgeschützten Verbindungen des Lese-Pools, nie auf einem Tokio-Worker (2.3 Regel 1); `view_cache` bleibt
//! dadurch ein reiner Speicher-Cache (K-1a „bei schreibgeschützter Datei nur im Speicher“).
//!
//! * **Doppelpuffer:** Zeilen kommen immer aus dem zuletzt veröffentlichten [`ViewSnapshot`]. Ein laufender,
//!   fehlgeschlagener oder abgebrochener Neuaufbau ändert die sichtbare View nicht; ein Abbruch gibt sofort zurück,
//!   ohne auf die unterbrochene SQLite-Anweisung zu warten (UI-141, ≤ 100 ms).
//! * **Version:** `ViewHandle.version` bleibt der `change_seq`, auf dem der veröffentlichte Stand beruht (gleiche API
//!   wie M2-K06; `export.view` vergleicht genau diesen Wert). Die interne Generation der Kernsitzung wird getrennt
//!   geführt.
//! * **Pflege (K-1c):** Der Änderungsfeed der Sitzung (`session::feed`) liefert geänderte `UrlId`s im Takt der
//!   Tabellen (K2-39); betroffene Views werden über `ViewSession::apply_changes` nachgeführt bzw. bei `reset`
//!   neu aufgebaut und als `ViewChanged` im nächsten Tick gemeldet (UI-018, UI-142).
//! * **Freigabe:** Nach `Budget::idle_release` (60 s) ohne Zugriff gibt eine View ihren Speicher frei, der nächste
//!   Zugriff baut lazy wieder auf (UI-142); das Gesamtbudget setzt `release_unused` aus `os-analyze` durch.
//! * **Segmente/Vergleich:** [`RestrictProvider`] liefert Bitmaps (`FilterSource`) für `segment`/`compare`
//!   (Schnittstelle für M2-K14/M2-C01); ohne Anbieter bleiben beide wie bisher `Unsupported`.
//!
//! Der seitenweise Cursor aus `ViewSession::page` ist prozesslokal und wird hier nicht verwendet: die App adressiert
//! Zeilen per Offset und Version; MCP/CLI behalten den M1-Cursor.

use crate::{
    commands::{
        catalog::{expanded_columns, view_error},
        view::session_spec,
    },
    dto::*,
    events::{TickParts, TickSource},
    pool::CrawlKey,
    runtime::{error::internal, sessions::SessionInfo, App},
    session::feed::ChangeSet,
};
use os_analyze::{
    search::FilterSource,
    views::session::{release_unused, Budget, BuildControl, RowsReq, ViewSession, ViewSnapshot},
};
use os_model::{time::Clock, CancelToken, FilterState, UrlId, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::Duration,
};

#[cfg(test)]
#[path = "../../tests/n21/views.rs"]
mod n21_tests;

/// Bitmap-Lieferant für Segment- (K-14) und Vergleichseinschränkungen (K-15) einer Tab-View.
pub trait RestrictProvider: Send + Sync {
    /// Einschränkungen für `crawl`; ein Fehler lehnt `view.open`/`view.update` ab.
    fn restrict(
        &self,
        crawl: &CrawlKey,
        segment: Option<&SegmentFilter>,
        compare: Option<&CompareClass>,
    ) -> Result<Vec<Arc<dyn FilterSource>>, CmdError>;
}

/// Meldet den Anbieter für `segment`/`compare` an (ersetzt einen früheren).
pub fn set_restrict_provider(app: &App, provider: Arc<dyn RestrictProvider>) {
    *lock(&shared(app).provider) = Some(provider);
}

/// Setzt das Speicherbudget für alle künftig geöffneten Views (Standard 512 MB, 60 s Leerlauf).
pub fn set_budget(app: &App, budget: Budget) {
    *lock(&shared(app).budget) = Some(budget);
}

/// Einschränkungen einer Tab-View; ohne Anbieter wie M2-K06 `Unsupported`.
pub(crate) fn restrictions(
    app: &App,
    crawl: &CrawlKey,
    segment: Option<&SegmentFilter>,
    compare: Option<&CompareClass>,
) -> Result<Vec<Arc<dyn FilterSource>>, CmdError> {
    if segment.is_none() && compare.is_none() {
        return Ok(Vec::new());
    }
    let provider = lock(&shared(app).provider).clone();
    match provider {
        Some(p) => p.restrict(crawl, segment, compare),
        None => crate::commands::catalog::check_scope(&segment.cloned(), &compare.cloned())
            .map(|_| Vec::new()),
    }
}

/// Gibt Views frei, die länger als ihr Leerlaufbudget nicht benutzt wurden, und setzt danach das Gesamtbudget durch.
/// Läuft sekündlich im Hintergrund; öffentlich für Tests und Wartung.
pub fn release_idle(app: &App) {
    crate::commands::view::release_closed(app);
    let shared = shared(app);
    let views = live(&shared);
    let now = app.config().clock.monotonic();
    let mut max = Budget::default().max_bytes;
    for view in &views {
        max = max.min(view.budget.max_bytes);
        let mut state = lock(&view.state);
        if state.snapshot.is_some() && now.saturating_sub(state.used) >= view.budget.idle_release {
            state.snapshot = None;
        }
    }
    lock(&shared.links).retain(|l| match l.upgrade() {
        Some(l) => {
            let mut ids = lock(&l.ids);
            if ids
                .as_ref()
                .is_some_and(|(_, used)| now.saturating_sub(*used) >= l.idle)
            {
                *ids = None;
            }
            true
        }
        None => false,
    });
    release_unused(max);
    // Positions the shared LRU budget released must not stay alive in a published snapshot.
    for view in &views {
        if let Ok(session) = view.session.try_lock() {
            if session.as_ref().is_some_and(|s| s.memory_bytes() == 0) {
                lock(&view.state).snapshot = None;
            }
        }
    }
}

/// Belegter Speicher aller Views der App (Positionslisten, Umkehrabbildungen, Bitmaps, Permutationen).
pub fn retained_bytes(app: &App) -> u64 {
    live(&shared(app))
        .iter()
        .map(|v| {
            // try_lock: a running build must not block the measurement
            v.session
                .try_lock()
                .ok()
                .and_then(|s| s.as_ref().map(ViewSession::memory_bytes))
                .unwrap_or(0)
        })
        .sum()
}

/// K-1a, UI-147: derivable view facts prepared on read-only connections (shared memberships,
/// Overview/Issues counts) are stored by the serialized writer. Reopening the saved crawl then
/// skips the crawl-wide scans. Running and read-only crawls keep them in memory only.
pub(crate) fn persist_facts(app: &App, key: &CrawlKey) {
    let (writer, key) = (app.clone(), key.clone());
    app.handle().spawn(async move {
        // CRAWL-126: no new view-cache write once quitting has begun.
        let Some(ticket) = crate::quit::cache_ticket(&writer) else {
            return;
        };
        let result = writer
            .write_queue()
            .submit_file(&key, move |file| {
                if !ticket.open() {
                    return Ok(false);
                }
                os_analyze::views::persist_facts(file).map_err(view_error)
            })
            .await;
        if let Err(error) = result {
            tracing::debug!(?error, "Ansichtsfakten nicht gespeichert");
        }
    });
}

#[derive(Default)]
struct Shared {
    init: AtomicBool,
    provider: Mutex<Option<Arc<dyn RestrictProvider>>>,
    budget: Mutex<Option<Budget>>,
    views: Mutex<Vec<Weak<TabView>>>,
    links: Mutex<Vec<Weak<LinkView>>>,
    changed: Mutex<Vec<ViewChanged>>,
    /// Running maintenance workers (one per view at most) and their end signal, see [`settle`].
    busy: std::sync::atomic::AtomicUsize,
    idle: tokio::sync::Notify,
}
impl TickSource for Shared {
    fn collect(&self, parts: &mut TickParts) {
        for change in std::mem::take(&mut *lock(&self.changed)) {
            parts.views.retain(|v| v.view_id != change.view_id);
            parts.views.push(change);
        }
    }
}
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}
fn shared(app: &App) -> Arc<Shared> {
    let shared = app.ext::<Shared>();
    if !shared.init.swap(true, Ordering::SeqCst) {
        app.add_tick_source(shared.clone());
        crate::session::feed::subscribe(app, Arc::new(on_change));
        let weak = app.downgrade();
        app.handle().spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let Some(app) = weak.upgrade() else { break };
                release_idle(&app);
            }
        });
    }
    shared
}
fn live(shared: &Shared) -> Vec<Arc<TabView>> {
    let mut views = lock(&shared.views);
    views.retain(|v| v.strong_count() > 0);
    views.iter().filter_map(Weak::upgrade).collect()
}

/// Veröffentlichter Stand einer Tab-View.
#[derive(Clone)]
pub(crate) struct Published {
    pub session: SessionInfo,
    pub spec: ViewSpec,
    /// Angefragte Spalten (`view.open`).
    pub columns: Vec<String>,
    /// Spalten der Projektion.
    pub meta: Vec<ColumnMeta>,
    pub version: u64,
    pub total: u64,
    pub state: FilterState,
    cache_stamp: String,
}
/// Ergebnis eines Aufbaus.
pub(crate) struct Built {
    pub version: u64,
    pub total: u64,
    pub meta: Vec<ColumnMeta>,
    pub state: FilterState,
}
/// Ein Zeilenfenster aus dem veröffentlichten Stand.
pub(crate) struct Window {
    pub meta: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Value>>,
    pub keys: Vec<i64>,
}

struct State {
    published: Option<Published>,
    snapshot: Option<ViewSnapshot>,
    /// Generation der Kernsitzung, zu der `snapshot` gehört.
    core: u64,
    used: Duration,
    /// Letzte Meldung eines Zugriffs an die Kernsitzung (deren eigene 60-s-Freigabe).
    touched: Duration,
}
/// Abstand, in dem Zugriffe an die Kernsitzung weitergemeldet werden.
const TOUCH_INTERVAL: Duration = Duration::from_secs(5);
#[derive(Default)]
struct Pending {
    ids: Vec<UrlId>,
    reset: bool,
}

/// Sitzung einer Tab-View der App.
pub(crate) struct TabView {
    view_id: Mutex<String>,
    key: CrawlKey,
    clock: Arc<dyn Clock>,
    budget: Budget,
    session: Arc<Mutex<Option<ViewSession>>>,
    builder: tokio::sync::Mutex<()>,
    state: Mutex<State>,
    pending: Mutex<Option<Pending>>,
    worker: AtomicBool,
}

impl TabView {
    /// Neue, noch nicht aufgebaute View (der Aufbau folgt mit [`TabView::build`]).
    pub fn new(app: &App, view_id: String, key: CrawlKey) -> Arc<TabView> {
        let shared = shared(app);
        let budget = lock(&shared.budget).unwrap_or_default();
        let view = Arc::new(TabView {
            view_id: Mutex::new(view_id),
            key,
            clock: app.config().clock.clone(),
            budget,
            session: Arc::new(Mutex::new(None)),
            builder: tokio::sync::Mutex::new(()),
            state: Mutex::new(State {
                published: None,
                snapshot: None,
                core: 0,
                used: app.config().clock.monotonic(),
                touched: app.config().clock.monotonic(),
            }),
            pending: Mutex::new(None),
            worker: AtomicBool::new(false),
        });
        lock(&shared.views).push(Arc::downgrade(&view));
        view
    }

    pub fn published(&self) -> Option<Published> {
        lock(&self.state).published.clone()
    }

    /// A reused session gets a fresh public handle; closed IDs stay revoked.
    pub fn rebind(&self, view_id: String) {
        *lock(&self.view_id) = view_id;
    }

    /// Reuse a published snapshot only while its file version and idle lease
    /// remain valid. A missed feed notification must never resurrect old rows.
    pub async fn cached(&self, app: &App) -> Result<Option<Built>, CmdError> {
        // Cache probing must not queue behind live maintenance. A busy session
        // is a cache miss; the caller starts its normal cancellable operation.
        let Ok(_builder) = self.builder.try_lock() else {
            return Ok(None);
        };
        let (seq, stamp) = app
            .read_pool()
            .run_cached(
                self.key.clone(),
                "table-cache",
                crate::runtime::current_cancel(),
                |file, _| {
                    let _snapshot = file.conn().unchecked_transaction()?;
                    Ok((
                        os_store::queries::change_seq(file.conn())?,
                        os_analyze::views::filter_cache_stamp(file)?,
                    ))
                },
            )
            .await?;
        let mut st = lock(&self.state);
        if st.snapshot.is_none()
            || self.clock.monotonic().saturating_sub(st.used) >= self.budget.idle_release
        {
            return Ok(None);
        }
        let Some(p) = st.published.as_ref().filter(|p| {
            p.version == seq && p.cache_stamp == stamp && p.state == FilterState::Ready
        }) else {
            return Ok(None);
        };
        let built = Built {
            version: p.version,
            total: p.total,
            meta: p.meta.clone(),
            state: p.state.clone(),
        };
        st.used = self.clock.monotonic();
        Ok(Some(built))
    }

    /// Baut `spec` privat auf und veröffentlicht erst bei Erfolg (Doppelpuffer). Ein Abbruch über `token` gibt
    /// sofort `Cancelled` zurück; die bisherige View bleibt unverändert lesbar.
    pub async fn build(
        self: &Arc<Self>,
        app: &App,
        session: SessionInfo,
        spec: ViewSpec,
        columns: Vec<String>,
        token: CancelToken,
    ) -> Result<Built, CmdError> {
        let core_spec = session_spec(app, &session, &spec, &columns, token.clone())?;
        let _builder = self.builder.lock().await;
        let slot = self.session.clone();
        let cfg = session.config.clone();
        let clock = self.clock.clone();
        let budget = self.budget;
        let control = BuildControl {
            cancel: token.clone(),
        };
        let (tab, filter) = match &spec {
            ViewSpec::Tab { tab, filter, .. } => (tab.clone(), filter.clone()),
            _ => return Err(CmdError::invalid_input("err-tab-required", "spec")),
        };
        let work = app.read_pool().run_cached(
            session.key.clone(),
            "table-build",
            token.clone(),
            move |file, _| {
                let mut guard = lock(&slot);
                let result = (|| {
                    match guard.as_mut() {
                        Some(s) => s.update_controlled(file, &cfg, core_spec, &control)?,
                        None => {
                            let s =
                                ViewSession::open_with_clock(file, &cfg, core_spec, budget, clock)?;
                            s.prepare(file, &control)?;
                            *guard = Some(s);
                        }
                    }
                    let s = guard.as_ref().expect("session");
                    let snapshot = s.snapshot(file)?;
                    let head = snapshot.rows(
                        file,
                        RowsReq {
                            version: s.version(),
                            offset: 0,
                            limit: 1,
                        },
                    )?;
                    let registry = os_analyze::tabs::configured_for_file(&cfg, file);
                    let state =
                        os_analyze::views::state_of_in(&registry, file, &cfg, &tab, &filter)?;
                    let def = os_analyze::names::find_tab(&registry, &tab).expect("validated tab");
                    let meta = expanded_columns(def, &head.columns, &cfg);
                    let seq = s.built_seq().unwrap_or(0);
                    Ok::<_, os_analyze::ViewError>((
                        snapshot,
                        s.version(),
                        seq,
                        head.total,
                        meta,
                        state,
                        os_analyze::views::filter_cache_stamp(file)?,
                        s.shared_pending(),
                    ))
                })();
                Ok(result)
            },
        );
        let (snapshot, core, version, total, meta, state, cache_stamp, pending) =
            until_cancelled(&token, work).await??.map_err(view_error)?;
        if pending {
            persist_facts(app, &self.key);
        }
        let built = Built {
            version,
            total,
            meta: meta.clone(),
            state: state.clone(),
        };
        // Publication and the check for changes queued during the build happen under the lock of `pending`, which
        // `enqueue` holds while it decides whether to start maintenance: a change either finds the view published
        // or is applied right after this publication (UI-018, UI-142).
        let queued = lock(&self.pending);
        let mut st = lock(&self.state);
        st.snapshot = Some(snapshot);
        st.core = core;
        st.used = self.clock.monotonic();
        st.touched = st.used;
        st.published = Some(Published {
            session,
            spec,
            columns,
            meta,
            version,
            total,
            state,
            cache_stamp,
        });
        drop(st);
        let catch_up = queued.is_some();
        drop(queued);
        if catch_up {
            self.start_worker(app);
        }
        Ok(built)
    }

    /// Veröffentlichter Stand mit gültigem Schnappschuss; nach einer Leerlauf-Freigabe lazy neu aufgebaut. Hat sich
    /// der Datenstand inzwischen geändert, wird der neue Stand veröffentlicht und `Stale` gemeldet.
    async fn snapshot(&self, app: &App, version: u64) -> Result<(ViewSnapshot, u64), CmdError> {
        {
            let mut st = lock(&self.state);
            if st.published.as_ref().map(|p| p.version) != Some(version) {
                return Err(crate::commands::view::stale());
            }
            st.used = self.clock.monotonic();
            if let Some(s) = &st.snapshot {
                return Ok((s.clone(), st.core));
            }
        }
        let _builder = self.builder.lock().await;
        let expected_core = {
            let st = lock(&self.state);
            // A build or maintenance run may have published while we waited for the builder.
            if st.published.as_ref().map(|p| p.version) != Some(version) {
                return Err(crate::commands::view::stale());
            }
            if let Some(snapshot) = &st.snapshot {
                return Ok((snapshot.clone(), st.core));
            }
            st.core
        };
        // Cancellation can discard a result after the core has committed it. Never resurrect that spec
        // when the published snapshot has been released. A detached build may still hold the session.
        let diverged = self
            .session
            .try_lock()
            .map(|s| s.as_ref().is_none_or(|s| s.version() != expected_core))
            .unwrap_or(true);
        if diverged {
            self.maintain_locked(
                app,
                Pending {
                    reset: true,
                    ..Default::default()
                },
            )
            .await?;
            let st = lock(&self.state);
            if st.published.as_ref().map(|p| p.version) != Some(version) {
                return Err(crate::commands::view::stale());
            }
            return Ok((st.snapshot.clone().expect("rebuilt snapshot"), st.core));
        }
        let slot = self.session.clone();
        let (snapshot, core, seq, total) = app
            .read_pool()
            .run(self.key.clone(), move |file, _| {
                let guard = lock(&slot);
                let s = guard.as_ref().ok_or_else(|| {
                    os_store::StoreError::InvalidInput("View ohne Sitzung".into())
                })?;
                Ok((|| {
                    let snapshot = s.snapshot(file)?;
                    let total = snapshot
                        .rows(
                            file,
                            RowsReq {
                                version: s.version(),
                                offset: 0,
                                limit: 1,
                            },
                        )?
                        .total;
                    Ok::<_, os_analyze::ViewError>((
                        snapshot,
                        s.version(),
                        s.built_seq().unwrap_or(0),
                        total,
                    ))
                })())
            })
            .await?
            .map_err(view_error)?;
        let mut st = lock(&self.state);
        st.snapshot = Some(snapshot.clone());
        st.core = core;
        st.used = self.clock.monotonic();
        let published = st.published.as_mut().expect("published");
        if published.version != seq || published.total != total {
            published.version = seq;
            published.total = total;
            drop(st);
            self.announce(app, seq, total);
            return Err(crate::commands::view::stale());
        }
        Ok((snapshot, core))
    }

    /// Zeilenfenster der veröffentlichten Version `version`.
    pub async fn rows(
        &self,
        app: &App,
        version: u64,
        offset: u64,
        limit: u32,
    ) -> Result<Window, CmdError> {
        if !(1..=500).contains(&limit) {
            return Err(CmdError::invalid_input("err-invalid-limit", "limit"));
        }
        let (snapshot, core) = self.snapshot(app, version).await?;
        let meta = self.published().map(|p| p.meta).unwrap_or_default();
        let touch = {
            let mut st = lock(&self.state);
            let now = self.clock.monotonic();
            let due = now.saturating_sub(st.touched) >= TOUCH_INTERVAL;
            if due {
                st.touched = now;
            }
            due
        };
        let slot = self.session.clone();
        app.read_pool()
            .run_cached(
                self.key.clone(),
                "table-window",
                crate::runtime::current_cancel(),
                move |file, _| {
                    if touch {
                        // Keeps the core's idle clock in step with reads served from the snapshot; skipped while a
                        // build holds the session.
                        if let Ok(session) = slot.try_lock() {
                            if let Some(s) = session.as_ref() {
                                let _ = s.snapshot(file);
                            }
                        }
                    }
                    let _snapshot = file.conn().unchecked_transaction()?;
                    Ok(snapshot.rows(
                        file,
                        RowsReq {
                            version: core,
                            offset,
                            limit,
                        },
                    ))
                },
            )
            .await?
            .map(|r| Window {
                meta,
                rows: r.rows,
                keys: r.row_keys.iter().map(|id| id.0).collect(),
            })
            .map_err(view_error)
    }

    /// Zeilenindex einer URL (Umkehrabbildung, ohne Datenbankzugriff).
    pub async fn locate(
        &self,
        app: &App,
        version: u64,
        url_id: i64,
    ) -> Result<Option<u64>, CmdError> {
        let (snapshot, _) = self.snapshot(app, version).await?;
        Ok(snapshot.locate(UrlId(url_id)))
    }

    /// Bereich zwischen zwei URLs aus genau einem veröffentlichten Stand (UI-012, UI-018): Der Live-Crawl kann die
    /// Version nicht zwischen dem Nachschlagen der Endpunkte und dem Lesen der Kennungen ändern.
    pub async fn range(
        &self,
        app: &App,
        version: u64,
        from: i64,
        to: i64,
    ) -> Result<(Option<u64>, Option<u64>, u64, Vec<i64>), CmdError> {
        let (snapshot, _) = self.snapshot(app, version).await?;
        let (from_row, to_row) = (snapshot.locate(UrlId(from)), snapshot.locate(UrlId(to)));
        let keys = match (from_row, to_row) {
            (Some(a), Some(b)) if !crate::commands::view::range_requires_export(a, b) => {
                snapshot.keys(a.min(b), a.max(b))
            }
            _ => Vec::new(),
        };
        Ok((from_row, to_row, snapshot.total(), keys))
    }

    fn announce(&self, app: &App, version: u64, total: u64) {
        lock(&shared(app).changed).push(ViewChanged {
            view_id: lock(&self.view_id).clone(),
            version,
            total: Some(total),
        });
    }

    /// Merkt Änderungen vor und startet höchstens einen Pflege-Lauf je View. Eine View, deren erster Aufbau noch läuft,
    /// behält die Änderungen, bis der Aufbau veröffentlicht hat; [`TabView::build`] startet dann die Pflege. Der Aufbau
    /// liest einen Stand, der vor diesen Änderungen liegen kann, und der Änderungsfeed meldet sie kein zweites Mal.
    fn enqueue(self: &Arc<Self>, app: &App, change: &ChangeSet) {
        {
            let mut pending = lock(&self.pending);
            let building = lock(&self.state).published.is_none();
            let p = pending.get_or_insert_with(Pending::default);
            p.reset |= change.reset;
            if p.reset {
                p.ids.clear();
            } else {
                p.ids.extend_from_slice(&change.ids);
                // A long first build collects like the feed (`CHANGE_LIMIT`): a larger set becomes one rebuild.
                if building && p.ids.len() > crate::session::feed::CHANGE_LIMIT as usize {
                    p.reset = true;
                    p.ids.clear();
                }
            }
            if building {
                return;
            }
        }
        self.start_worker(app);
    }

    /// Startet den Pflege-Lauf der View, falls keiner läuft.
    fn start_worker(self: &Arc<Self>, app: &App) {
        if self.worker.swap(true, Ordering::SeqCst) {
            return;
        }
        let view = self.clone();
        let weak = app.downgrade();
        let shared = app.ext::<Shared>();
        // Counted before the spawn: `settle` right after the feed's listeners must see this run.
        shared.busy.fetch_add(1, Ordering::SeqCst);
        app.handle().spawn(async move {
            loop {
                let Some(app) = weak.upgrade() else { break };
                let next = lock(&view.pending).take();
                let Some(next) = next else {
                    view.worker.store(false, Ordering::SeqCst);
                    // A change queued after `take` but before the flag reset starts its own worker or is seen here.
                    if lock(&view.pending).is_some() && !view.worker.swap(true, Ordering::SeqCst) {
                        continue;
                    }
                    break;
                };
                if let Err(e) = view.maintain(&app, next).await {
                    tracing::warn!(error = ?e, view = %*lock(&view.view_id), "View maintenance failed");
                }
            }
            shared.busy.fetch_sub(1, Ordering::SeqCst);
            shared.idle.notify_waiters();
        });
    }

    /// K-1c: geänderte URLs gegen Filter, Suche und Sortierung nachführen bzw. neu aufbauen.
    async fn maintain(&self, app: &App, pending: Pending) -> Result<(), CmdError> {
        let _builder = self.builder.lock().await;
        self.maintain_locked(app, pending).await
    }

    /// Caller holds `builder`, so a lazy rebuild and feed maintenance restore the same published spec.
    async fn maintain_locked(&self, app: &App, mut pending: Pending) -> Result<(), CmdError> {
        let Some(published) = self.published() else {
            return Ok(());
        };
        let expected_core = lock(&self.state).core;
        // A cancelled build may have finished in the core after all: then rebuild the published specification.
        // try_lock: a detached, cancelled build may still hold the session on a pool thread.
        let diverged = self
            .session
            .try_lock()
            .map(|s| s.as_ref().is_none_or(|s| s.version() != expected_core))
            .unwrap_or(true);
        pending.reset |= diverged;
        pending.ids.sort_unstable();
        pending.ids.dedup();
        let rebuild = pending
            .reset
            .then(|| {
                session_spec(
                    app,
                    &published.session,
                    &published.spec,
                    &published.columns,
                    CancelToken::default(),
                )
            })
            .transpose()?;
        let slot = self.session.clone();
        let cfg = published.session.config.clone();
        let clock = self.clock.clone();
        let budget = self.budget;
        let (snapshot, core, seq, total) = app
            .read_pool()
            .run(self.key.clone(), move |file, _| {
                let mut guard = lock(&slot);
                Ok((|| {
                    match (guard.as_mut(), rebuild) {
                        (Some(s), Some(spec)) => s.update(file, &cfg, spec)?,
                        (None, Some(spec)) => {
                            *guard = Some(ViewSession::open_with_clock(
                                file, &cfg, spec, budget, clock,
                            )?)
                        }
                        (Some(s), None) => s.apply_changes(file, &pending.ids)?,
                        (None, None) => unreachable!("a missing session implies a rebuild"),
                    }
                    let s = guard.as_ref().expect("session");
                    let snapshot = s.snapshot(file)?;
                    let total = snapshot
                        .rows(
                            file,
                            RowsReq {
                                version: s.version(),
                                offset: 0,
                                limit: 1,
                            },
                        )?
                        .total;
                    Ok::<_, os_analyze::ViewError>((
                        snapshot,
                        s.version(),
                        s.built_seq().unwrap_or(0),
                        total,
                    ))
                })())
            })
            .await?
            .map_err(view_error)?;
        let mut st = lock(&self.state);
        st.snapshot = Some(snapshot);
        st.core = core;
        let published = st
            .published
            .as_mut()
            .ok_or_else(|| internal("views::maintain", "nicht veröffentlicht"))?;
        let changed = published.version != seq || published.total != total;
        published.version = seq;
        published.total = total;
        drop(st);
        if changed {
            self.announce(app, seq, total);
        }
        Ok(())
    }
}

/// Waits until the view maintenance started by the feed has published, at most `max` (UI-018, B4-LAGFIX2).
/// The session loop calls this between a poll that published rows and the tick, so the totals travel with
/// the status of the same poll. A slower maintenance run keeps going and is announced with a later tick.
pub async fn settle(app: &App, max: Duration) {
    let shared = app.ext::<Shared>();
    let _ = tokio::time::timeout(max, async {
        loop {
            let idle = shared.idle.notified();
            tokio::pin!(idle);
            idle.as_mut().enable();
            if shared.busy.load(Ordering::SeqCst) == 0 {
                return;
            }
            idle.await;
        }
    })
    .await;
}

fn on_change(app: &App, change: &ChangeSet) {
    for view in live(&shared(app)) {
        if view.key == change.key {
            view.enqueue(app, change);
        }
    }
}

/// Wartet auf `work`, gibt bei Abbruch aber sofort `Cancelled` zurück; die unterbrochene Arbeit endet im Hintergrund.
pub(crate) async fn until_cancelled<T>(
    token: &CancelToken,
    work: impl std::future::Future<Output = T>,
) -> Result<T, CmdError> {
    let notify = Arc::new(tokio::sync::Notify::new());
    let signal = notify.clone();
    let _registration = token.on_cancel(move || signal.notify_one());
    if token.is_cancelled() {
        return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
    }
    tokio::select! {
        r = work => Ok(r),
        _ = notify.notified() => Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled")),
    }
}

/// Materialisierte Link-ID-Liste einer Detail-View (Inlinks/Outlinks/Resources, K-1g): einmal in Tabellenreihenfolge
/// gelesen, danach liest jedes Fenster nur seine IDs (kein `COUNT`/`OFFSET` je Fenster, auch bei 1 Mio. Inlinks).
/// Freigabe nach dem Leerlaufbudget wie bei Tab-Views; der nächste Zugriff liest die Liste neu.
pub(crate) struct LinkView {
    key: CrawlKey,
    clock: Arc<dyn Clock>,
    idle: Duration,
    ids: Mutex<Option<(Arc<Vec<i64>>, Duration)>>,
}
impl LinkView {
    pub fn new(app: &App, key: CrawlKey) -> Arc<LinkView> {
        let shared = shared(app);
        let idle = lock(&shared.budget).unwrap_or_default().idle_release;
        let view = Arc::new(LinkView {
            key,
            clock: app.config().clock.clone(),
            idle,
            ids: Mutex::new(None),
        });
        lock(&shared.links).push(Arc::downgrade(&view));
        view
    }
    /// Link IDs of this selection generation, independently of unrelated writer commits.
    pub async fn ids(
        &self,
        app: &App,
        selection: os_store::queries::app_support::LinkSelection,
        page: os_store::queries::PageReq,
    ) -> Result<Arc<Vec<i64>>, CmdError> {
        let now = self.clock.monotonic();
        if let Some((ids, used)) = lock(&self.ids).as_mut() {
            *used = now;
            return Ok(ids.clone());
        }
        let ids = app
            .read_pool()
            .run_cached(
                self.key.clone(),
                "detail-link-ids",
                crate::runtime::current_cancel(),
                move |file, _| {
                    os_store::queries::app_support::selected_link_ids(
                        file.conn(),
                        &selection,
                        &page,
                    )
                },
            )
            .await?;
        let ids = Arc::new(ids);
        *lock(&self.ids) = Some((ids.clone(), now));
        Ok(ids)
    }
}
