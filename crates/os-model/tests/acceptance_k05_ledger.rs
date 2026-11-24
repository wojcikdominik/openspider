//! Unabhängige Abnahmetests M4-K05: gemeinsames Crawl-Budget `BudgetLedger` (AI-013).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-013, „Konventionen“: Mikro-USD als Ganzzahl), dem
//! Paketauftrag M4-K05 (`prompt_notes`: Standardwerte 5,00 USD / 80 %, Ledger ohne Kenntnis von `os-ai`, nie
//! überschritten auch bei parallelen Workern) und den Orchestrator-Auslegungen (a) Überschreitung einer
//! Reservierung sperrt crawl-weit weitere bezahlte Reservierungen bis zur Budgeterhöhung, Beträge 0 laufen weiter.
// acceptance: independent

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use os_model::budget::{
    BudgetError, BudgetLedger, BudgetLevel, BudgetLimits, BudgetScope, CrawlBudgetLedger, MicroUsd,
    SpendSource, AREA_AI,
};

fn usd(micro: u64) -> MicroUsd {
    MicroUsd(micro)
}

fn crawl_only(limit: u64) -> BudgetLimits {
    BudgetLimits {
        crawl: usd(limit),
        unlimited: false,
        areas: BTreeMap::new(),
        tasks: BTreeMap::new(),
        warning_percent: 80,
    }
}

fn ai() -> BudgetScope {
    BudgetScope::area(AREA_AI)
}

fn ai_task(task: &str) -> BudgetScope {
    BudgetScope::task(AREA_AI, task)
}

fn is_exceeded(r: &Result<os_model::budget::Reservation, BudgetError>) -> bool {
    matches!(r, Err(BudgetError::Exceeded(_)))
}

// spec: AI-013
// acceptance: independent
// Standardwerte (E-5 a): Crawl-Budget 5,00 USD, Warnschwelle 80 %, nicht unbegrenzt.
#[test]
fn defaults_are_five_usd_and_eighty_percent() {
    let d = BudgetLimits::default();
    assert_eq!(d.crawl, usd(5_000_000));
    assert_eq!(d.warning_percent, 80);
    assert!(!d.unlimited, "„Unbegrenzt“ nur über ausdrückliche Checkbox");
    assert!(
        d.areas.is_empty(),
        "Teilbudget „KI gesamt“ leer = kein Teilbudget"
    );
    assert!(d.tasks.is_empty());
}

// spec: AI-013
// acceptance: independent
// Mikro-USD: 1 USD = 1.000.000 µUSD.
#[test]
fn micro_usd_unit() {
    assert_eq!(MicroUsd::USD, 1_000_000);
}

// spec: AI-013
// acceptance: independent
// Crawl-Budget 0,010 USD, Reservierung 0,004 USD je Request → genau 2 Reservierungen gehen durch, die dritte nicht.
#[test]
fn crawl_budget_ten_milli_allows_exactly_two_of_four_milli() {
    let ledger = CrawlBudgetLedger::new(crawl_only(10_000));
    let mut sent = 0;
    let mut refused = 0;
    for _ in 0..10 {
        match ledger.reserve(ai_task("decision"), usd(4_000)) {
            Ok(r) => {
                sent += 1;
                // Mock kostet je Request 0,004 USD.
                let s = ledger.settle(r.id, usd(4_000)).unwrap();
                assert!(!s.over_reservation);
            }
            Err(BudgetError::Exceeded(level)) => {
                refused += 1;
                assert!(
                    level.stops_whole_area(),
                    "Crawl-Budget trifft alle KI-Einträge"
                );
            }
            Err(e) => panic!("unerwarteter Fehler {e}"),
        }
    }
    assert_eq!(sent, 2);
    assert_eq!(refused, 8);
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.spent, usd(8_000));
    assert_eq!(snap.reserved, usd(0));
    assert_eq!(snap.remaining, Some(usd(2_000)));
}

// spec: AI-013
// acceptance: independent
// Dieselbe Grenze, wenn alle Reservierungen gleichzeitig offen sind (parallele Worker vor der Antwort).
#[test]
fn open_reservations_count_against_budget() {
    let ledger = CrawlBudgetLedger::new(crawl_only(10_000));
    let a = ledger.reserve(ai(), usd(4_000)).unwrap();
    let b = ledger.reserve(ai(), usd(4_000)).unwrap();
    let c = ledger.reserve(ai(), usd(4_000));
    assert!(is_exceeded(&c), "dritte Reservierung passt nicht: {c:?}");
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.reserved, usd(8_000));
    assert_eq!(
        snap.consumption(),
        usd(8_000),
        "Verbrauch = Protokolle + offene Reservierungen"
    );
    assert_ne!(a.id, b.id);
}

// spec: AI-013
// acceptance: independent
// Teilbudget „KI gesamt“ 0,008 USD bei Crawl-Budget 1,00 USD, je Request 0,004 USD → genau 2 KI-Requests.
#[test]
fn ai_area_budget_allows_exactly_two() {
    let mut limits = crawl_only(1_000_000);
    limits.areas.insert(AREA_AI.to_string(), usd(8_000));
    let ledger = CrawlBudgetLedger::new(limits);
    let mut sent = 0;
    let mut last_err = None;
    for task in [
        "decision",
        "suggest-title",
        "suggest-meta",
        "decision",
        "embedding",
    ] {
        match ledger.reserve(ai_task(task), usd(4_000)) {
            Ok(r) => {
                ledger.settle(r.id, usd(4_000)).unwrap();
                sent += 1;
            }
            Err(e) => last_err = Some(e),
        }
    }
    assert_eq!(sent, 2);
    match last_err {
        Some(BudgetError::Exceeded(level)) => {
            assert_eq!(level, BudgetLevel::Area(AREA_AI.to_string()));
            assert!(
                level.stops_whole_area(),
                "„KI gesamt“ erreicht → alle offenen KI-Einträge"
            );
        }
        other => panic!("erwartet Exceeded(Area), war {other:?}"),
    }
}

// spec: AI-013
// acceptance: independent
// Teilbudget `suggest-title = 0,50 USD` stoppt nur Title-Vorschläge; Decisions laufen bis zum Crawl-Budget weiter.
#[test]
fn task_budget_stops_only_that_task() {
    let mut limits = crawl_only(1_000_000);
    limits.tasks.insert(
        AREA_AI.to_string(),
        BTreeMap::from([("suggest-title".to_string(), usd(500_000))]),
    );
    let ledger = CrawlBudgetLedger::new(limits);
    let mut titles = 0;
    loop {
        match ledger.reserve(ai_task("suggest-title"), usd(200_000)) {
            Ok(r) => {
                ledger.settle(r.id, usd(200_000)).unwrap();
                titles += 1;
            }
            Err(BudgetError::Exceeded(level)) => {
                assert_eq!(
                    level,
                    BudgetLevel::Task {
                        area: AREA_AI.to_string(),
                        task: "suggest-title".to_string()
                    }
                );
                assert!(
                    !level.stops_whole_area(),
                    "Aufgaben-Teilbudget trifft nur die Einträge dieser Aufgabe"
                );
                break;
            }
            Err(e) => panic!("{e}"),
        }
        assert!(titles < 10);
    }
    assert_eq!(titles, 2, "2 × 0,20 = 0,40 ≤ 0,50; 3 × 0,20 = 0,60 > 0,50");
    // Decisions laufen weiter, bis das Crawl-Budget (1,00 USD, davon 0,40 verbraucht) erreicht ist.
    let mut decisions = 0;
    loop {
        match ledger.reserve(ai_task("decision"), usd(200_000)) {
            Ok(r) => {
                ledger.settle(r.id, usd(200_000)).unwrap();
                decisions += 1;
            }
            Err(BudgetError::Exceeded(level)) => {
                assert_eq!(level, BudgetLevel::Crawl);
                break;
            }
            Err(e) => panic!("{e}"),
        }
        assert!(decisions < 10);
    }
    assert_eq!(decisions, 3);
    assert_eq!(ledger.snapshot().unwrap().spent, usd(1_000_000));
}

// spec: AI-013
// acceptance: independent
// Budget 0 = keine kostenpflichtigen Aufrufe; nur Beträge 0 (Ollama, Cache-Treffer) laufen.
#[test]
fn zero_budget_allows_only_zero_amounts() {
    let ledger = CrawlBudgetLedger::new(crawl_only(0));
    assert!(is_exceeded(&ledger.reserve(ai_task("decision"), usd(1))));
    assert!(is_exceeded(&ledger.reserve(ai(), usd(4_000))));
    for _ in 0..50 {
        let r = ledger
            .reserve(ai_task("suggest-alt"), usd(0))
            .expect("Ollama/Cache-Treffer mit Betrag 0 läuft auch bei Budget 0");
        ledger.settle(r.id, usd(0)).unwrap();
    }
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.spent, usd(0));
    assert!(!snap.halted);
}

// spec: AI-013
// acceptance: independent
// „Unbegrenzt“ nur ausdrücklich: dann blockiert das Crawl-Budget nicht, Teilbudgets wirken weiter.
#[test]
fn unlimited_ignores_crawl_budget_but_keeps_sub_budgets() {
    let mut limits = crawl_only(0);
    limits.unlimited = true;
    limits.areas.insert(AREA_AI.to_string(), usd(8_000));
    let ledger = CrawlBudgetLedger::new(limits);
    let a = ledger.reserve(ai(), usd(4_000)).unwrap();
    ledger.settle(a.id, usd(4_000)).unwrap();
    let b = ledger.reserve(ai(), usd(4_000)).unwrap();
    ledger.settle(b.id, usd(4_000)).unwrap();
    assert!(is_exceeded(&ledger.reserve(ai(), usd(1))));
}

// spec: AI-013
// acceptance: independent
// Freigeben einer Reservierung (nichts gesendet) gibt den Betrag zurück.
#[test]
fn release_returns_amount() {
    let ledger = CrawlBudgetLedger::new(crawl_only(10_000));
    let a = ledger.reserve(ai(), usd(6_000)).unwrap();
    assert_eq!(ledger.snapshot().unwrap().remaining, Some(usd(4_000)));
    assert!(is_exceeded(&ledger.reserve(ai(), usd(6_000))));
    ledger.release(a.id).unwrap();
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.reserved, usd(0));
    assert_eq!(snap.spent, usd(0));
    assert_eq!(snap.remaining, Some(usd(10_000)));
    // Betrag wieder verfügbar.
    let b = ledger.reserve(ai(), usd(10_000)).unwrap();
    // Freigegebene Reservierung kann nicht mehr abgerechnet werden.
    assert!(ledger.settle(a.id, usd(1)).is_err());
    ledger.settle(b.id, usd(9_000)).unwrap();
    assert_eq!(ledger.snapshot().unwrap().spent, usd(9_000));
}

// spec: AI-013
// acceptance: independent
// Nach der Antwort ersetzt der tatsächliche Betrag die Reservierung.
#[test]
fn settle_replaces_reservation_with_actual() {
    let ledger = CrawlBudgetLedger::new(crawl_only(10_000));
    let a = ledger.reserve(ai(), usd(4_000)).unwrap();
    let s = ledger.settle(a.id, usd(1_500)).unwrap();
    assert_eq!(s.actual, usd(1_500));
    assert!(!s.over_reservation);
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.spent, usd(1_500));
    assert_eq!(snap.reserved, usd(0));
    assert_eq!(snap.remaining, Some(usd(8_500)));
    // Genau gleich der Reservierung ist keine Überschreitung.
    let b = ledger.reserve(ai(), usd(4_000)).unwrap();
    assert!(!ledger.settle(b.id, usd(4_000)).unwrap().over_reservation);
    assert!(!ledger.snapshot().unwrap().halted);
}

// spec: AI-013
// acceptance: independent
// Überschreitet eine Ist-Antwort ihre Reservierung: Mehrbetrag wird verbucht, `over_reservation`, danach nimmt der
// Ledger crawl-weit keine bezahlte Reservierung mehr an (auch andere Aufgaben), bis das Budget erhöht wird;
// Beträge 0 laufen weiter. Auslegung (a) des Orchestrators.
#[test]
fn over_reservation_books_excess_and_halts_paid_calls_until_raised() {
    let ledger = CrawlBudgetLedger::new(crawl_only(1_000_000));
    let in_flight = ledger.reserve(ai_task("suggest-meta"), usd(4_000)).unwrap();
    let a = ledger.reserve(ai_task("decision"), usd(4_000)).unwrap();
    let s = ledger.settle(a.id, usd(9_000)).unwrap();
    assert!(s.over_reservation);
    assert_eq!(s.actual, usd(9_000), "Mehrbetrag wird verbucht");
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.spent, usd(9_000));
    assert!(snap.halted);

    for scope in [
        ai(),
        ai_task("decision"),
        ai_task("suggest-title"),
        BudgetScope::area("dataforseo"),
    ] {
        let r = ledger.reserve(scope.clone(), usd(1));
        assert!(
            is_exceeded(&r),
            "{scope:?}: keine bezahlte Reservierung mehr, war {r:?}"
        );
        if let Err(BudgetError::Exceeded(level)) = r {
            assert!(
                level.stops_whole_area(),
                "crawl-weit, nicht nur die Aufgabe"
            );
        }
    }
    // Beträge 0 (Ollama, Cache-Treffer) laufen weiter.
    let zero = ledger.reserve(ai_task("decision"), usd(0)).unwrap();
    ledger.settle(zero.id, usd(0)).unwrap();
    // Bereits laufende Anfrage wird noch abgerechnet.
    ledger.settle(in_flight.id, usd(3_000)).unwrap();
    assert_eq!(ledger.snapshot().unwrap().spent, usd(12_000));

    // „Budget erhöhen und fortsetzen“.
    ledger.set_limits(crawl_only(2_000_000)).unwrap();
    let again = ledger.reserve(ai_task("decision"), usd(4_000)).unwrap();
    ledger.settle(again.id, usd(4_000)).unwrap();
    let snap = ledger.snapshot().unwrap();
    assert!(!snap.halted);
    assert_eq!(snap.spent, usd(16_000));
}

// spec: AI-013
// acceptance: independent
// Warnschwelle Standard 80 % des Crawl-Budgets.
#[test]
fn warning_at_eighty_percent() {
    let ledger = CrawlBudgetLedger::new(BudgetLimits::default());
    let a = ledger.reserve(ai(), usd(3_900_000)).unwrap();
    ledger.settle(a.id, usd(3_900_000)).unwrap();
    assert!(!ledger.snapshot().unwrap().warning, "78 % < 80 %");
    let b = ledger.reserve(ai(), usd(100_000)).unwrap();
    ledger.settle(b.id, usd(100_000)).unwrap();
    assert!(ledger.snapshot().unwrap().warning, "80 % erreicht");
}

// spec: AI-013
// acceptance: independent
// Verbrauch aus den Protokollen der Bereiche (KI: `ai_requests`) gilt nach dem Öffnen weiter; der Ledger kennt nur
// die Quelle, nicht `os-ai`. Ein zweiter Bereich (ab M5 DataForSEO) zählt in das gemeinsame Crawl-Budget.
#[test]
fn reopened_ledger_counts_previous_spend_of_all_areas() {
    struct Fixed(&'static str, BTreeMap<String, MicroUsd>);
    impl SpendSource for Fixed {
        fn area(&self) -> &str {
            self.0
        }
        fn spent_by_task(&self) -> Result<BTreeMap<String, MicroUsd>, BudgetError> {
            Ok(self.1.clone())
        }
    }
    let ai_log = Fixed(
        AREA_AI,
        BTreeMap::from([
            ("decision".to_string(), usd(150_000)),
            ("suggest-title".to_string(), usd(100_000)),
        ]),
    );
    let dfs_log = Fixed(
        "dataforseo",
        BTreeMap::from([("serp".to_string(), usd(700_000))]),
    );
    let ledger = CrawlBudgetLedger::open(crawl_only(1_000_000), &[&ai_log, &dfs_log]).unwrap();
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.spent, usd(950_000));
    assert_eq!(snap.area_spent(AREA_AI), usd(250_000));
    assert_eq!(snap.remaining, Some(usd(50_000)));
    assert!(is_exceeded(&ledger.reserve(ai(), usd(60_000))));
    let ok = ledger.reserve(ai(), usd(40_000)).unwrap();
    ledger.release(ok.id).unwrap();
}

/// Einfacher deterministischer Zufall (LCG), damit die Lasttests reproduzierbar sind.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
}

/// 4 Worker, 200 Einträge: je Eintrag Reservierung, „Request“, Abrechnung mit Ist ≤ Reservierung
/// (bzw. Freigabe, wenn nicht gesendet). Liefert (Summe Ist, Anzahl gesendet).
fn load_run(ledger: Arc<dyn BudgetLedger>, seed: u64, max_res: u64) -> (u64, u64) {
    const ENTRIES: usize = 200;
    let mut rng = Lcg(seed);
    // Je Eintrag: Aufgabe, Reservierung, Ist (≤ Reservierung), wird nicht gesendet (Freigabe)?
    let plan: Arc<Vec<(String, u64, u64, bool)>> = Arc::new(
        (0..ENTRIES)
            .map(|_| {
                let task = ["decision", "suggest-title", "suggest-meta", "embedding"]
                    [rng.range(0, 3) as usize]
                    .to_string();
                let res = rng.range(1, max_res);
                let actual = rng.range(0, res);
                let release = rng.range(0, 9) == 0;
                (task, res, actual, release)
            })
            .collect(),
    );
    let next = Arc::new(AtomicUsize::new(0));
    let total = Arc::new(AtomicU64::new(0));
    let sent = Arc::new(AtomicU64::new(0));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let (ledger, plan, next, total, sent) = (
                ledger.clone(),
                plan.clone(),
                next.clone(),
                total.clone(),
                sent.clone(),
            );
            thread::spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= plan.len() {
                    break;
                }
                let (task, res, actual, release) = &plan[i];
                match ledger.reserve(BudgetScope::task(AREA_AI, task), MicroUsd(*res)) {
                    Ok(r) => {
                        thread::yield_now();
                        if *release {
                            ledger.release(r.id).unwrap();
                        } else {
                            let s = ledger.settle(r.id, MicroUsd(*actual)).unwrap();
                            assert!(!s.over_reservation);
                            total.fetch_add(*actual, Ordering::SeqCst);
                            sent.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    Err(BudgetError::Exceeded(_)) => {}
                    Err(e) => panic!("{e}"),
                }
            })
        })
        .collect();
    for w in workers {
        w.join().unwrap();
    }
    (total.load(Ordering::SeqCst), sent.load(Ordering::SeqCst))
}

// spec: AI-013
// acceptance: independent
// Bei 4 gleichzeitigen Workern wird das Budget nie überschritten: Summe der Ist-Kosten ≤ Budget, sofern jede
// Ist-Antwort ≤ ihrer Reservierung ist (Lasttest mit 200 Einträgen; mehrere Budgets und Startwerte).
#[test]
fn parallel_workers_never_exceed_crawl_budget() {
    let budgets = [0, 1, 3_999, 10_000, 54_321, 123_457, 400_000, 1_000_000];
    for (bi, budget) in budgets.into_iter().enumerate() {
        for seed in 0..6u64 {
            for max_res in [4_000u64, 25_000] {
                let ledger: Arc<dyn BudgetLedger> =
                    Arc::new(CrawlBudgetLedger::new(crawl_only(budget)));
                let (total, sent) = load_run(
                    ledger.clone(),
                    seed * 7_919 + bi as u64 * 104_729 + max_res,
                    max_res,
                );
                assert!(
                    total <= budget,
                    "Budget {budget}, Seed {seed}, max {max_res}: Ist-Summe {total} > Budget"
                );
                let snap = ledger.snapshot().unwrap();
                assert_eq!(snap.spent, MicroUsd(total), "Ledger-Verbrauch = Summe Ist");
                assert_eq!(
                    snap.reserved,
                    MicroUsd(0),
                    "keine Reservierung bleibt offen"
                );
                if budget == 0 {
                    assert_eq!(sent, 0);
                }
            }
        }
    }
}

// spec: AI-013
// acceptance: independent
// Lasttest auch gegen Teilbudgets: „KI gesamt“ und ein Aufgaben-Teilbudget werden von parallelen Workern nicht
// überschritten.
#[test]
fn parallel_workers_never_exceed_sub_budgets() {
    for seed in 0..8u64 {
        let mut limits = crawl_only(1_000_000);
        limits.areas.insert(AREA_AI.to_string(), usd(77_777));
        limits.tasks.insert(
            AREA_AI.to_string(),
            BTreeMap::from([("suggest-title".to_string(), usd(20_001))]),
        );
        let ledger = Arc::new(CrawlBudgetLedger::new(limits));
        let (total, _) = load_run(ledger.clone(), 31 + seed, 6_000);
        assert!(total <= 77_777, "Seed {seed}: KI gesamt {total} > 77.777");
        let snap = ledger.snapshot().unwrap();
        let title = snap
            .areas
            .get(AREA_AI)
            .and_then(|a| a.tasks.get("suggest-title"))
            .map(|t| t.spent)
            .unwrap_or_default();
        assert!(
            title <= usd(20_001),
            "Seed {seed}: suggest-title {title:?} > 20.001"
        );
    }
}
