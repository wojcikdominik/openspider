//! KI-Warteschlange während des Crawls (08 AI-039, Nutzerentscheidung E-6 a; AI-020 [M4], Nachtrag K4-13; AI-032,
//! Nachtrag K4-24; M4-D05): Umsetzung von [`os_model::ai::hook::AiQueue`] für die Decision-Fragen.
//!
//! Je Crawl-Lauf eine Sitzung auf einem eigenen Thread mit eigener (einfädiger) tokio-Laufzeit und eigener Verbindung
//! zur Crawl-Datei:
//! * **Gespeicherte KI-Eingaben:** Mit der Profil-Option „KI-Eingaben speichern“ (Standard an) schreibt die Sitzung je
//!   interner HTML-Seite mit Status 200 die aufbereiteten Eingaben (`os_store::ai::inputs`) – auch ohne aktive Fragen.
//! * **Automatische Ausführung:** Sind Fragen aktiv und ist die Option „automatische Ausführung“ an (Standard), laufen die
//!   Fragen für jede gecrawlte Seite; Ergebnisse stehen während des Crawls in der Datei. Jeder Teil-Lauf ist ein
//!   `DecisionRun` über die seit dem letzten Teil gekommenen Seiten (`Selection::Open`), mit dem Besitz als
//!   `OwnerLease` (passiv, solange die eigene Engine den Crawl führt; Übernahme nach ihrem Ende, K11).
//! * **Lebenszyklus:** Pause bzw. Stopp senden keinen neuen Request, laufende werden übernommen; Ende ohne Stoppen
//!   (`queue_empty`, `limit_reached`): die Warteschlange läuft, bis sie leer ist, erst dann wird die Zukunft von
//!   [`AiQueueSession::end`] fertig (die Engine ruft danach `on_finished`); Abbruch verwirft laufende Requests.
//! * **Fortsetzen:** dieselbe Laufzeile (`ai_runs`, K4-27) mit ihrer Bestätigung; die bereits gecrawlten Seiten kommen
//!   aus der Crawl-Datei (Leser der Engine), gesendet werden genau die offenen Einträge.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use os_model::ai::hook::{
    AiQueue, AiQueueDone, AiQueueEnd, AiQueueSession, AiQueueStart, InputSource, PageAiFields,
};
use os_model::budget::MicroUsd;
use os_model::CrawlConfig;
use os_store::ai::inputs::{self, StoredImage, StoredInputs};
use os_store::{CrawlFile, OpenMode};
use tokio::sync::{mpsc, watch};

use super::env::RunEnv;
use super::items;
use super::job::{self, Tools};
use crate::decisions::exec::{
    ActiveQuestion, Confirmation, ControlState, RunControl, RunEnd, RunItem, Selection,
};

/// Baut die Umgebung eines Laufs aus der Konfiguration des Crawls und der Besitzerart.
pub type EnvFactory =
    Arc<dyn Fn(&CrawlConfig, &'static str) -> Result<RunEnv, String> + Send + Sync>;

/// KI-Warteschlange einer Anwendung.
#[derive(Clone)]
pub struct DecisionQueue {
    env: EnvFactory,
}

impl std::fmt::Debug for DecisionQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecisionQueue").finish_non_exhaustive()
    }
}

impl DecisionQueue {
    pub fn new(env: EnvFactory) -> Self {
        Self { env }
    }
}

/// Gespeicherte KI-Eingaben aus den Feldern einer geparsten Seite.
pub fn stored_inputs(page: &PageAiFields) -> StoredInputs {
    StoredInputs {
        page_text: page.page_text.clone(),
        h2: page.h2.clone(),
        images: page
            .images
            .iter()
            .map(|i| StoredImage {
                src: i.src.clone(),
                alt: i.alt.clone(),
                context: i.context.clone(),
            })
            .collect(),
        html_lang: page.html_lang.clone(),
        structured_data_types: page.structured_data_types.clone(),
    }
}

/// Je Crawl-Datei höchstens ein sendender KI-Lauf über die Decision-Fragen (eine Sitzung nach dem Fortsetzen bzw. ein
/// Lauf aus der Pause wartet, bis der vorige seine laufenden Requests übernommen hat – sonst ginge ein laufender Eintrag
/// zweimal hinaus).
pub(crate) fn file_lock(path: &Path) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .entry(key)
        .or_default()
        .clone()
}

enum Msg {
    Page(Box<PageAiFields>),
    End(AiQueueEnd),
    /// „Starten“ im Kostenvoranschlag während des Crawls (AI-012): die wartenden Einträge gehen hinaus.
    Confirm,
}

/// Sitzung im Register: Kennung, Kanal und „wartet auf die Kostenbestätigung“.
type Registered = (u64, mpsc::UnboundedSender<Msg>, Arc<AtomicBool>);
type Senders = Mutex<HashMap<PathBuf, Registered>>;

/// Sitzungen dieses Prozesses je Crawl-Datei (für die Kostenbestätigung während des Crawls).
fn senders() -> &'static Senders {
    static SENDERS: OnceLock<Senders> = OnceLock::new();
    SENDERS.get_or_init(Default::default)
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Kostenbestätigung („Starten“, AI-012) für die laufende KI-Warteschlange des Crawls `path`: Die Laufzeile erhält
/// `confirmed_at` (K4-27), die Einträge mit `Pending (confirmation)` und alle folgenden gehen ohne neuen Voranschlag
/// hinaus. Nur, wenn die Warteschlange gerade auf die Bestätigung wartet (08 AI-020, Nachtrag 28.11.2026, einzige
/// Ausnahme von der Abweisung bei laufendem Crawl); sonst `false` – keine Sitzung, automatische Ausführung aus oder
/// nichts wartet.
pub fn confirm_running(path: &Path) -> bool {
    senders()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&canonical(path))
        .is_some_and(|(_, tx, awaiting)| {
            awaiting.load(Ordering::SeqCst) && tx.send(Msg::Confirm).is_ok()
        })
}

struct Session {
    tx: mpsc::UnboundedSender<Msg>,
    control: RunControl,
    done: watch::Receiver<bool>,
}

impl AiQueueSession for Session {
    fn page(&self, page: PageAiFields) {
        let _ = self.tx.send(Msg::Page(Box::new(page)));
    }

    fn end(&self, end: AiQueueEnd) -> AiQueueDone {
        // Sofort wirksam (auch wenn die Zukunft nie abgewartet wird): kein neuer Request ab jetzt.
        match end {
            AiQueueEnd::Paused => self.control.pause(),
            AiQueueEnd::Stopped => self.control.stop(),
            AiQueueEnd::Abort => self.control.abort(),
            AiQueueEnd::Drain => {}
        }
        let _ = self.tx.send(Msg::End(end));
        let mut done = self.done.clone();
        // Fertig, wenn die Sitzung das Ende umgesetzt hat: laufende Requests übernommen (Pause, Stopp) bzw.
        // abgebrochen, offene Einträge als `Pending` geschrieben, bei `Drain` die Warteschlange leer.
        Box::pin(async move {
            let _ = done.wait_for(|d| *d).await;
        })
    }
}

impl AiQueue for DecisionQueue {
    fn begin(&self, start: AiQueueStart) -> Option<Arc<dyn AiQueueSession>> {
        let cfg = &start.config;
        let (questions, errors) = items::active_questions(cfg, &[]);
        for e in &errors {
            super::note(format!("KI-Frage nicht ausführbar: {}", e));
        }
        let auto = cfg.ai.auto_run.get() && !questions.is_empty();
        let store = cfg.ai.store_inputs.get();
        if !auto && !store {
            return None;
        }
        let env = if auto {
            match (self.env)(cfg, start.owner_kind) {
                Ok(env) => Some(env),
                Err(e) => {
                    super::note(format!(
                        "KI-Umgebung nicht verfügbar; nur Speicherung der KI-Eingaben: {}",
                        e
                    ));
                    None
                }
            }
        } else {
            None
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let (done_tx, done) = watch::channel(false);
        let control = RunControl::new();
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let awaiting = Arc::new(AtomicBool::new(false));
        senders().lock().unwrap_or_else(|p| p.into_inner()).insert(
            canonical(&start.crawl_file),
            (id, tx.clone(), awaiting.clone()),
        );
        let worker = Worker {
            awaiting,
            env,
            questions,
            store,
            start,
            control: control.clone(),
            id,
        };
        let spawned = std::thread::Builder::new()
            .name("os-ai-queue".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt.block_on(worker.run(rx)),
                    Err(e) => super::note(format!("KI-Warteschlange ohne Laufzeit: {}", e)),
                }
                let _ = done_tx.send(true);
            });
        if let Err(e) = spawned {
            super::note(format!("KI-Warteschlange nicht gestartet: {}", e));
            return None;
        }
        Some(Arc::new(Session { tx, control, done }))
    }
}

struct Worker {
    env: Option<RunEnv>,
    /// Wartet auf die Kostenbestätigung (für [`confirm_running`]).
    awaiting: Arc<AtomicBool>,
    questions: Vec<ActiveQuestion>,
    store: bool,
    start: AiQueueStart,
    control: RunControl,
    /// Kennung im Register der Sitzungen.
    id: u64,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let mut senders = senders().lock().unwrap_or_else(|p| p.into_inner());
        let key = canonical(&self.start.crawl_file);
        if senders.get(&key).is_some_and(|(id, _, _)| *id == self.id) {
            senders.remove(&key);
        }
    }
}

/// Zustand einer Sitzung zwischen den Nachrichten.
#[derive(Default)]
struct Inbox {
    pending: Vec<PageAiFields>,
    /// Seiten, deren Einträge auf die Kostenbestätigung warten (`Pending (confirmation)`).
    waiting: Vec<PageAiFields>,
    to_store: Vec<(i64, StoredInputs)>,
    end: Option<AiQueueEnd>,
    confirm: bool,
}

impl Inbox {
    fn take(&mut self, msg: Msg, store: bool, ki: bool) {
        match msg {
            Msg::Page(page) => {
                if store && page.is_html_200() && page.source == InputSource::Parsed {
                    self.to_store.push((page.url_id, stored_inputs(&page)));
                }
                if ki {
                    self.pending.push(*page);
                }
            }
            Msg::End(end) => {
                // Ein Abbruch überholt alles; sonst gilt das erste Ende.
                if self.end.is_none() || end == AiQueueEnd::Abort {
                    self.end = Some(end);
                }
            }
            Msg::Confirm => self.confirm = true,
        }
    }
}

impl Worker {
    async fn run(self, mut rx: mpsc::UnboundedReceiver<Msg>) {
        let path = self.start.crawl_file.clone();
        let _registered = super::controls().register(&path, self.control.clone());
        let lock = file_lock(&path);
        let _turn = lock.lock().await;
        let mut inbox = Inbox::default();
        let file = match CrawlFile::open(&path, OpenMode::ReadWrite) {
            Ok(f) => f,
            Err(e) => {
                super::note(format!(
                    "KI-Warteschlange: Crawl-Datei nicht geöffnet: {}",
                    e
                ));
                while let Some(msg) = rx.recv().await {
                    if matches!(msg, Msg::End(_)) {
                        break;
                    }
                }
                return;
            }
        };
        let conn = file.conn();
        let ki = self.env.is_some();
        let mut run = None;
        if let Some(env) = &self.env {
            match self.prepare(env, conn) {
                Ok(state) => run = Some(state),
                Err(e) => super::note(format!("KI-Lauf nicht vorbereitet: {}", e)),
            }
            if let (Some(_), Some(loader)) = (&run, &self.start.existing) {
                match loader(&path, &self.start.config) {
                    Ok(pages) => inbox.pending.extend(pages),
                    Err(e) => super::note(format!("Seiten zum Fortsetzen nicht gelesen: {}", e)),
                }
            }
        }
        let ki = ki && run.is_some();
        // Adapter-Instanzen je KI-Lauf (E8): ein Satz für alle Teil-Läufe dieser Sitzung, damit eine Kontingentsperre
        // für den ganzen Lauf gilt (AI-020, Nachtrag 26.11.2026).
        let tools = self.env.as_ref().map(Tools::new);
        let mut completed = false;
        loop {
            while let Ok(msg) = rx.try_recv() {
                inbox.take(msg, self.store, ki);
            }
            self.flush(conn, &mut inbox);
            if let (Some(env), Some(state), true) = (&self.env, run.as_mut(), inbox.confirm) {
                inbox.confirm = false;
                if let Err(e) = os_store::ai::answers::set_confirmed_at(
                    conn,
                    state.run_id,
                    &env.clock.now().to_db(),
                ) {
                    super::note(format!("Kostenbestätigung nicht gespeichert: {}", e));
                }
                state.confirmed = true;
                state.awaiting_confirmation = false;
                self.awaiting.store(false, Ordering::SeqCst);
                let waiting = std::mem::take(&mut inbox.waiting);
                inbox.pending.extend(waiting);
            }
            let sending = self.control.state() == ControlState::Running;
            if let (Some(state), true, false) = (run.as_mut(), sending, inbox.pending.is_empty()) {
                let env = self.env.as_ref().expect("KI-Umgebung");
                let tools = tools.as_ref().expect("Werkzeuge des KI-Laufs");
                let chunk = std::mem::take(&mut inbox.pending);
                let end = self
                    .dispatch(env, tools, state, conn, &chunk, &mut rx, &mut inbox, ki)
                    .await;
                if end == Some(RunEnd::NeedsConfirmation) {
                    state.awaiting_confirmation = true;
                    self.awaiting.store(true, Ordering::SeqCst);
                    inbox.waiting.extend(chunk);
                }
                continue;
            }
            match inbox.end {
                // Ende ohne Stoppen: leer = fertig; sonst wurde inzwischen pausiert bzw. gestoppt (nichts mehr senden).
                Some(AiQueueEnd::Drain) => {
                    completed = inbox.pending.is_empty() || run.is_none();
                    break;
                }
                Some(_) => break,
                None => {}
            }
            match rx.recv().await {
                Some(msg) => inbox.take(msg, self.store, ki),
                None => break,
            }
        }
        self.awaiting.store(false, Ordering::SeqCst);
        while let Ok(msg) = rx.try_recv() {
            inbox.take(msg, self.store, ki);
        }
        self.flush(conn, &mut inbox);
        // „Ein offener Eintrag ist eine Zeile“ (08 AI-020, Nachtrag 28.11.2026): Seiten, die nach Pause, Stopp, Abbruch
        // bzw. SIGINT nicht mehr gesendet wurden, erhalten ihre Zeile (`Pending`, außerhalb des Geltungsbereichs
        // `Skipped (scope)`), ohne Request und ohne Reservierung – gemeinsam für App, CLI und MCP.
        if run.is_some() && !inbox.pending.is_empty() {
            if let Err(e) = items::mark_open(conn, &self.questions, &inbox.pending) {
                super::note(format!("Offene KI-Einträge nicht geschrieben: {}", e));
            }
        }
        if let (Some(env), Some(state)) = (&self.env, &run) {
            let stopped = self.control.state() == ControlState::Stopped;
            if completed || stopped {
                if let Err(e) =
                    os_store::ai::answers::finish_run(conn, state.run_id, &env.clock.now().to_db())
                {
                    super::note(format!("Laufzeile nicht abgeschlossen: {}", e));
                }
            }
        }
    }

    fn prepare(
        &self,
        env: &RunEnv,
        conn: &os_store::ai::cache::Connection,
    ) -> Result<RunState, os_store::StoreError> {
        let ledger =
            job::shared_ledger(&self.start.crawl_file, conn, &self.start.config.ai.budget)?;
        let run_id = match (self.start.resumed, job::open_run(conn)?) {
            (true, Some(id)) => id,
            _ => job::begin_run(conn, env, &self.start.config, &*ledger)?,
        };
        let confirmed = os_store::ai::answers::confirmed_at(conn, run_id)?.is_some();
        Ok(RunState {
            ledger,
            run_id,
            confirmed,
            awaiting_confirmation: false,
        })
    }

    fn flush(&self, conn: &os_store::ai::cache::Connection, inbox: &mut Inbox) {
        if inbox.to_store.is_empty() {
            return;
        }
        let rows = std::mem::take(&mut inbox.to_store);
        if let Err(e) = inputs::write_all(conn, &rows) {
            super::note(format!("KI-Eingaben nicht gespeichert: {}", e));
        }
    }

    /// Ein Teil-Lauf über `chunk`; nimmt währenddessen weitere Nachrichten an.
    #[allow(clippy::too_many_arguments)]
    async fn dispatch(
        &self,
        env: &RunEnv,
        tools: &Tools,
        state: &mut RunState,
        conn: &os_store::ai::cache::Connection,
        chunk: &[PageAiFields],
        rx: &mut mpsc::UnboundedReceiver<Msg>,
        inbox: &mut Inbox,
        ki: bool,
    ) -> Option<RunEnd> {
        if let Err(e) = items::mark_unavailable(conn, &self.questions, chunk) {
            super::note(format!(
                "Skipped (input unavailable) nicht geschrieben: {}",
                e
            ));
        }
        let run_items: Vec<RunItem> = chunk.iter().map(items::run_item).collect();
        let confirmation = if state.confirmed {
            Confirmation::Granted
        } else if state.awaiting_confirmation {
            // Nach einem Voranschlag über der Schwelle warten auch die Einträge späterer Seiten auf „Starten“.
            Confirmation::AskAbove(MicroUsd(0))
        } else {
            Confirmation::AskAbove(env.threshold)
        };
        // Während des eigenen Crawls schreibt allein die Engine die Besitzerangaben (08 AI-020, „Wer schreibt“); erst
        // nach ihrem Ende (Warteschlange läuft leer) trägt sich der KI-Lauf selbst ein.
        let owner = (inbox.end == Some(AiQueueEnd::Drain)).then(|| {
            job::owner_start(
                self.start.crawl_file.clone(),
                self.start.owner_kind,
                env.clock.clone(),
            )
        });
        let fut = job::run_chunk(
            env,
            tools,
            conn,
            &*state.ledger,
            state.run_id,
            &self.questions,
            &run_items,
            Selection::Open,
            confirmation,
            self.control.clone(),
            owner,
            // Kontingentsperre je KI-Lauf, über alle Seitenblöcke dieser Sitzung (AI-020, Nachtrag 26.11.2026).
            true,
        );
        tokio::pin!(fut);
        let result = loop {
            tokio::select! {
                result = &mut fut => break result,
                Some(msg) = rx.recv() => {
                    inbox.take(msg, self.store, ki);
                    self.flush(conn, inbox);
                }
            }
        };
        match result {
            Ok(report) => {
                for e in &report.commit_errors {
                    super::note(format!("Antwort nicht übernommen: {}", e));
                }
                Some(report.end)
            }
            Err(e) => {
                super::note(format!("KI-Teil-Lauf fehlgeschlagen: {}", e));
                None
            }
        }
    }
}

struct RunState {
    ledger: super::control::LedgerGuard,
    run_id: i64,
    confirmed: bool,
    awaiting_confirmation: bool,
}
