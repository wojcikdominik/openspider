//! Vergleichsmodus (K-15, OUT-140 ff.): Zuordnung mit URL-Mapping, Klassifikation Added/New/Removed/Missing je Filter;
//! Change Detection (`compare/change/`, TECH-300 ff., M2-C02).
//!
//! Ablauf: [`CrawlSide::open`] öffnet beide Crawl-Dateien lesend (ältere Schemaversionen als migrierte Kopie, das
//! Original bleibt unverändert, OUT-140), [`Comparison::open`] ordnet sie nach Startzeitpunkt (der spätere ist
//! Current), übersetzt das URL-Mapping ([`mapping`], OUT-146) und bildet die Paare ([`pairing`]). Die Klassifikation je
//! Filter ([`classify`], OUT-142) läuft über die Filter-Bitmaps beider Dateien (K-1) und wird je Filter zwischengespeichert;
//! [`Comparison::swap`] vertauscht die Rollen ohne Neuberechnung der Paare (OUT-141). Overview, Issues (OUT-143),
//! Unterfilter der Haupttabelle ([`ClassSource`] als [`FilterSource`]) und Inlinks/Outlinks ([`links`], OUT-149) lesen
//! dieselben Mengen.
//!
//! Lange Schritte prüfen das [`CancelToken`] an Checkpoints: eine Pause des Ressourcenwächters (CRAWL-129, K2-56)
//! hält die Vergleichsberechnung an, statt sie abzubrechen; ein Abbruch liefert [`CompareError::Cancelled`].

pub mod change;
pub mod changes;
pub mod classify;
pub mod links;
pub mod mapping;
pub mod pairing;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use os_model::{
    CancelToken, CrawlConfig, CrawlMode, FilterState, IssuePriority, IssueType, MappingRule,
};
use os_store::{CrawlFile, CrawlMeta, OpenMode, StoreError};
pub use roaring::RoaringBitmap;

pub use classify::{Classes, CompareClass, FilterComparison, FilterSet, Side};
pub use links::{
    ChangeFilter, LinkChange, LinkComparison, LinkDirection, LinkRow, LinkSource, Presence,
};
pub use mapping::{Mapped, MappingError, MappingStep, MappingTrace, MappingWarning, UrlMapping};
pub use pairing::{Collision, Pairing, EXISTS_SQL};

use crate::defs::TabDef;
use crate::error::ViewError;
use crate::search::FilterSource;

/// Tab der Change Detection: gehört M2-C02 und wird von der Klassifikation nicht erfasst.
pub const CHANGE_DETECTION_TAB: &str = "Change Detection";

/// Ist `tab` ein Tab mit Ergebnissen von KI-Aufgaben (`AI`, `AI Decisions`, `AI Suggestions`, `AI Search`)? Ohne
/// aktive KI erscheinen diese Tabs in keiner Zusammenfassung über die Tab-Registry (Plan M4 E1, E-2 b; M4-V01).
pub fn is_ai_task_tab(tab: &str) -> bool {
    use crate::tabs::{ai, ai_decisions, ai_search, ai_suggestions};
    [
        ai::tab().id,
        ai_decisions::tab().id,
        ai_suggestions::tab().id,
        ai_search::tab().id,
    ]
    .contains(&tab)
}

/// Fehler des Vergleichs.
#[derive(Debug, thiserror::Error)]
pub enum CompareError {
    /// Crawl-Datei nicht lesbar.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// SQL-Fehler.
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    /// Ansicht/Filter nicht auflösbar.
    #[error(transparent)]
    View(#[from] ViewError),
    /// Ungültige Mapping-Regel (mit Position).
    #[error(transparent)]
    Mapping(#[from] MappingError),
    /// Abgebrochen.
    #[error("Vergleich abgebrochen")]
    Cancelled,
    /// Ungültige Eingabe.
    #[error("{0}")]
    Invalid(String),
}

pub(crate) fn checkpoint(cancel: &CancelToken) -> Result<(), CompareError> {
    cancel.checkpoint().map_err(|_| CompareError::Cancelled)
}

/// Eine geöffnete Crawl-Datei mit Konfiguration und Metadaten.
pub struct CrawlSide {
    /// Geöffnete (ggf. migrierte) Datei.
    pub file: CrawlFile,
    /// Konfiguration des Crawls.
    pub config: CrawlConfig,
    /// Metadaten.
    pub meta: CrawlMeta,
    /// Pfad der Originaldatei.
    pub source: PathBuf,
    /// Pfad der migrierten Kopie, falls die Datei eine ältere Schemaversion hatte.
    pub migrated: Option<PathBuf>,
}

impl CrawlSide {
    /// Öffnet eine Crawl-Datei lesend. Braucht sie eine Migration, entsteht in `work_dir` eine migrierte Kopie; das
    /// Original bleibt Byte für Byte unverändert (OUT-140).
    pub fn open(path: &Path, work_dir: &Path) -> Result<CrawlSide, CompareError> {
        match CrawlFile::open(path, OpenMode::ReadOnly) {
            Ok(file) => Self::from_file(file, path.to_path_buf(), None),
            Err(StoreError::MigrationRequired { .. }) => {
                let stem = path
                    .file_stem()
                    .map_or_else(|| "crawl".into(), |s| s.to_string_lossy().into_owned());
                let mut n = 0u32;
                let dst = loop {
                    let candidate = work_dir.join(format!("{stem}.compare-{n}.ospider"));
                    if !candidate.exists() {
                        break candidate;
                    }
                    n += 1;
                };
                let file = CrawlFile::migrate_copy(path, &dst)?;
                tracing::info!(source = %path.display(), copy = %dst.display(), "Vergleich: ältere Crawl-Datei als Kopie migriert");
                Self::from_file(file, path.to_path_buf(), Some(dst))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Übernimmt eine bereits geöffnete Datei.
    pub fn from_file(
        file: CrawlFile,
        source: PathBuf,
        migrated: Option<PathBuf>,
    ) -> Result<CrawlSide, CompareError> {
        let meta = file.meta()?;
        let config = file.config()?;
        Ok(CrawlSide {
            file,
            config,
            meta,
            source,
            migrated,
        })
    }

    fn tabs(&self) -> Vec<TabDef> {
        crate::tabs::configured_for_file(&self.config, &self.file)
    }
}

/// Hinweis zum Vergleich (Anzeige im Dialog, Log).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareNote {
    /// Die Crawls haben unterschiedliche Modi (Spider vs. Liste); der Vergleich ist erlaubt (OUT-140).
    DifferentModes {
        /// Modus des Current-Crawls.
        current: CrawlMode,
        /// Modus des Previous-Crawls.
        previous: CrawlMode,
    },
    /// Eine Datei wurde als Kopie migriert.
    Migrated {
        /// Betroffener Crawl.
        side: Side,
        /// Pfad der Kopie.
        copy: PathBuf,
    },
    /// Kollision im URL-Mapping (OUT-146).
    Collision(Collision),
    /// Regel überschritt das Schrittlimit (OUT-146).
    MappingWarning(MappingWarning),
}

/// Vergleichszeile eines Issues (OUT-143).
#[derive(Debug, Clone, PartialEq)]
pub struct IssueComparison {
    /// Kennung des Issues.
    pub id: String,
    /// Typ.
    pub issue_type: IssueType,
    /// Priorität.
    pub priority: IssuePriority,
    /// Zahlen des zugrunde liegenden Filters „URL hat dieses Issue“.
    pub filter: FilterComparison,
}

/// Unterfilter der Haupttabelle als Einschränkung einer View (K-1, [`FilterSource`]): nur für Added und New, deren
/// Zeilen im Filter des Current-Crawls liegen.
pub struct ClassSource {
    key: String,
    ids: RoaringBitmap,
}

impl FilterSource for ClassSource {
    fn key(&self) -> String {
        self.key.clone()
    }
    fn bitmap(&self, _file: &CrawlFile) -> Result<RoaringBitmap, ViewError> {
        Ok(self.ids.clone())
    }
}

/// Sortiert die Issues nach `Change` (OUT-143); Zeilen ohne `Change` stehen in beiden Richtungen am Ende, Gleichstände
/// nach Kennung.
pub fn sort_by_change(rows: &mut [IssueComparison], descending: bool) {
    rows.sort_by(|a, b| {
        let (x, y) = (a.filter.change(), b.filter.change());
        match (x, y) {
            (Some(x), Some(y)) if descending => y.cmp(&x),
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| a.id.cmp(&b.id))
    });
}

type FilterKey = (String, String);

/// Ein geöffneter Vergleich zweier Crawls.
pub struct Comparison {
    sides: [CrawlSide; 2],
    current: usize,
    rules: Vec<MappingRule>,
    mapping: UrlMapping,
    mapped_side: Side,
    pairing: Pairing,
    cache: [HashMap<FilterKey, Arc<FilterSet>>; 2],
    version: u64,
    notes: Vec<CompareNote>,
}

impl Comparison {
    /// Öffnet den Vergleich; die Reihenfolge der Argumente ist gleichgültig: der Crawl mit dem späteren Startzeitpunkt
    /// ist Current (OUT-140; bei gleichem oder fehlendem Startzeitpunkt bleibt `a` Current).
    pub fn open(
        a: CrawlSide,
        b: CrawlSide,
        rules: &[MappingRule],
        cancel: &CancelToken,
    ) -> Result<Comparison, CompareError> {
        let mapping = UrlMapping::compile(rules)?;
        let (current, previous) = if b.meta.started_at > a.meta.started_at {
            (b, a)
        } else {
            (a, b)
        };
        let pairing = Pairing::build(&current.file, &previous.file, &mapping, cancel)?;
        let mut notes = Vec::new();
        if current.meta.mode != previous.meta.mode {
            notes.push(CompareNote::DifferentModes {
                current: current.meta.mode,
                previous: previous.meta.mode,
            });
        }
        for (side, s) in [(Side::Current, &current), (Side::Previous, &previous)] {
            if let Some(copy) = &s.migrated {
                notes.push(CompareNote::Migrated {
                    side,
                    copy: copy.clone(),
                });
            }
        }
        notes.extend(
            pairing
                .collisions
                .iter()
                .cloned()
                .map(CompareNote::Collision),
        );
        Ok(Comparison {
            sides: [current, previous],
            current: 0,
            rules: rules.to_vec(),
            mapping,
            mapped_side: Side::Previous,
            pairing,
            cache: [HashMap::new(), HashMap::new()],
            version: 1,
            notes,
        })
    }

    fn index(&self, side: Side) -> usize {
        match side {
            Side::Current => self.current,
            Side::Previous => 1 - self.current,
        }
    }

    /// Der Crawl einer Rolle.
    pub fn side(&self, side: Side) -> &CrawlSide {
        &self.sides[self.index(side)]
    }

    /// Der Current-Crawl.
    pub fn current(&self) -> &CrawlSide {
        self.side(Side::Current)
    }

    /// Der Previous-Crawl.
    pub fn previous(&self) -> &CrawlSide {
        self.side(Side::Previous)
    }

    /// Mapping-Regeln, mit denen der Vergleich geöffnet wurde.
    pub fn rules(&self) -> &[MappingRule] {
        &self.rules
    }

    /// Das übersetzte URL-Mapping.
    pub fn mapping(&self) -> &UrlMapping {
        &self.mapping
    }

    /// Ist ein URL-Mapping aktiv (Spalte `Previous Address`, OUT-151)?
    pub fn has_mapping(&self) -> bool {
        !self.mapping.is_empty()
    }

    /// Die Paare in den aktuellen Rollen.
    pub fn pairing(&self) -> &Pairing {
        &self.pairing
    }

    /// Version der Klassifikation (steigt mit jedem Tausch).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Hinweise (Modus-Unterschied, Migration, Kollisionen, Mapping-Warnungen).
    pub fn notes(&self) -> Vec<CompareNote> {
        let mut notes = self.notes.clone();
        notes.extend(
            self.mapping
                .warnings()
                .into_iter()
                .map(CompareNote::MappingWarning),
        );
        notes
    }

    /// Vertauscht Current und Previous (OUT-141): dieselben Paare mit vertauschten Rollen, alle Zähler neu.
    pub fn swap(&mut self) {
        self.current = 1 - self.current;
        self.mapped_side = match self.mapped_side {
            Side::Current => Side::Previous,
            Side::Previous => Side::Current,
        };
        self.pairing = self.pairing.inverted();
        self.version += 1;
    }

    /// Filtermenge einer Seite (zwischengespeichert; `None`, wenn der Filter in diesem Crawl nicht existiert).
    pub fn filter_set(
        &mut self,
        side: Side,
        tab: &str,
        filter: &str,
    ) -> Result<Option<Arc<FilterSet>>, CompareError> {
        let i = self.index(side);
        let key = (tab.to_string(), filter.to_string());
        if let Some(set) = self.cache[i].get(&key) {
            return Ok(Some(set.clone()));
        }
        let s = &self.sides[i];
        let tabs = s.tabs();
        let known = tabs
            .iter()
            .find(|t| t.id == tab)
            .is_some_and(|t| t.filters.iter().any(|f| f.id == filter));
        if !known {
            return Ok(None);
        }
        let set = Arc::new(classify::filter_set(
            &tabs, &s.file, &s.config, tab, filter,
        )?);
        self.cache[i].insert(key, set.clone());
        Ok(Some(set))
    }

    /// Löst `tab`/`filter` gegen die Tabs des Current-Crawls auf (Kennung oder Anzeigename, ohne Beachtung der
    /// Groß-/Kleinschreibung) und liefert die kanonischen Kennungen.
    pub fn resolve(&self, tab: &str, filter: &str) -> Result<(String, String), CompareError> {
        let cfg = &self.current().config;
        let tabs = self.current().tabs();
        let t = tabs
            .iter()
            .find(|t| t.id.eq_ignore_ascii_case(tab))
            .ok_or_else(|| CompareError::Invalid(format!("unbekannter Tab {tab:?}")))?;
        let f = t
            .filters
            .iter()
            .find(|f| {
                f.id.eq_ignore_ascii_case(filter)
                    || crate::names::label(f.id, cfg).eq_ignore_ascii_case(filter)
            })
            .ok_or_else(|| CompareError::Invalid(format!("unbekannter Filter {tab}:{filter}")))?;
        Ok((t.id.to_string(), f.id.to_string()))
    }

    /// Vergleichszeile eines Filters.
    pub fn filter(&mut self, tab: &str, filter: &str) -> Result<FilterComparison, CompareError> {
        let (tab, filter) = self.resolve(tab, filter)?;
        let label = crate::names::label(&filter, &self.current().config);
        let current = self
            .filter_set(Side::Current, &tab, &filter)?
            .ok_or_else(|| CompareError::Invalid(format!("unbekannter Filter {tab}:{filter}")))?;
        let previous = self.filter_set(Side::Previous, &tab, &filter)?;
        let classes = match (&current.ids, previous.as_ref().and_then(|p| p.ids.as_ref())) {
            (Some(c), Some(p)) => {
                let k = Classes::classify(&self.pairing, c, p);
                Some(CompareClass::ALL.map(|class| k.count(class)))
            }
            _ => None,
        };
        Ok(FilterComparison {
            tab,
            filter,
            label,
            current_state: current.state.clone(),
            previous_state: previous.as_ref().map(|p| p.state.clone()),
            current: current.ids.as_ref().map(RoaringBitmap::len),
            previous: previous
                .as_ref()
                .map_or(Some(0), |p| p.ids.as_ref().map(RoaringBitmap::len)),
            classes,
        })
    }

    /// Die vier Mengen eines Filters; `None`, wenn eine Seite keine Zahl hat (Zustand ≠ bereit).
    pub fn classes(&mut self, tab: &str, filter: &str) -> Result<Option<Classes>, CompareError> {
        let (tab, filter) = self.resolve(tab, filter)?;
        let current = self.filter_set(Side::Current, &tab, &filter)?;
        let previous = self.filter_set(Side::Previous, &tab, &filter)?;
        Ok(
            match (
                current.as_ref().and_then(|c| c.ids.as_ref()),
                previous.as_ref().and_then(|p| p.ids.as_ref()),
            ) {
                (Some(c), Some(p)) => Some(Classes::classify(&self.pairing, c, p)),
                _ => None,
            },
        )
    }

    /// Kennungen eines Unterfilters im Crawl, aus dem seine Zeilen stammen (`CompareClass::side`): Ein Klick auf
    /// die Zahl listet genau diese URLs (OUT-142, OUT-143).
    pub fn class_ids(
        &mut self,
        tab: &str,
        filter: &str,
        class: CompareClass,
    ) -> Result<RoaringBitmap, CompareError> {
        let classes = self.classes(tab, filter)?.ok_or_else(|| {
            CompareError::Invalid(format!(
                "Filter {tab}:{filter} hat in einem der Crawls keine Zahl"
            ))
        })?;
        Ok(classes.ids(class, &self.pairing))
    }

    /// Einschränkung einer View des Current-Crawls auf Added oder New (K-1); Removed und Missing liegen außerhalb
    /// des Filters bzw. im Previous-Crawl und laufen über [`Comparison::class_ids`].
    pub fn source(
        &mut self,
        tab: &str,
        filter: &str,
        class: CompareClass,
    ) -> Result<Arc<dyn FilterSource>, CompareError> {
        if class.side() != Side::Current || class == CompareClass::Removed {
            return Err(CompareError::Invalid(format!(
                "Unterfilter {} liegt außerhalb des Filters im Current-Crawl",
                class.as_str()
            )));
        }
        let ids = self.class_ids(tab, filter, class)?;
        Ok(Arc::new(ClassSource {
            key: format!(
                "compare:{}:{}:{}:{}:{}",
                self.current().meta.crawl_id,
                self.previous().meta.crawl_id,
                self.version,
                tab,
                class.as_str().to_ascii_lowercase()
            ) + ":"
                + filter,
            ids,
        }))
    }

    /// Alle Filter aller Tabs des Current-Crawls in Anzeigereihenfolge (ohne Stubs und ohne Change Detection), wie
    /// die Overview im Vergleichsmodus (OUT-142). Prüft das Token je Filter (Pause/Abbruch, CRAWL-129).
    pub fn overview(
        &mut self,
        cancel: &CancelToken,
    ) -> Result<Vec<FilterComparison>, CompareError> {
        // Plan M4 E1 (E-2 b): Tabs aus KI-Aufgaben nur, wenn KI im Crawl aktiv ist (Laufkopie bei Schalter „KI (Vorschau)“
        // aus: `ai.enabled = false`); M4-V01.
        let ai = self.current().config.ai.enabled;
        let tabs: Vec<(String, Vec<String>)> = self
            .current()
            .tabs()
            .iter()
            .filter(|t| !t.is_stub() && t.id != CHANGE_DETECTION_TAB)
            .filter(|t| ai || !is_ai_task_tab(t.id))
            .map(|t| {
                (
                    t.id.to_string(),
                    t.filters
                        .iter()
                        .filter(|f| !f.is_stub())
                        .map(|f| f.id.to_string())
                        .collect(),
                )
            })
            .collect();
        let mut rows = Vec::new();
        for (tab, filters) in tabs {
            for filter in filters {
                checkpoint(cancel)?;
                rows.push(self.filter(&tab, &filter)?);
            }
        }
        Ok(rows)
    }

    /// Issues im Vergleich (OUT-143): jeder Filter mit Katalogeintrag, der in mindestens einem Crawl Treffer hat;
    /// fehlt das Issue in einem Crawl, steht dort 0. Reihenfolge wie die Issues-Liste (Typ, Priorität, Name).
    pub fn issues(&mut self, cancel: &CancelToken) -> Result<Vec<IssueComparison>, CompareError> {
        let mut rows: Vec<IssueComparison> = self
            .overview(cancel)?
            .into_iter()
            .filter(|f| f.current.unwrap_or(0) > 0 || f.previous.unwrap_or(0) > 0)
            .filter_map(|f| {
                let entry = crate::issues::entry_for(&f.tab, &f.filter)
                    .or_else(|| crate::issues::entry_for(&f.tab, &f.label))?;
                Some(IssueComparison {
                    id: entry.id.clone(),
                    issue_type: entry.issue_type,
                    priority: entry.priority,
                    filter: f,
                })
            })
            .collect();
        let order = |t: IssueType| match t {
            IssueType::Issue => 0,
            IssueType::Warning => 1,
            IssueType::Opportunity => 2,
        };
        let prio = |p: IssuePriority| match p {
            IssuePriority::High => 0,
            IssuePriority::Medium => 1,
            IssuePriority::Low => 2,
        };
        rows.sort_by(|a, b| {
            order(a.issue_type)
                .cmp(&order(b.issue_type))
                .then(prio(a.priority).cmp(&prio(b.priority)))
                .then_with(|| {
                    (&a.filter.tab, &a.filter.filter).cmp(&(&b.filter.tab, &b.filter.filter))
                })
        });
        Ok(rows)
    }

    /// URL eines Crawls im gemeinsamen Adressraum beider Crawls: URLs des abgebildeten Crawls (ursprünglich Previous,
    /// nach einem Tausch Current) laufen durch das URL-Mapping, die des anderen bleiben unverändert (OUT-146). Für
    /// Zielvergleiche (Canonical, Weiterleitungsziel; TECH-309/310, OUT-154).
    pub fn common_url(&self, side: Side, url: &str) -> String {
        if side == self.mapped_side && !self.mapping.is_empty() {
            self.mapping.apply(url).url
        } else {
            url.to_string()
        }
    }

    /// Kennung einer URL im anderen Crawl (nach URL-Mapping).
    pub fn partner(&self, side: Side, url_id: i64) -> Option<i64> {
        let id = u32::try_from(url_id).ok()?;
        match side {
            Side::Current => self.pairing.previous_of(id),
            Side::Previous => self.pairing.current_of(id),
        }
        .map(i64::from)
    }

    /// Existiert die URL im Crawl (gecrawlt, OUT-142)?
    pub fn exists(&self, side: Side, url_id: i64) -> bool {
        u32::try_from(url_id).is_ok_and(|id| match side {
            Side::Current => self.pairing.current.contains(id),
            Side::Previous => self.pairing.previous.contains(id),
        })
    }

    /// Inlinks/Outlinks-Vergleich der in `side` markierten URL (OUT-149).
    pub fn links(
        &self,
        side: Side,
        url_id: i64,
        direction: LinkDirection,
    ) -> Result<LinkComparison, CompareError> {
        let partner = self.partner(side, url_id);
        let own = self.exists(side, url_id).then_some(url_id);
        let (current_id, previous_id) = match side {
            Side::Current => (own, partner),
            Side::Previous => (partner, own),
        };
        links::compare_links(
            &self.current().file,
            &self.previous().file,
            &self.mapping,
            self.mapped_side,
            current_id,
            previous_id,
            direction,
        )
    }

    /// Voraussetzung des Change-Detection-Elements „Inhalt“ für einen Crawl (CRAWL-033 → CRAWL-063): gespeichertes
    /// HTML oder gespeicherte Inhalts-Signaturen.
    pub fn content_available(&self, side: Side) -> Result<bool, CompareError> {
        let available: bool = self.side(side).file.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM page_html) OR EXISTS(SELECT 1 FROM minhash)",
            [],
            |r| r.get(0),
        )?;
        Ok(available)
    }

    /// „Inhalt“ ist vergleichbar, wenn beide Crawls die Voraussetzung erfüllen (CRAWL-033).
    pub fn content_comparable(&self) -> Result<bool, CompareError> {
        Ok(self.content_available(Side::Current)? && self.content_available(Side::Previous)?)
    }

    /// Zustand eines Filters in beiden Crawls (Hilfe für Exporte und Commands).
    pub fn states(
        &mut self,
        tab: &str,
        filter: &str,
    ) -> Result<(FilterState, Option<FilterState>), CompareError> {
        let row = self.filter(tab, filter)?;
        Ok((row.current_state, row.previous_state))
    }
}
