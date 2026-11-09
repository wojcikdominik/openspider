//! Writer-Thread mit begrenztem Kanal (Backpressure, PERF-003).
//!
//! Ein eigener Betriebssystem-Thread besitzt die Senke (`Box<dyn CrawlSink>`, Standard `SqliteSink`) und
//! schreibt, was die Worker über einen **begrenzten** Kanal schicken. Ist der Kanal voll, warten die Worker
//! in `reserve().await`/`send().await`. Zusätzlich drosselt ein gemeinsames Byte-Gate neue Fetches.
//! Bereits zugelassene Seiten und atomare Übergrößen dürfen die Schwelle überschreiten;
//! die Bytebilanz ist kein harter RSS-Deckel.
//!
//! Der Thread fasst die Nachrichten zu Batches zusammen (**Gruppen-Commit**, M1-P30). Die Reihenfolge der
//! Nachrichten bleibt erhalten; der Store verarbeitet innerhalb eines Batches `new_urls` vor `pages`, deshalb
//! genügt es, dass die Nachricht mit den neuen URLs vor der mit der Seite im Kanal liegt (die Worker sorgen
//! dafür, siehe `worker.rs`).
//!
//! * **Normalbetrieb** (die Senke kommt hinterher): geschrieben wird, sobald [`BATCH_PAGES`] Seiten beisammen sind
//!   oder [`BATCH_MAX_WAIT`] seit der ersten Nachricht des Batches verstrichen ist – ein einzelner Batch liegt also
//!   höchstens 100 ms im Speicher.
//! * **Rückstau** (während des letzten Schreibens oder beim Eintreffen der Nachricht war der Kanal nicht leer, die
//!   Senke ist der Engpass): mehrere Batches werden zu **einer** Transaktion zusammengefasst, bis
//!   [`BATCH_PAGES_BACKLOG`] Seiten beisammen sind, das Nutzdatenbudget [`BATCH_BYTES`] erreicht ist
//!   oder [`BATCH_MAX_WAIT_BACKLOG`] verstrichen ist. Jeder Commit
//!   schreibt die geänderten Seiten der Indizes und der Tabellen neu ins WAL; bei gleichverteilten Zielen (jede
//!   Seite berührt 30 verschiedene Zeilen) wächst die Zahl dieser Seiten kaum mit der Batchgröße, die Kosten je
//!   Seite sinken also mit größeren Batches (Messung P28/P30: 1 000 statt 200 Seiten je Commit −42 %, 5 000 −71 %
//!   Writer-Zeit). Der Writer sammelt in seinem eigenen Speicher weiter, die Worker werden dadurch nicht
//!   zusätzlich gedrosselt. Seiten- und Nutzdatenbudget begrenzen den Sammelbatch (PERF-001/003).
//!
//! Was bei einem harten Abbruch ungeschrieben bleibt, ist durch die Konstanten begrenzt: Kanal
//! ([`CHANNEL_CAPACITY`]) plus größter Batch ([`BATCH_PAGES_BACKLOG`]) plus die Abrufe in Bearbeitung (K-49).
//! Eine Worker-Nachricht trägt höchstens eine Seite, der Kanal also höchstens [`CHANNEL_CAPACITY`] Seiten; der
//! Batch in der Senke und der sich füllende Batch sind nie gleichzeitig offen. Mit 8 Threads:
//! 8 + 1 024 + 4 000 = 5 032 Seiten je Abbruch (vor B3-QUEUE 8 + 256 + 4 000 = 4 264).
//!
//! Ein Fehler der Senke wird gemerkt; danach verwirft der Thread weitere Batches (die Worker blockieren
//! nicht) und meldet den Fehler bei `flush`/`finish`. Die Engine bricht den Crawl dann ab.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::diag;
use os_model::WriteBatch;
use os_store::{CrawlSink, StoreError};
use tokio::sync::{mpsc, oneshot};

/// Seiten je Batch im Normalbetrieb (M1-Plan 3.7).
pub const BATCH_PAGES: usize = 200;
/// Größter Batch bei Rückstau. M2-E07: im 250k-Lauf 75 s Writer-Zeit bei 101 s Crawl,
/// 316 kumulierte Worker-Sekunden Kanalwartezeit und fast nur volle 2k-Batches.
/// 4k bündelt die zufälligen Zähler-/Indexänderungen stärker; Kanal (damals 256 Nachrichten,
/// seit B3-QUEUE [`CHANNEL_CAPACITY`]) und Sammelzeit (500 ms) bleiben begrenzt. Der Batch-Speicher
/// verdoppelt sich höchstens.
/// K-49: Die Schranke ungeschriebener Seiten bei hartem Abbruch wächst um den Batchanteil
/// von 2 000 auf 4 000 Seiten; dazu kommen Kanal und Abrufe in Bearbeitung.
/// Im vollständigen 1M-Vergleich sank die Writer-Zeit von 396 auf 295 s bei 803 MB RSS;
/// die Messung umfasst auch die Speicher-/Indexkorrekturen (kein isolierter Batchvergleich).
pub const BATCH_PAGES_BACKLOG: usize = 4_000;
/// Budget für die geschätzten Nutzdaten eingehender Nachrichten je Sammelbatch.
/// Einzelne größere Nachrichten bleiben atomar und werden allein geschrieben.
/// Kein exaktes RSS-Limit: Vec-Wachstum und Allocator-Metadaten kommen hinzu.
pub const BATCH_BYTES: u64 = 8 * 1024 * 1024;
/// Shared admission threshold for fetched bodies and unwritten results (PERF-003).
///
/// B3-QUEUE: drei Batches. Neben dem Batch in der Senke passt der nächste volle Batch in den Kanal
/// (Referenzseiten: 8 MiB + 9 MiB, siehe [`CHANNEL_CAPACITY`]); der Rest deckt die Abrufe und
/// Auswertungen der aktiven Worker. Mit 16 MiB (zwei Batches, M2-N20) griff die Schwelle, bevor der
/// Kanal einen vollen Batch hielt; 32 MiB brachten gegenüber 24 MiB keinen messbaren Gewinn
/// (Server, 1 Mio. URLs: 696 gegen 692 s) bei etwas mehr Puffer (PERF-003: 448 gegen 445 MB).
pub const IN_FLIGHT_BYTES: u64 = 3 * BATCH_BYTES;
/// Längste Wartezeit, bis ein nicht voller Batch im Normalbetrieb geschrieben wird.
pub const BATCH_MAX_WAIT: Duration = Duration::from_millis(100);
/// Längste Wartezeit beim Sammeln eines Gruppen-Commits (Rückstau).
pub const BATCH_MAX_WAIT_BACKLOG: Duration = Duration::from_millis(500);
/// Höchstzahl der Nachrichten je Seite im Kanal: die Discovery-Nachricht (neue URLs, Warteschlange) vor der
/// Seite (`worker.rs`). Eine Obergrenze, kein Normalfall: Sind alle Ziele schon bekannt, entfällt die
/// Discovery-Nachricht. Zu Crawlbeginn kommen fast zwei Nachrichten je Seite an, im eingeschwungenen Crawl
/// eher eine.
pub const MESSAGES_PER_PAGE: usize = 2;
/// Geschätzte Nutzdaten (`estimate_bytes`) einer Seite der PERF-001-Fixture
/// (`synthetic:n=1000000,links=30`) samt Discovery-Nachricht. Gemessen (B3-PERF, 1 Mio. URLs): 2 156
/// Batches zu je rund 464 Seiten, die am Nutzdatenbudget [`BATCH_BYTES`] enden, also gut 18 000 Byte je Seite.
pub const REFERENCE_PAGE_BYTES: u64 = 18 * 1024;
/// Nachrichten, die im Kanal warten dürfen, bevor die Worker blockieren.
///
/// Abgeleitet aus der Batchgröße (B3-QUEUE): Der Kanal fasst auch im ungünstigsten Fall (zwei Nachrichten je
/// Seite, [`MESSAGES_PER_PAGE`]) die Nachrichten eines am Nutzdatenbudget vollen Batches der Referenzseite,
/// auf die nächste Zweierpotenz gerundet (8 MiB / 18 KiB = 455 Seiten, × 2 Nachrichten = 910, also 1 024).
/// Während der Writer einen Batch schreibt, legen die Worker den nächsten vollständig in den Kanal; danach
/// findet der Writer einen vollen Batch vor, statt bis zur Sammelzeit zu warten. Mit 256 Nachrichten
/// (≈ 128 Seiten) warteten Writer und Worker abwechselnd (1 Mio. URLs: Writer 235 s untätig, Worker 41 %
/// ihrer Zeit blockiert).
///
/// „Ein Batch“ gilt genau so nur zu Crawlbeginn. Im eingeschwungenen Crawl (rund eine Nachricht je Seite)
/// fasst der Kanal Seiten für gut zwei Batches; dann begrenzt die Byte-Schwelle [`IN_FLIGHT_BYTES`] den
/// Puffer, nicht die Kanalgröße (PERF-005, 1 Mio.: höchstens 1 492 wiederholte Abrufe je Abbruch, also
/// Kanal und Batch weit unter ihren Seitengrenzen).
pub const CHANNEL_CAPACITY: usize =
    (MESSAGES_PER_PAGE * (BATCH_BYTES / REFERENCE_PAGE_BYTES) as usize).next_power_of_two();

// B3-QUEUE: Die Herleitung gilt beim Übersetzen. Der Kanal fasst die Nachrichten eines vollen Batches der
// Referenzseite, aber höchstens die doppelte Zahl (Kanalanteil der K-49-Grenze); der Batch in der Senke und ein
// voller Kanal aus Referenzseiten liegen unter der Zulassungsschwelle. Laufzeitprüfung gegen Fixture-Seiten:
// Test `the_channel_holds_the_next_full_batch_of_reference_pages_while_the_writer_writes`.
const _: () = {
    let pages_per_batch = (BATCH_BYTES / REFERENCE_PAGE_BYTES) as usize;
    // `>=`: Ist das Produkt selbst eine Zweierpotenz (z. B. Referenzseite 16 KiB: 2 × 512 = 1 024), bleibt
    // `next_power_of_two` bei ihm stehen.
    assert!(CHANNEL_CAPACITY >= MESSAGES_PER_PAGE * pages_per_batch);
    assert!(CHANNEL_CAPACITY <= 2 * MESSAGES_PER_PAGE * pages_per_batch);
    let channel_bytes = (CHANNEL_CAPACITY / MESSAGES_PER_PAGE) as u64 * REFERENCE_PAGE_BYTES;
    assert!(BATCH_BYTES + channel_bytes < IN_FLIGHT_BYTES);
};

/// Nach großen Batches ungenutzte Allocator-Seiten zurückgeben (PERF-003).
/// Kleine normale Commits benötigen diesen zusätzlichen Systemaufruf nicht.
const HEAP_RECLAIM_MIN_BATCH_BYTES: u64 = 8 * 1024 * 1024;

enum Msg {
    Batch(Box<WriteBatch>, Charge),
    /// Antwort: `true` = alles geschrieben, `false` = die Senke hat einen Fehler gemeldet.
    Flush(oneshot::Sender<bool>),
    Checkpoint(oneshot::Sender<bool>),
    /// Folgende Commits synchronisieren das WAL (End-Batch, CRAWL-126).
    Durable(oneshot::Sender<bool>),
}

/// Zähler des Writers (für Tests und die Lastmessung).
#[derive(Debug, Default)]
pub struct WriterStats {
    /// Geschriebene Batches.
    pub batches: AtomicU64,
    /// Geschriebene Seiten (`PageWrite`).
    pub pages: AtomicU64,
    /// Größter Batch in Seiten.
    pub max_batch_pages: AtomicUsize,
    /// Größte beobachtete Zahl wartender Nachrichten.
    pub max_queued: AtomicUsize,
}

struct Shared {
    failed: AtomicBool,
    /// Erster Fehler der Senke (als Original, für `finish`) und sein Text.
    error: Mutex<Option<StoreError>>,
    stats: WriterStats,
    budget: Arc<ByteBudget>,
}

/// Shared byte accounting. Admission happens before Fetch and before reserving a
/// discovery slot. Already active pages may cross the threshold; they must drain
/// without another admission wait (otherwise all workers could deadlock).
#[derive(Default)]
struct ByteBudget {
    bytes: AtomicU64,
    changed: tokio::sync::Notify,
}
pub(crate) struct Charge {
    budget: Arc<ByteBudget>,
    bytes: u64,
    writer_stage: Option<diag::StageGuard>,
}
impl ByteBudget {
    fn charge(self: &Arc<Self>, bytes: u64) -> Charge {
        let total = self.bytes.fetch_add(bytes, Ordering::SeqCst) + bytes;
        diag::peak(&diag::STATS.mem_in_flight_bytes_max, total);
        Charge {
            budget: self.clone(),
            bytes,
            writer_stage: None,
        }
    }
    async fn ready(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.bytes.load(Ordering::SeqCst) < IN_FLIGHT_BYTES {
                return;
            }
            changed.await;
        }
    }
}
impl Charge {
    pub(crate) fn resize(&mut self, bytes: u64) {
        if bytes > self.bytes {
            let total = self
                .budget
                .bytes
                .fetch_add(bytes - self.bytes, Ordering::SeqCst)
                + bytes
                - self.bytes;
            diag::peak(&diag::STATS.mem_in_flight_bytes_max, total);
        } else {
            self.budget
                .bytes
                .fetch_sub(self.bytes - bytes, Ordering::SeqCst);
            self.budget.changed.notify_waiters();
        }
        self.bytes = bytes;
    }
    fn writer(mut self) -> Self {
        self.writer_stage = Some(diag::StageGuard::new(
            &diag::STATS.mem_writer_backlog_entries,
            &diag::STATS.mem_writer_backlog_bytes,
            &diag::STATS.mem_writer_backlog_entries_max,
            &diag::STATS.mem_writer_backlog_bytes_max,
            self.bytes,
        ));
        self
    }
}
impl Drop for Charge {
    fn drop(&mut self) {
        self.budget.bytes.fetch_sub(self.bytes, Ordering::SeqCst);
        self.budget.changed.notify_waiters();
    }
}

/// Der Writer-Thread ist beendet oder hat einen Fehler gemeldet; den Fehler selbst liefert
/// [`WriterThread::finish`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterGone;

impl std::fmt::Display for WriterGone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Writer-Thread ist beendet oder meldet einen Fehler")
    }
}

impl std::error::Error for WriterGone {}

/// Schreibseite: von den Workern geklont.
#[derive(Clone)]
pub struct WriterSender {
    tx: mpsc::Sender<Msg>,
    shared: Arc<Shared>,
}

/// Schwache Schreibseite für die Steuerung von außen (UI-018, B4-LAGFIX2).
#[derive(Clone)]
pub struct WeakWriterSender {
    tx: mpsc::WeakSender<Msg>,
}

impl WeakWriterSender {
    /// Schreibt den im Writer gesammelten Batch sofort, sofern im Kanal nichts wartet: Die App fordert das je
    /// fälligem Oberflächen-Tick an, damit gezählte URLs schon in den Tabellen stehen, statt bis zu
    /// [`BATCH_MAX_WAIT`] später. Bei Rückstau (Kanal nicht leer) oder beendetem Writer `None`; der
    /// Gruppen-Commit bleibt ungestört (PERF-002/003). Die Zukunft liefert `true` nach der Bestätigung.
    pub fn flush_if_idle(&self) -> Option<impl std::future::Future<Output = bool> + Send + use<>> {
        let tx = self.tx.upgrade()?;
        if tx.capacity() < tx.max_capacity() {
            return None;
        }
        let (ack, wait) = oneshot::channel();
        tx.try_send(Msg::Flush(ack)).ok()?;
        Some(async move { matches!(wait.await, Ok(true)) })
    }
}

/// Reservierter Platz im Kanal. Senden geht ohne Warten und ist damit unter einer Sperre möglich.
pub struct Slot {
    permit: mpsc::OwnedPermit<Msg>,
    budget: Arc<ByteBudget>,
}

impl Slot {
    /// Legt den Batch in den Kanal (sofort, ohne Warten).
    pub fn send(self, batch: WriteBatch) {
        if !is_empty(&batch) {
            let charge = self.budget.charge(estimate_bytes(&batch)).writer();
            self.permit.send(Msg::Batch(Box::new(batch), charge));
        }
    }
}

fn is_empty(b: &WriteBatch) -> bool {
    b.cookies.is_empty()
        && b.discoveries.is_empty()
        && b.new_urls.is_empty()
        && b.url_updates.is_empty()
        && b.pages.is_empty()
        && b.queue.is_empty()
        && b.sitemaps.is_empty()
        && b.robots.is_empty()
        && b.meta.is_empty()
        && b.events.is_empty()
        && b.counters.is_empty()
        && b.url_flags.is_empty()
        && b.ext.is_empty()
        && b.structured_data.is_empty()
        && b.sd_issues.is_empty()
        && b.render.is_empty()
        && b.a11y.is_empty()
        && b.spelling.is_empty()
        && b.custom_search.is_empty()
        && b.custom_extraction.is_empty()
        && b.custom_js.is_empty()
        && b.forms.is_empty()
        && b.minhash.is_empty()
        && b.api_values.is_empty()
}

/// Der Writer-Thread samt Kanal.
pub struct WriterThread {
    sender: WriterSender,
    thread: Option<JoinHandle<()>>,
}

impl WriterThread {
    /// Startet den Thread mit der Senke.
    pub fn spawn(sink: Box<dyn CrawlSink>) -> Self {
        Self::spawn_with(sink, CHANNEL_CAPACITY)
    }

    /// Wie [`spawn`](Self::spawn) mit eigener Kanalgröße (Tests).
    pub fn spawn_with(sink: Box<dyn CrawlSink>, capacity: usize) -> Self {
        diag::STATS
            .mem_in_flight_limit_bytes
            .store(IN_FLIGHT_BYTES, Ordering::Relaxed);
        diag::STATS
            .mem_writer_batch_limit_bytes
            .store(BATCH_BYTES, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel(capacity.max(1));
        let shared = Arc::new(Shared {
            failed: AtomicBool::new(false),
            error: Mutex::new(None),
            stats: WriterStats::default(),
            budget: Arc::new(ByteBudget::default()),
        });
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("os-engine-writer".to_string())
            .spawn(move || writer_loop(sink, rx, capacity.max(1), thread_shared))
            .expect("Writer-Thread starten");
        WriterThread {
            sender: WriterSender { tx, shared },
            thread: Some(thread),
        }
    }

    /// Schreibseite für die Worker.
    pub fn sender(&self) -> WriterSender {
        self.sender.clone()
    }

    /// Wartet, bis alles bisher Gesendete geschrieben ist.
    pub async fn flush(&self) -> Result<(), WriterGone> {
        self.sender.flush().await
    }

    /// Die nach diesem Aufruf gesendeten Batches werden mit `synchronous = FULL` geschrieben: ein danach
    /// bestätigtes `flush` ist dauerhaft, auch wenn der Checkpoint das WAL nicht sofort zurückführen kann (CRAWL-126).
    pub async fn durable(&self) -> Result<(), WriterGone> {
        let (ack, wait) = oneshot::channel();
        if self.sender.tx.send(Msg::Durable(ack)).await.is_err() {
            return Err(WriterGone);
        }
        match wait.await {
            Ok(true) => Ok(()),
            _ => Err(WriterGone),
        }
    }

    /// Schreibt ausstehende Daten dauerhaft (WAL-Checkpoint).
    pub async fn checkpoint(&self) -> Result<(), WriterGone> {
        let (ack, wait) = oneshot::channel();
        if self.sender.tx.send(Msg::Checkpoint(ack)).await.is_err() {
            return Err(WriterGone);
        }
        match wait.await {
            Ok(true) => Ok(()),
            _ => Err(WriterGone),
        }
    }

    /// Beendet den Thread: alle Absender müssen vorher freigegeben sein, sonst wartet `finish` auf sie.
    /// Gibt den ersten Fehler der Senke zurück.
    pub fn finish(mut self) -> Result<WriterStatsSnapshot, StoreError> {
        let shared = Arc::clone(&self.sender.shared);
        let thread = self.thread.take();
        // Der letzte Sender verschwindet hier; der Thread beendet sich, sobald der Kanal leer ist.
        drop(self.sender);
        if let Some(t) = thread {
            let _ = t.join();
        }
        if std::env::var_os("OS_SCALE_DIAG").as_deref() == Some(std::ffi::OsStr::new("1")) {
            for (group, values) in [
                ("engine", crate::diag::snapshot()),
                ("store", os_store::sink::diag::snapshot()),
            ] {
                let fields = values
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                eprintln!("OS_SCALE_DIAG {group} {fields}");
            }
        }
        let snapshot = WriterStatsSnapshot::of(&shared.stats);
        let error = shared
            .error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        match error {
            Some(e) => Err(e),
            None => Ok(snapshot),
        }
    }

    /// Zähler.
    pub fn stats(&self) -> WriterStatsSnapshot {
        WriterStatsSnapshot::of(&self.sender.shared.stats)
    }
}

/// Kopie der Zähler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WriterStatsSnapshot {
    /// Geschriebene Batches.
    pub batches: u64,
    /// Geschriebene Seiten.
    pub pages: u64,
    /// Größter Batch in Seiten.
    pub max_batch_pages: usize,
    /// Größte Zahl wartender Nachrichten.
    pub max_queued: usize,
}

impl WriterStatsSnapshot {
    fn of(s: &WriterStats) -> Self {
        WriterStatsSnapshot {
            batches: s.batches.load(Ordering::Relaxed),
            pages: s.pages.load(Ordering::Relaxed),
            max_batch_pages: s.max_batch_pages.load(Ordering::Relaxed),
            max_queued: s.max_queued.load(Ordering::Relaxed),
        }
    }
}

impl WriterSender {
    /// Blocks new Fetch work while retained responses/results exceed the shared
    /// byte threshold. No Frontier lock may be held by the caller.
    pub(crate) async fn wait_for_bytes(&self) {
        let at = Instant::now();
        self.shared.budget.ready().await;
        diag::add(&diag::STATS.byte_wait_ns, at);
    }

    pub(crate) fn fetched_bytes(&self, bytes: u64) -> Charge {
        self.shared.budget.charge(bytes)
    }

    /// Reserviert vor synchroner Frontier-Mutation; bei Gegendruck bleibt die Frontier unverändert.
    pub fn try_reserve(&self) -> Result<Slot, WriterGone> {
        if self.shared.budget.bytes.load(Ordering::SeqCst) >= IN_FLIGHT_BYTES {
            return Err(WriterGone);
        }
        self.tx
            .clone()
            .try_reserve_owned()
            .map(|permit| Slot {
                permit,
                budget: self.shared.budget.clone(),
            })
            .map_err(|_| WriterGone)
    }

    /// Reserviert einen Platz (wartet, solange der Kanal voll ist – hier greift der Gegendruck).
    pub async fn reserve(&self) -> Result<Slot, WriterGone> {
        self.wait_for_bytes().await;
        self.reserve_admitted().await
    }

    /// Drain an already byte-accounted page, even above the admission threshold.
    pub(crate) async fn reserve_admitted(&self) -> Result<Slot, WriterGone> {
        match self.tx.clone().reserve_owned().await {
            Ok(permit) => Ok(Slot {
                permit,
                budget: self.shared.budget.clone(),
            }),
            Err(_) => Err(WriterGone),
        }
    }

    /// Sendet einen Batch (wartet bei vollem Kanal).
    pub async fn send(&self, batch: WriteBatch) -> Result<(), WriterGone> {
        if is_empty(&batch) {
            return Ok(());
        }
        self.wait_for_bytes().await;
        let charge = self.shared.budget.charge(estimate_bytes(&batch));
        self.send_admitted(batch, charge).await
    }

    /// Transfer a live page's byte ownership to the writer without a budget gap.
    pub(crate) async fn send_admitted(
        &self,
        batch: WriteBatch,
        mut charge: Charge,
    ) -> Result<(), WriterGone> {
        charge.resize(estimate_bytes(&batch));
        self.tx
            .send(Msg::Batch(Box::new(batch), charge.writer()))
            .await
            .map_err(|_| WriterGone)
    }

    /// Wartet, bis alles bisher Gesendete geschrieben ist.
    pub async fn flush(&self) -> Result<(), WriterGone> {
        let (ack, wait) = oneshot::channel();
        if self.tx.send(Msg::Flush(ack)).await.is_err() {
            return Err(WriterGone);
        }
        match wait.await {
            Ok(true) => Ok(()),
            _ => Err(WriterGone),
        }
    }

    /// Schwache Schreibseite: hält den Kanal nicht offen, [`WriterThread::finish`] endet wie bisher.
    pub fn downgrade(&self) -> WeakWriterSender {
        WeakWriterSender {
            tx: self.tx.downgrade(),
        }
    }

    /// Ob die Senke einen Fehler gemeldet hat.
    pub fn failed(&self) -> bool {
        self.shared.failed.load(Ordering::SeqCst)
    }

    /// Text des ersten Fehlers der Senke.
    pub fn error(&self) -> Option<String> {
        self.shared
            .error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(ToString::to_string)
    }
}

pub(crate) fn merge(into: &mut WriteBatch, from: WriteBatch) {
    into.cookies.extend(from.cookies);
    into.discoveries.extend(from.discoveries);
    into.new_urls.extend(from.new_urls);
    into.url_updates.extend(from.url_updates);
    into.pages.extend(from.pages);
    into.queue.extend(from.queue);
    into.sitemaps.extend(from.sitemaps);
    into.robots.extend(from.robots);
    into.meta.extend(from.meta);
    into.events.extend(from.events);
    into.counters.extend(from.counters);
    into.url_flags.extend(from.url_flags);
    into.ext.extend(from.ext);
    into.structured_data.extend(from.structured_data);
    into.sd_issues.extend(from.sd_issues);
    into.render.extend(from.render);
    into.a11y.extend(from.a11y);
    into.spelling.extend(from.spelling);
    into.custom_search.extend(from.custom_search);
    into.custom_extraction.extend(from.custom_extraction);
    into.custom_js.extend(from.custom_js);
    into.forms.extend(from.forms);
    into.minhash.extend(from.minhash);
    into.api_values.extend(from.api_values);
}

/// Schätzung der Batch-Nutzdaten: Vektorkapazitäten und die wesentlichen eigenen
/// Heap-Daten im Textcrawl, einschließlich M2-Erweiterungen und gespeichertem HTML.
/// Kein vollständiges Heap-Profil für sämtliche optionalen Fach-/Renderdaten.
pub(crate) fn estimate_bytes(b: &WriteBatch) -> u64 {
    use std::mem::size_of;
    let opt = |s: &Option<String>| s.as_ref().map_or(0, String::capacity);
    let mut bytes = b.new_urls.capacity() * size_of::<os_model::NewUrl>()
        + b.url_updates.capacity() * size_of::<os_model::UrlUpdate>()
        + b.pages.capacity() * size_of::<os_model::PageWrite>()
        + b.queue.capacity() * size_of::<os_model::QueueOp>();
    bytes += b.cookies.capacity() * b.cookies.first().map_or(0, std::mem::size_of_val);
    bytes += b.discoveries.capacity() * b.discoveries.first().map_or(0, std::mem::size_of_val);
    bytes += b.sitemaps.capacity() * b.sitemaps.first().map_or(0, std::mem::size_of_val);
    bytes += b.robots.capacity() * b.robots.first().map_or(0, std::mem::size_of_val);
    bytes += b.meta.capacity() * b.meta.first().map_or(0, std::mem::size_of_val);
    bytes += b.events.capacity() * b.events.first().map_or(0, std::mem::size_of_val);
    bytes += b.counters.capacity() * b.counters.first().map_or(0, std::mem::size_of_val);
    bytes += b.url_flags.capacity() * b.url_flags.first().map_or(0, std::mem::size_of_val);
    bytes += b.ext.capacity() * b.ext.first().map_or(0, std::mem::size_of_val);
    bytes +=
        b.structured_data.capacity() * b.structured_data.first().map_or(0, std::mem::size_of_val);
    bytes += b.sd_issues.capacity() * b.sd_issues.first().map_or(0, std::mem::size_of_val);
    bytes += b.render.capacity() * b.render.first().map_or(0, std::mem::size_of_val);
    bytes += b.a11y.capacity() * b.a11y.first().map_or(0, std::mem::size_of_val);
    bytes += b.spelling.capacity() * b.spelling.first().map_or(0, std::mem::size_of_val);
    bytes += b.custom_search.capacity() * b.custom_search.first().map_or(0, std::mem::size_of_val);
    bytes += b.custom_extraction.capacity()
        * b.custom_extraction.first().map_or(0, std::mem::size_of_val);
    bytes += b.custom_js.capacity() * b.custom_js.first().map_or(0, std::mem::size_of_val);
    bytes += b.forms.capacity() * b.forms.first().map_or(0, std::mem::size_of_val);
    bytes += b.minhash.capacity() * b.minhash.first().map_or(0, std::mem::size_of_val);
    bytes += b.api_values.capacity() * b.api_values.first().map_or(0, std::mem::size_of_val);
    for u in &b.new_urls {
        bytes += u.url.capacity() + u.address.capacity() + u.origin.capacity();
    }
    for e in &b.ext {
        if let std::borrow::Cow::Owned(key) = &e.key.0 {
            bytes += key.capacity();
        }
        if let os_model::Value::Text(text) = &e.value {
            bytes += text.capacity();
        }
    }
    for (key, value) in &b.meta {
        bytes += key.capacity() + value.capacity();
    }
    for e in &b.events {
        bytes += e.kind.capacity() + e.code.capacity() + e.message.capacity();
    }
    for p in &b.pages {
        if let Some(html) = &p.html {
            bytes += html.body.capacity() + opt(&html.encoding);
        }
        bytes += p.fetch.status.capacity()
            + opt(&p.fetch.content_type)
            + opt(&p.fetch.http_version)
            + opt(&p.fetch.last_modified_raw)
            + opt(&p.fetch.redirect_url)
            + opt(&p.fetch.hash_md5)
            + opt(&p.fetch.charset)
            + opt(&p.fetch.robots_matched_line)
            + opt(&p.fetch.robots_txt_url);
        if let Some(page) = &p.page {
            bytes += opt(&page.canonical_url);
            for pair in [
                &page.title,
                &page.meta_description,
                &page.meta_keywords,
                &page.h1,
                &page.h2,
            ] {
                for text in [&pair.first, &pair.second].into_iter().flatten() {
                    bytes += text.text.capacity();
                }
            }
        }
        bytes += p.links.capacity() * size_of::<os_model::LinkRow>()
            + p.images.capacity() * size_of::<os_model::ImageRow>()
            + p.headers.capacity() * size_of::<os_model::HeaderRow>();
        for l in &p.links {
            bytes += l.target_raw.capacity()
                + opt(&l.anchor)
                + opt(&l.alt_text)
                + opt(&l.rel)
                + opt(&l.target_attr)
                + opt(&l.link_path);
        }
        for h in &p.headers {
            bytes += h.name.capacity() + h.value.capacity();
        }
    }
    bytes as u64
}

/// Ob der Batch den Crawl regulär beendet (letzter `state`-Eintrag der Metadaten ist `Completed`).
fn completes_crawl(batch: &WriteBatch) -> bool {
    batch
        .meta
        .iter()
        .rev()
        .find(|(key, _)| key == "state")
        .is_some_and(|(_, value)| value == os_model::CrawlState::Completed.as_str())
}

fn record_error(shared: &Shared, e: StoreError) {
    let mut slot = shared.error.lock().unwrap_or_else(|p| p.into_inner());
    if slot.is_none() {
        *slot = Some(e);
    }
    shared.failed.store(true, Ordering::SeqCst);
}

fn write_out(
    sink: &mut Box<dyn CrawlSink>,
    batch: &mut WriteBatch,
    pages: &mut usize,
    shared: &Shared,
    charges: &mut Vec<Charge>,
) {
    let _charges = std::mem::take(charges);
    if is_empty(batch) {
        return;
    }
    let taken = std::mem::take(batch);
    let n = *pages;
    *pages = 0;
    if shared.failed.load(Ordering::SeqCst) {
        return; // nach einem Fehler nur noch verwerfen
    }
    diag::peak(&diag::STATS.w_pages_max, n as u64);
    let heap_bytes = estimate_bytes(&taken);
    diag::peak(&diag::STATS.mem_writer_batch_bytes_max, heap_bytes);
    let completes = completes_crawl(&taken);
    let t = Instant::now();
    let written = sink.write(taken);
    if heap_bytes >= HEAP_RECLAIM_MIN_BATCH_BYTES {
        // Die Senke hat den Batch konsumiert; dessen freigegebene Puffer können
        // jetzt zurückgegeben werden, ohne laufende Seiten oder Caches zu ändern.
        let reclaim_at = Instant::now();
        let released = os_store::profile::release_system_heap();
        diag::inc(&diag::STATS.w_heap_reclaims);
        if released {
            diag::inc(&diag::STATS.w_heap_reclaim_successes);
        }
        diag::add_max(
            &diag::STATS.w_heap_reclaim_ns,
            &diag::STATS.w_heap_reclaim_max_ns,
            reclaim_at,
        );
    }
    let write_ns = diag::add_max(&diag::STATS.w_write_ns, &diag::STATS.w_write_max_ns, t);
    if completes {
        // Der Schreibaufruf mit `Completed` (Endbatch, K-80): Daten, Indexaufbau und Commit.
        diag::STATS
            .w_end_batch_ns
            .fetch_add(write_ns, Ordering::Relaxed);
        diag::inc(&diag::STATS.w_end_batches);
    }
    diag::inc(&diag::STATS.w_batches);
    diag::STATS.w_pages.fetch_add(n as u64, Ordering::Relaxed);
    diag::record_batch_pages(n);
    match written {
        Ok(()) => {
            let s = &shared.stats;
            s.batches.fetch_add(1, Ordering::Relaxed);
            s.pages.fetch_add(n as u64, Ordering::Relaxed);
            s.max_batch_pages.fetch_max(n, Ordering::Relaxed);
        }
        Err(e) => record_error(shared, e),
    }
}

/// Ergebnis des Wartens auf die nächste Nachricht.
enum Next {
    Msg(Msg),
    /// Die Wartezeit des offenen Batches ist abgelaufen.
    Timeout,
    /// Alle Absender sind weg und der Kanal ist leer.
    Closed,
}

fn writer_loop(
    mut sink: Box<dyn CrawlSink>,
    mut rx: mpsc::Receiver<Msg>,
    capacity: usize,
    shared: Arc<Shared>,
) {
    // Eigene kleine Laufzeit nur für das Warten mit Zeitgrenze: `recv()` mit `timeout` schläft im Betriebssystem
    // bis zur nächsten Nachricht oder zum Ablauf – ohne Abfragen im Millisekundentakt (unter Windows rundet
    // `sleep(1 ms)` auf die Timer-Auflösung von ~15,6 ms).
    let waiter = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("Warteschleife des Writers");
    let mut batch = WriteBatch::default();
    let mut pages = 0usize;
    let mut charges = Vec::new();
    let mut batch_bytes = 0u64;
    let mut first_at: Option<Instant> = None;
    // An explicit checkpoint with no batch after it makes the closing one redundant: with a reader still holding a
    // snapshot it would only wait another bounded interval (CRAWL-126). "Successful" means the call returned without
    // error, not that the WAL is empty: frames a reader kept are merged later in the background (CRAWL-126), or by
    // SQLite when the last connection closes.
    let mut checkpointed = false;
    // Die Senke ist der Engpass: bei der letzten Nachricht oder nach dem letzten Schreiben stand Arbeit im Kanal.
    let mut behind = false;

    loop {
        let next = match first_at {
            None => {
                let t = Instant::now();
                let received = rx.try_recv().ok().or_else(|| waiter.block_on(rx.recv()));
                diag::add(&diag::STATS.w_idle_ns, t);
                received.map_or(Next::Closed, Next::Msg)
            }
            Some(first) => match rx.try_recv() {
                Ok(m) => Next::Msg(m),
                Err(mpsc::error::TryRecvError::Disconnected) => Next::Closed,
                Err(mpsc::error::TryRecvError::Empty) => {
                    let max_wait = if behind {
                        BATCH_MAX_WAIT_BACKLOG
                    } else {
                        BATCH_MAX_WAIT
                    };
                    let left = max_wait.saturating_sub(first.elapsed());
                    if left.is_zero() {
                        Next::Timeout
                    } else {
                        let t = Instant::now();
                        let received =
                            waiter.block_on(async { tokio::time::timeout(left, rx.recv()).await });
                        diag::inc(&diag::STATS.w_poll_sleeps);
                        diag::add(&diag::STATS.w_poll_sleep_ns, t);
                        match received {
                            Ok(Some(m)) => Next::Msg(m),
                            Ok(None) => Next::Closed,
                            Err(_) => Next::Timeout,
                        }
                    }
                }
            },
        };
        let msg = match next {
            Next::Msg(m) => m,
            Next::Timeout => {
                write_out(&mut sink, &mut batch, &mut pages, &shared, &mut charges);
                batch_bytes = 0;
                first_at = None;
                behind = !rx.is_empty();
                continue;
            }
            Next::Closed => break,
        };
        let queued = rx.len() + 1;
        shared.stats.max_queued.fetch_max(queued, Ordering::Relaxed);
        diag::STATS
            .w_queue_max
            .fetch_max(queued as u64, Ordering::Relaxed);
        match msg {
            Msg::Batch(b, charge) => {
                checkpointed = false;
                let incoming_bytes = estimate_bytes(&b);
                if batch_bytes.saturating_add(incoming_bytes) > BATCH_BYTES {
                    write_out(&mut sink, &mut batch, &mut pages, &shared, &mut charges);
                    batch_bytes = 0;
                    first_at = None;
                    behind = !rx.is_empty();
                }
                charges.push(charge);
                batch_bytes = batch_bytes.saturating_add(incoming_bytes);
                pages += b.pages.len();
                merge(&mut batch, *b);
                first_at.get_or_insert_with(Instant::now);
                // Rückstau auch dann, wenn schon mehr als ein Achtel des Kanals wartet. B3-QUEUE: bewusst relativ
                // gelassen (bei 1 024 Nachrichten also ab 129 statt ab 33). Hauptauslöser ist `behind` (nach jedem
                // Schreiben mit nicht leerem Kanal); die Achtel-Regel greift nur für den ersten Batch nach einer
                // Ruhephase, und 129 Nachrichten sind höchstens 129 Seiten, weniger als `BATCH_PAGES`. Spätestens
                // der nächste Batch sammelt dann mit `BATCH_PAGES_BACKLOG`. Alle B3-QUEUE-Messungen liefen so.
                let backlog = behind || rx.len() * 8 > capacity;
                let limit = if backlog {
                    BATCH_PAGES_BACKLOG
                } else {
                    BATCH_PAGES
                };
                if pages >= limit || batch_bytes >= BATCH_BYTES {
                    write_out(&mut sink, &mut batch, &mut pages, &shared, &mut charges);
                    batch_bytes = 0;
                    first_at = None;
                    behind = !rx.is_empty();
                }
            }
            Msg::Flush(ack) => {
                write_out(&mut sink, &mut batch, &mut pages, &shared, &mut charges);
                batch_bytes = 0;
                first_at = None;
                behind = !rx.is_empty();
                let _ = ack.send(!shared.failed.load(Ordering::SeqCst));
            }
            Msg::Checkpoint(ack) => {
                write_out(&mut sink, &mut batch, &mut pages, &shared, &mut charges);
                batch_bytes = 0;
                first_at = None;
                behind = !rx.is_empty();
                let ok = if shared.failed.load(Ordering::SeqCst) {
                    false
                } else if let Err(e) = sink.checkpoint() {
                    record_error(&shared, e);
                    false
                } else {
                    true
                };
                checkpointed = ok;
                let _ = ack.send(ok);
            }
            Msg::Durable(ack) => {
                write_out(&mut sink, &mut batch, &mut pages, &shared, &mut charges);
                batch_bytes = 0;
                first_at = None;
                behind = !rx.is_empty();
                let ok = if shared.failed.load(Ordering::SeqCst) {
                    false
                } else if let Err(e) = sink.durable() {
                    record_error(&shared, e);
                    false
                } else {
                    true
                };
                let _ = ack.send(ok);
            }
        }
    }
    write_out(&mut sink, &mut batch, &mut pages, &shared, &mut charges);
    if !checkpointed && !shared.failed.load(Ordering::SeqCst) {
        if let Err(e) = sink.checkpoint() {
            record_error(&shared, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use os_model::{CrawlEvent, Timestamp};

    #[derive(Default)]
    struct Log {
        batches: Vec<WriteBatch>,
        checkpoints: u32,
    }

    struct TestSink {
        log: Arc<Mutex<Log>>,
        delay: Duration,
        fail_at: Option<usize>,
    }

    impl CrawlSink for TestSink {
        fn write(&mut self, batch: WriteBatch) -> Result<(), StoreError> {
            std::thread::sleep(self.delay);
            let mut log = self.log.lock().unwrap();
            if self.fail_at == Some(log.batches.len()) {
                return Err(StoreError::NotImplemented("Testfehler"));
            }
            log.batches.push(batch);
            Ok(())
        }
        fn checkpoint(&mut self) -> Result<(), StoreError> {
            self.log.lock().unwrap().checkpoints += 1;
            Ok(())
        }
    }

    fn sink(delay: Duration, fail_at: Option<usize>) -> (Box<dyn CrawlSink>, Arc<Mutex<Log>>) {
        let log = Arc::new(Mutex::new(Log::default()));
        (
            Box::new(TestSink {
                log: Arc::clone(&log),
                delay,
                fail_at,
            }),
            log,
        )
    }

    fn page_batch(n: usize) -> WriteBatch {
        let mut b = WriteBatch::default();
        for i in 0..n {
            b.pages.push(os_model::PageWrite {
                url_id: os_model::UrlId(i as i64 + 1),
                replace: false,
                fetch: os_model::FetchFacts::default(),
                page: None,
                indexability: os_model::Indexability::Indexable,
                indexability_status: None,
                links: vec![],
                images: vec![],
                canonicals: vec![],
                directives: vec![],
                refresh: vec![],
                hreflang: vec![],
                pagination: vec![],
                headers: vec![],
                html: None,
            });
        }
        b
    }

    // spec: PERF-003
    #[tokio::test]
    async fn byte_admission_waits_until_the_slow_fixture_store_consumes_its_batch() {
        use std::future::Future;
        use std::task::{Context, Poll};
        struct GatedSink {
            entered: Option<oneshot::Sender<()>>,
            release: std::sync::mpsc::Receiver<()>,
        }
        impl CrawlSink for GatedSink {
            fn write(&mut self, batch: WriteBatch) -> Result<(), StoreError> {
                if let Some(entered) = self.entered.take() {
                    let _ = entered.send(());
                    self.release.recv().unwrap();
                }
                drop(batch);
                Ok(())
            }
            fn checkpoint(&mut self) -> Result<(), StoreError> {
                Ok(())
            }
        }
        fn fixture(bytes: u64) -> WriteBatch {
            let mut batch = page_batch(1);
            let overhead = estimate_bytes(&batch);
            batch.pages[0].html = Some(os_model::PageHtml {
                encoding: None,
                body: vec![b'x'; (bytes - overhead) as usize],
            });
            assert_eq!(estimate_bytes(&batch), bytes);
            batch
        }
        let (entered, wait) = oneshot::channel();
        let (release, block) = std::sync::mpsc::channel();
        let writer = WriterThread::spawn(Box::new(GatedSink {
            entered: Some(entered),
            release: block,
        }));
        let tx = writer.sender();
        // A full byte budget, despite only one channel message. No wall-clock
        // threshold: the sink is released explicitly after polling the gate.
        let mut page = tx.fetched_bytes(IN_FLIGHT_BYTES / 2);
        // Transfer Fetch -> Parse -> Writer with the same ownership token.
        page.resize(IN_FLIGHT_BYTES);
        tx.send_admitted(fixture(IN_FLIGHT_BYTES), page)
            .await
            .unwrap();
        wait.await.unwrap();
        assert_eq!(
            tx.shared.budget.bytes.load(Ordering::SeqCst),
            IN_FLIGHT_BYTES
        );
        {
            let mut admission = std::pin::pin!(tx.wait_for_bytes());
            let mut context = Context::from_waker(futures_util::task::noop_waker_ref());
            assert_eq!(admission.as_mut().poll(&mut context), Poll::Pending);
            assert!(tx.try_reserve().is_err());
            release.send(()).unwrap();
            admission.await;
        }
        tx.flush().await.unwrap();
        assert_eq!(tx.shared.budget.bytes.load(Ordering::SeqCst), 0);
        drop(tx);
        assert_eq!(writer.finish().unwrap().pages, 1);
    }

    // spec: PERF-002, PERF-003
    // B3-QUEUE: Während der Writer einen vollen Batch schreibt, legen die Worker den nächsten vollen Batch aus
    // Referenzseiten im ungünstigsten Fall (je eine Discovery- und eine Seitennachricht, Crawlbeginn) in den Kanal,
    // ohne auf Platz im Kanal oder auf das Byte-Gate zu warten. Danach schreibt der Writer ihn am Nutzdatenbudget,
    // ohne auf die Sammelzeit zu warten.
    #[tokio::test]
    async fn the_channel_holds_the_next_full_batch_of_reference_pages_while_the_writer_writes() {
        struct GatedSink {
            entered: Option<oneshot::Sender<()>>,
            release: std::sync::mpsc::Receiver<()>,
            pages: Arc<Mutex<Vec<usize>>>,
        }
        impl CrawlSink for GatedSink {
            fn write(&mut self, batch: WriteBatch) -> Result<(), StoreError> {
                if let Some(entered) = self.entered.take() {
                    let _ = entered.send(());
                    self.release.recv().unwrap();
                }
                self.pages.lock().unwrap().push(batch.pages.len());
                Ok(())
            }
            fn checkpoint(&mut self) -> Result<(), StoreError> {
                Ok(())
            }
        }
        fn page_of(bytes: u64) -> WriteBatch {
            let mut batch = page_batch(1);
            let overhead = estimate_bytes(&batch);
            batch.pages[0].html = Some(os_model::PageHtml {
                encoding: None,
                body: vec![b'x'; (bytes - overhead) as usize],
            });
            assert_eq!(estimate_bytes(&batch), bytes);
            batch
        }
        let (entered, wait) = oneshot::channel();
        let (release, block) = std::sync::mpsc::channel();
        let pages = Arc::new(Mutex::new(Vec::new()));
        let writer = WriterThread::spawn(Box::new(GatedSink {
            entered: Some(entered),
            release: block,
            pages: Arc::clone(&pages),
        }));
        let tx = writer.sender();
        // Ein Batch am Nutzdatenbudget geht sofort in die Senke und bleibt dort, bis der Test ihn freigibt.
        tx.send(page_of(BATCH_BYTES)).await.unwrap();
        wait.await.unwrap();
        // Der nächste volle Batch im ungünstigsten Fall: je Seite eine Discovery- und eine Seitennachricht.
        let discovery = event("discovery");
        let discovery_bytes = estimate_bytes(&discovery);
        let per_batch = (BATCH_BYTES / REFERENCE_PAGE_BYTES) as usize;
        for i in 0..per_batch {
            let slot = tx
                .try_reserve()
                .unwrap_or_else(|_| panic!("Worker wartet vor Discovery {i} von {per_batch}"));
            slot.send(event("discovery"));
            let slot = tx
                .try_reserve()
                .unwrap_or_else(|_| panic!("Worker wartet vor Seite {i} von {per_batch}"));
            slot.send(page_of(REFERENCE_PAGE_BYTES - discovery_bytes));
        }
        // Eine überzählige Seite mit bekannten Zielen (ohne Discovery) überschreitet das Budget. Sie passt noch in
        // den Kanal, außer der Batch füllt ihn genau (Referenzseite mit Zweierpotenz-Produkt, z. B. 16 KiB); dann
        // endet der Batch schon an seiner letzten Seite am Budget, und die überzählige Seite folgt nach der Freigabe.
        let extra = page_of(REFERENCE_PAGE_BYTES);
        let extra = match tx.try_reserve() {
            Ok(slot) => {
                slot.send(extra);
                None
            }
            Err(_) => {
                assert_eq!(CHANNEL_CAPACITY, MESSAGES_PER_PAGE * per_batch);
                Some(extra)
            }
        };
        assert!(
            tx.shared.budget.bytes.load(Ordering::SeqCst) < IN_FLIGHT_BYTES,
            "der schreibende und der wartende Batch liegen unter der Zulassungsschwelle"
        );
        release.send(()).unwrap();
        if let Some(extra) = extra {
            tx.send(extra).await.unwrap();
        }
        tx.flush().await.unwrap();
        drop(tx);
        writer.finish().unwrap();
        // Der wartende Batch endet am Nutzdatenbudget (nicht an der Sammelzeit); die überzählige Seite folgt allein.
        assert_eq!(*pages.lock().unwrap(), vec![1, per_batch, 1]);
    }

    // spec: PERF-003, PERF-005
    #[tokio::test]
    async fn n15_byte_budget_flushes_extension_only_batches_without_loss() {
        let (s, log) = sink(Duration::from_millis(5), None);
        let writer = WriterThread::spawn(s);
        let tx = writer.sender();
        let part = BATCH_BYTES as usize / 3 + 1024;
        let sizes = [part, part, part, BATCH_BYTES as usize + 1];
        for (i, size) in sizes.into_iter().enumerate() {
            tx.send(WriteBatch {
                ext: vec![os_model::ExtWrite {
                    url: os_model::UrlId(i as i64),
                    key: os_model::ExtKey::from_static("crawl.test"),
                    value: os_model::Value::Text("x".repeat(size)),
                }],
                ..Default::default()
            })
            .await
            .unwrap();
        }
        tx.flush().await.unwrap();
        drop(tx);
        writer.finish().unwrap();
        let log = log.lock().unwrap();
        assert_eq!(
            log.batches.iter().map(|b| b.ext.len()).collect::<Vec<_>>(),
            vec![2, 1, 1]
        );
        let entries = log.batches.iter().flat_map(|b| &b.ext).collect::<Vec<_>>();
        for (i, entry) in entries.into_iter().enumerate() {
            assert_eq!(entry.url, os_model::UrlId(i as i64));
            let os_model::Value::Text(text) = &entry.value else {
                panic!("text expected")
            };
            assert_eq!(text.len(), sizes[i]);
            assert!(text.bytes().all(|b| b == b'x'));
        }
    }

    fn event(code: &str) -> WriteBatch {
        WriteBatch {
            events: vec![CrawlEvent {
                at: Timestamp::UNIX_EPOCH,
                kind: "info".into(),
                code: code.into(),
                message: String::new(),
            }],
            ..WriteBatch::default()
        }
    }

    // spec: PERF-005
    #[tokio::test]
    async fn messages_are_merged_in_order_and_flush_waits_for_the_write() {
        let (s, log) = sink(Duration::ZERO, None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        for code in ["a", "b", "c"] {
            tx.send(event(code)).await.unwrap();
        }
        tx.flush().await.unwrap();
        {
            let log = log.lock().unwrap();
            let codes: Vec<_> = log
                .batches
                .iter()
                .flat_map(|b| b.events.iter().map(|e| e.code.clone()))
                .collect();
            assert_eq!(codes, ["a", "b", "c"]);
        }
        drop(tx);
        let stats = w.finish().unwrap();
        assert!(stats.batches >= 1);
        assert!(log.lock().unwrap().checkpoints >= 1, "Abschluss-Checkpoint");
    }

    // spec: PERF-005
    // spec: UI-018
    // B4-LAGFIX2: Auf Anforderung schreibt ein untätiger Writer die gezählten Seiten sofort statt nach
    // BATCH_MAX_WAIT; bei wartenden Nachrichten wird nichts angefordert, und die schwache Seite hält den
    // Writer nach `finish` nicht offen.
    #[tokio::test]
    async fn an_idle_writer_writes_counted_pages_on_request_and_the_weak_side_keeps_nothing_open() {
        let (s, log) = sink(Duration::ZERO, None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        let weak = tx.downgrade();
        tx.send(page_batch(1)).await.unwrap();
        let start = Instant::now();
        while tx.tx.capacity() < tx.tx.max_capacity() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        assert!(weak.flush_if_idle().expect("idle writer").await);
        assert!(start.elapsed() < BATCH_MAX_WAIT, "{:?}", start.elapsed());
        assert_eq!(log.lock().unwrap().batches.len(), 1);
        drop(tx);
        let stats = w.finish().unwrap();
        assert_eq!(stats.pages, 1);
        assert!(weak.flush_if_idle().is_none(), "finished writer");

        let (s, log) = sink(Duration::from_millis(300), None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        let weak = tx.downgrade();
        tx.send(page_batch(BATCH_PAGES)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(page_batch(1)).await.unwrap();
        assert!(
            weak.flush_if_idle().is_none(),
            "a waiting message keeps the group commit"
        );
        tx.flush().await.unwrap();
        assert_eq!(log.lock().unwrap().batches.len(), 2);
        drop(tx);
        w.finish().unwrap();
    }

    #[tokio::test]
    async fn the_first_batch_is_cut_at_the_normal_page_limit_before_any_backlog() {
        let (s, log) = sink(Duration::ZERO, None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        for _ in 0..30 {
            tx.send(page_batch(20)).await.unwrap(); // 600 Seiten
        }
        tx.flush().await.unwrap();
        drop(tx);
        let stats = w.finish().unwrap();
        assert_eq!(stats.pages, 600);
        let log = log.lock().unwrap();
        // Noch kein Rückstau (weniger als ein Achtel des Kanals wartet): der erste Batch endet bei 200 Seiten.
        assert_eq!(log.batches[0].pages.len(), BATCH_PAGES);
        // Spätere Batches dürfen bei Rückstau größer werden, bleiben aber unter dem oberen Limit + einer Nachricht.
        assert!(stats.max_batch_pages <= BATCH_PAGES_BACKLOG + 20);
    }

    /// Batch mit `n` Seiten, deren Kennungen bei `first` beginnen (Reihenfolgeprüfung).
    fn numbered_batch(first: i64, n: usize) -> WriteBatch {
        let mut b = page_batch(n);
        for (i, p) in b.pages.iter_mut().enumerate() {
            p.url_id = os_model::UrlId(first + i as i64);
        }
        b
    }

    // spec: PERF-005
    #[tokio::test]
    async fn a_backlog_is_written_as_few_large_batches_in_order() {
        // Langsame Senke: die Worker (hier ein schneller Absender) liegen vorn, der Writer fasst zusammen.
        let (s, log) = sink(Duration::from_millis(40), None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        let total = 6_000usize;
        for i in 0..(total / 50) {
            tx.send(numbered_batch((i * 50) as i64 + 1, 50))
                .await
                .unwrap();
        }
        tx.flush().await.unwrap();
        drop(tx);
        let stats = w.finish().unwrap();
        assert_eq!(stats.pages, total as u64);
        let log = log.lock().unwrap();
        // Ohne Gruppen-Commit wären es mindestens total / BATCH_PAGES = 30 Batches.
        assert!(
            log.batches.len() <= 12,
            "{} Batches für {total} Seiten (Gruppen-Commit erwartet)",
            log.batches.len()
        );
        assert!(
            stats.max_batch_pages >= 4 * BATCH_PAGES,
            "größter Batch {} Seiten",
            stats.max_batch_pages
        );
        assert!(stats.max_batch_pages <= BATCH_PAGES_BACKLOG + 50);
        // Reihenfolge und Vollständigkeit über alle Batches
        let ids: Vec<i64> = log
            .batches
            .iter()
            .flat_map(|b| b.pages.iter().map(|p| p.url_id.0))
            .collect();
        assert_eq!(ids, (1..=total as i64).collect::<Vec<_>>());
    }

    // spec: PERF-005
    #[tokio::test]
    async fn a_backlog_remainder_is_written_without_flush_after_the_backlog_wait() {
        let (s, log) = sink(Duration::from_millis(30), None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        for i in 0..40 {
            tx.send(numbered_batch(i * 50 + 1, 50)).await.unwrap(); // 2 000 Seiten
        }
        let start = Instant::now();
        loop {
            let written: usize = log
                .lock()
                .unwrap()
                .batches
                .iter()
                .map(|b| b.pages.len())
                .sum();
            if written == 2_000 {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "nur {written} von 2 000 Seiten geschrieben"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // Spätestens nach der Sammelzeit des Gruppen-Commits plus dem Schreiben (mit Reserve für langsame Runner)
        assert!(start.elapsed() < BATCH_MAX_WAIT_BACKLOG + Duration::from_secs(3));
        drop(tx);
        w.finish().unwrap();
    }

    // Messzähler (M1-P33, Fable-Nacharbeit c), kein Spec-Tag: der größte Batch in Seiten und der Speicher des
    // größten Batches stehen im DIAG-Block. Die Prozess-Zähler teilen sich parallel laufende Tests: nur `>=`.
    #[tokio::test]
    async fn the_largest_batch_in_pages_is_counted_in_the_diag_block() {
        let (s, _log) = sink(Duration::ZERO, None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        tx.send(page_batch(37)).await.unwrap();
        tx.flush().await.unwrap();
        drop(tx);
        w.finish().unwrap();
        let snap = diag::snapshot();
        let get = |name: &str| snap.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
        assert!(get("w_pages_max").unwrap() >= 37);
        assert!(get("mem_writer_batch_bytes_max").unwrap() >= 37 * 100);
    }

    // spec: PERF-005
    #[tokio::test]
    async fn a_lone_message_is_written_after_the_maximum_wait() {
        let (s, log) = sink(Duration::ZERO, None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        tx.send(page_batch(1)).await.unwrap();
        let start = Instant::now();
        loop {
            if log.lock().unwrap().batches.len() == 1 {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "Batch wurde nicht geschrieben"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        drop(tx);
        w.finish().unwrap();
    }

    // spec: PERF-005, PERF-003
    #[tokio::test]
    async fn full_channel_blocks_the_sender_until_the_writer_catches_up() {
        // Eine langsame Senke und ein kleiner Kanal: `send` darf nur so weit vorauslaufen wie der Kanal groß ist.
        let (s, log) = sink(Duration::from_millis(30), None);
        let w = WriterThread::spawn_with(s, 4);
        let tx = w.sender();
        let sent = Arc::new(AtomicUsize::new(0));
        let producer = {
            let tx = tx.clone();
            let sent = Arc::clone(&sent);
            tokio::spawn(async move {
                for _ in 0..120 {
                    tx.send(page_batch(BATCH_PAGES)).await.unwrap();
                    sent.fetch_add(1, Ordering::SeqCst);
                }
            })
        };
        // Während der Produzent läuft: gesendet - geschrieben bleibt durch Kanal + Batch begrenzt.
        let mut max_ahead = 0usize;
        while !producer.is_finished() {
            let written: usize = log
                .lock()
                .unwrap()
                .batches
                .iter()
                .map(|b| b.pages.len() / BATCH_PAGES)
                .sum();
            let ahead = sent.load(Ordering::SeqCst).saturating_sub(written);
            max_ahead = max_ahead.max(ahead);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        producer.await.unwrap();
        tx.flush().await.unwrap();
        // Kanal (4) + sich ansammelnder Batch (bis BATCH_PAGES_BACKLOG Seiten) + gerade geschriebener Batch
        let per_batch = BATCH_PAGES_BACKLOG / BATCH_PAGES + 1;
        let bound = 4 + 2 * per_batch + 2;
        assert!(
            max_ahead <= bound,
            "Produzent lief {max_ahead} von 120 Nachrichten voraus (Kanal 4, Grenze {bound})"
        );
        assert!(
            max_ahead < 120,
            "ohne Gegendruck liefe er ungebremst voraus"
        );
        drop(tx);
        let stats = w.finish().unwrap();
        assert_eq!(stats.pages, 120 * BATCH_PAGES as u64);
    }

    // Infrastruktur (PERF-003), kein Spec-Tag.
    #[tokio::test]
    async fn a_failing_sink_is_reported_and_does_not_block_senders() {
        let (s, _log) = sink(Duration::ZERO, Some(0));
        let w = WriterThread::spawn_with(s, 2);
        let tx = w.sender();
        for _ in 0..20 {
            // Jeder Batch ist groß genug für einen eigenen Schreibvorgang.
            let _ = tx.send(page_batch(BATCH_PAGES)).await;
        }
        let flushed = tx.flush().await;
        assert!(flushed.is_err());
        assert!(tx.failed());
        assert_eq!(
            tx.error().as_deref().map(|e| e.contains("Testfehler")),
            Some(true)
        );
        drop(tx);
        assert!(w.finish().is_err());
    }

    // Infrastruktur (PERF-003), kein Spec-Tag.
    #[tokio::test]
    async fn reserved_slot_sends_without_waiting_and_keeps_order() {
        let (s, log) = sink(Duration::ZERO, None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        let slot = tx.reserve().await.unwrap();
        // Zwischen Reservierung und Senden darf ein anderer Absender dazwischenkommen – der Slot behält
        // seinen Platz im Kanal nur, wenn er vor dem anderen reserviert wurde.
        slot.send(event("erst"));
        tx.send(event("dann")).await.unwrap();
        tx.flush().await.unwrap();
        let codes: Vec<_> = log
            .lock()
            .unwrap()
            .batches
            .iter()
            .flat_map(|b| b.events.iter().map(|e| e.code.clone()))
            .collect();
        assert_eq!(codes, ["erst", "dann"]);
        drop(tx);
        w.finish().unwrap();
    }

    // spec: CRAWL-126
    #[tokio::test]
    async fn a_checkpoint_without_a_later_batch_skips_the_closing_checkpoint() {
        let (s, log) = sink(Duration::ZERO, None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        tx.send(event("end")).await.unwrap();
        w.durable().await.unwrap();
        w.checkpoint().await.unwrap();
        // Flush and durable write nothing new: the explicit checkpoint stays the last one.
        w.flush().await.unwrap();
        drop(tx);
        w.finish().unwrap();
        let log = log.lock().unwrap();
        assert_eq!(
            log.checkpoints, 1,
            "no closing checkpoint after the explicit one"
        );
        assert_eq!(log.batches.len(), 1);
    }

    // spec: CRAWL-126
    #[tokio::test]
    async fn a_batch_after_the_checkpoint_gets_the_closing_checkpoint() {
        let (s, log) = sink(Duration::ZERO, None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        tx.send(event("a")).await.unwrap();
        w.checkpoint().await.unwrap();
        tx.send(event("b")).await.unwrap();
        drop(tx);
        w.finish().unwrap();
        let log = log.lock().unwrap();
        assert_eq!(
            log.checkpoints, 2,
            "the batch after the checkpoint is checkpointed at the end"
        );
        let codes: Vec<_> = log
            .batches
            .iter()
            .flat_map(|b| b.events.iter().map(|e| e.code.clone()))
            .collect();
        assert_eq!(codes, ["a", "b"]);
    }

    // spec: CRAWL-126
    #[tokio::test]
    async fn without_an_explicit_checkpoint_the_closing_checkpoint_runs() {
        let (s, log) = sink(Duration::ZERO, None);
        let w = WriterThread::spawn(s);
        let tx = w.sender();
        tx.send(event("a")).await.unwrap();
        w.durable().await.unwrap();
        w.flush().await.unwrap();
        drop(tx);
        w.finish().unwrap();
        assert_eq!(log.lock().unwrap().checkpoints, 1);
    }
}

#[cfg(test)]
mod w1_contracts {
    use super::*;
    use os_model::*;

    // spec: CRAWL-062, CRAWL-063, CRAWL-130
    // Infrastruktur (M2-F05a); Fachprüfung M2-E01/E02/E05 und D-Pakete.
    #[test]
    fn extension_batches_are_nonempty_measured_and_merged_without_loss() {
        let url_id = UrlId(1);
        macro_rules! check {
            ($field:ident, $value:expr) => {{
                let part = WriteBatch {
                    $field: vec![$value],
                    ..Default::default()
                };
                assert!(!is_empty(&part), stringify!($field));
                assert!(estimate_bytes(&part) > 0, stringify!($field));
                let mut out = WriteBatch::default();
                merge(&mut out, part.clone());
                assert_eq!(out, part, stringify!($field));
                merge(&mut out, part.clone());
                let mut expected = part.clone();
                expected.$field.extend(part.$field);
                assert_eq!(out, expected, stringify!($field));
            }};
        }
        check!(
            cookies,
            CookieWrite {
                url_id,
                origin: CookieOrigin::Http,
                position: 0,
                name: "fixture".into(),
                value: "v".into(),
                domain: None,
                path: None,
                expires: None,
                max_age: None,
                secure: true,
                http_only: true,
                same_site: None,
                raw: "fixture=v".into()
            }
        );
        check!(
            discoveries,
            DiscoveryWrite {
                seq: 1,
                target: url_id,
                source: None,
                kind: DiscoveryKind::Link,
                link_type: None,
                anchor: None,
                depth: Some(0)
            }
        );
        check!(
            ext,
            ExtWrite {
                url: url_id,
                key: ExtKey::from_static("validation.checked"),
                value: Value::Bool(true)
            }
        );
        check!(
            structured_data,
            SdItemWrite {
                url_id,
                format: "json-ld".into(),
                type_path: "Thing".into(),
                json: "{}".into()
            }
        );
        check!(
            sd_issues,
            SdIssueWrite {
                url_id,
                item: None,
                validation: "parse".into(),
                feature: None,
                severity: "error".into(),
                property: None,
                code: "fixture".into()
            }
        );
        check!(
            render,
            RenderWrite {
                url_id,
                console_dropped: 2,
                status: "ok".into(),
                html: Some(b"<p>fixture</p>".to_vec()),
                word_count: Some(1),
                timings: Default::default(),
                console: vec![],
                blocked: vec![],
                network: vec![],
                screenshots: vec![]
            }
        );
        check!(
            a11y,
            A11yWrite {
                url_id,
                rules: vec![],
                nodes: vec![]
            }
        );
        check!(
            spelling,
            SpellingWrite {
                url_id,
                kind: "spelling".into(),
                language: "de".into(),
                section: "body".into(),
                text: "fixture".into(),
                detail: None,
                suggestion: None,
                rule_id: None
            }
        );
        check!(
            custom_search,
            CustomSearchWrite {
                url_id,
                def_index: 0,
                count: 1,
                error: None
            }
        );
        check!(
            custom_extraction,
            CustomExtractionWrite {
                url_id,
                def_index: 0,
                number: 0,
                value: "fixture".into(),
                error: None
            }
        );
        check!(
            custom_js,
            CustomJsWrite {
                url_id,
                def_index: 0,
                number: 0,
                value: Some("{}".into()),
                error: None
            }
        );
        check!(
            forms,
            FormWrite {
                url_id,
                idx: 0,
                action: None,
                method: "get".into()
            }
        );
        check!(minhash, (url_id, vec![1, 2, 3]));
        check!(
            api_values,
            ApiValueWrite {
                url_id: Some(url_id),
                url: "https://site.test/".into(),
                source: "ga4".into(),
                metric: "sessions".into(),
                level: "url".into(),
                v_real: Some(1.0),
                v_text: None,
                fetched_at: Timestamp::UNIX_EPOCH
            }
        );
        check!(meta, ("progress_snapshot".into(), "{}".into()));
    }
}
