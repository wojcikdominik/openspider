//! Gemeinsame Fixture der unabhängigen Abnahmetests M4-V01: eine Crawl-Datei mit Vorschlägen in allen Zuständen
//! (AI-070 Filter, AI-071/AI-072 Spalten, AI-076 Review, AI-020 Statuswerte, OUT-016 Formelwerte).
//!
//! Eingebunden per `#[path]` aus den Testzielen von os-store, os-analyze, os-export und os-app; nutzt nur `os_store`
//! und `os_model`. Die Werte sind frei erfunden (Host `ai.test:8080`, 08 Konventionen).
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;

use os_store::ai::suggestions::{
    self as store, AcceptedValue, ElementRecord, Origin, ReviewStatus, StoredCandidate,
};

pub const HOST: &str = "http://ai.test:8080";
pub const MODEL: &str = "mock-claude";
pub const GENERATED_AT: &str = "2026-11-27T09:30:00Z";
pub const ACCEPTED_AT: &str = "2026-11-27T10:15:00Z";

/// Eine Seite: Pfad, aktueller Title, Title-Pixelbreite, aktuelle Meta Description, Pixelbreite.
pub struct Row {
    pub path: &'static str,
    pub title: Option<&'static str>,
    pub title_px: Option<u32>,
    pub meta: Option<&'static str>,
    pub meta_px: Option<u32>,
}

/// Seiten in ID-Reihenfolge (ID = Position + 1).
pub const ROWS: [Row; 13] = [
    Row { path: "/valid-open/", title: Some("Startseite"), title_px: Some(99), meta: Some("Alte Beschreibung der Startseite"), meta_px: Some(222) },
    Row { path: "/accepted/", title: Some("Wanderschuhe"), title_px: Some(120), meta: None, meta_px: None },
    Row { path: "/edited/", title: Some("Rucksäcke"), title_px: Some(95), meta: None, meta_px: None },
    Row { path: "/rejected/", title: Some("Zelte"), title_px: Some(55), meta: None, meta_px: None },
    Row { path: "/novalid/", title: Some("Schlafsäcke"), title_px: Some(110), meta: None, meta_px: None },
    Row { path: "/pending/", title: Some("Kocher"), title_px: Some(66), meta: None, meta_px: None },
    Row { path: "/error/", title: Some("Lampen"), title_px: Some(70), meta: None, meta_px: None },
    Row { path: "/budget/", title: Some("Stöcke"), title_px: Some(64), meta: None, meta_px: None },
    Row { path: "/refusal/", title: Some("Karten"), title_px: Some(68), meta: None, meta_px: None },
    Row { path: "/skipped/", title: Some("Kompass"), title_px: Some(80), meta: None, meta_px: None },
    Row { path: "/meta-only/", title: Some("Nur Meta"), title_px: Some(81), meta: None, meta_px: None },
    Row { path: "/no-suggestion/", title: Some("Ohne Vorschlag"), title_px: Some(140), meta: None, meta_px: None },
    Row { path: "/formula/", title: Some("=HYPERLINK(\"http://evil\")"), title_px: Some(250), meta: None, meta_px: None },
];

pub fn address(path: &str) -> String {
    format!("{HOST}{path}")
}

pub fn id(path: &str) -> i64 {
    ROWS.iter().position(|r| r.path == path).expect("Pfad der Fixture") as i64 + 1
}

/// Adressen zu Pfaden (sortiert).
pub fn addresses(paths: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = paths.iter().map(|p| address(p)).collect();
    v.sort();
    v
}

pub fn cand(index: u32, value: &str, px: u32, violations: &[&str], round: u32) -> StoredCandidate {
    StoredCandidate {
        index,
        value: value.to_string(),
        length: value.chars().count() as u32,
        pixel_width: Some(px),
        valid: violations.is_empty(),
        violations: violations.iter().map(|s| s.to_string()).collect(),
        round,
    }
}

fn record(path: &str, element: &str, status: &str) -> ElementRecord {
    let mut r = ElementRecord::pending(id(path), element);
    r.status = status.to_string();
    r.model = Some(MODEL.to_string());
    r.run_id = Some(1);
    r.generated_at = Some(GENERATED_AT.to_string());
    r.rounds = 1;
    r
}

fn with(mut r: ElementRecord, candidates: Vec<StoredCandidate>, selected: Option<u32>) -> ElementRecord {
    let visible = selected.or_else(|| candidates.first().map(|c| c.index));
    r.order = visible.into_iter().collect();
    r.violations = visible
        .and_then(|k| candidates.iter().find(|c| c.index == k))
        .map(|c| c.violations.clone())
        .unwrap_or_default();
    r.selected = selected;
    r.candidates = candidates;
    r
}

fn accept(mut r: ElementRecord, review: ReviewStatus, origin: Origin, value: &str, px: u32, violations: &[&str]) -> ElementRecord {
    r.review = review;
    r.accepted = Some(AcceptedValue {
        value: value.to_string(),
        length: value.chars().count() as u32,
        pixel_width: Some(px),
        violations: violations.iter().map(|s| s.to_string()).collect(),
        origin,
        candidate: if origin == Origin::Ai { r.selected } else { None },
        at: ACCEPTED_AT.to_string(),
    });
    r
}

pub const V_OPEN_1: &str = "Startseite für Wanderausrüstung und Zubehör";
pub const V_OPEN_2: &str = "Wanderausrüstung online kaufen – Startseite";
pub const V_ACC_INVALID: &str = "Wanderschuhe für Damen und Herren im großen Test mit Kaufberatung";
pub const V_ACC: &str = "Wanderschuhe für Damen und Herren im Test";
pub const V_EDITED: &str = "=SUM(A1) Rucksäcke für jede Tour – zu lang für einen guten Title";
pub const V_REJ: &str = "Zelte für Camping und Trekking im Überblick";
pub const V_NOVALID_1: &str = "Schlafsäcke für jede Jahreszeit und jedes Budget im großen Test";
pub const V_NOVALID_2: &str = "Schlafsäcke für Sommer, Winter und Übergang: Modelle und Tipps 2026";
pub const V_META: &str = "Entdecken Sie unsere Auswahl an Wanderausrüstung: Schuhe, Rucksäcke und Zelte für jede Tour.";
pub const V_META_ONLY: &str = "Nur eine Meta Description für diese Seite mit genug Text für die Mindestlänge.";
pub const V_FORMULA: &str = "+Angebote für Outdoor-Ausrüstung im Überblick";
pub const V_FORMULA_2: &str = "@Outdoor Angebote und Rabatte der Woche";
pub const V_FORMULA_3: &str = "-50 % auf Outdoor-Ausrüstung in dieser Woche";

/// Alle Elementeinträge der Fixture.
pub fn records() -> Vec<ElementRecord> {
    let t = store::ELEMENT_TITLE;
    let m = store::ELEMENT_META_DESCRIPTION;
    vec![
        with(record("/valid-open/", t, "Valid"), vec![cand(1, V_OPEN_1, 401, &[], 1), cand(2, V_OPEN_2, 399, &[], 1)], Some(1)),
        with(record("/valid-open/", m, "Valid"), vec![cand(1, V_META, 610, &[], 1)], Some(1)),
        accept(
            with(record("/accepted/", t, "Valid"), vec![cand(1, V_ACC_INVALID, 590, &["length>60", "pixel_width>561"], 1), cand(2, V_ACC, 380, &[], 1)], Some(2)),
            ReviewStatus::Accepted, Origin::Ai, V_ACC, 380, &[],
        ),
        accept(
            with(record("/edited/", t, "Valid"), vec![cand(1, "Rucksäcke für jede Tour im großen Vergleich", 410, &[], 1)], Some(1)),
            ReviewStatus::Edited, Origin::Edited, V_EDITED, 600, &["length>60", "pixel_width>561"],
        ),
        {
            let mut r = with(record("/rejected/", t, "Valid"), vec![cand(1, V_REJ, 390, &[], 1)], Some(1));
            r.review = ReviewStatus::Rejected;
            r
        },
        {
            let mut r = with(
                record("/novalid/", t, "No valid suggestion"),
                vec![cand(1, V_NOVALID_1, 575, &["length>60", "pixel_width>561"], 1), cand(2, V_NOVALID_2, 560, &["length>60"], 2)],
                None,
            );
            r.rounds = 3;
            // sichtbar: der mit den wenigsten Verstößen
            r.order = vec![2];
            r.violations = vec!["length>60".into()];
            r
        },
        record("/pending/", t, "Pending"),
        record("/error/", t, "Error: 500 Internal Server Error"),
        record("/budget/", t, "Budget exceeded"),
        record("/refusal/", t, "Refusal"),
        record("/skipped/", t, "Skipped (no capable model)"),
        with(record("/meta-only/", m, "Valid"), vec![cand(1, V_META_ONLY, 540, &[], 1)], Some(1)),
        accept(
            with(record("/formula/", t, "Valid"), vec![cand(1, V_FORMULA, 420, &[], 1), cand(2, V_FORMULA_2, 380, &[], 1), cand(3, V_FORMULA_3, 400, &[], 1)], Some(1)),
            ReviewStatus::Accepted, Origin::Ai, V_FORMULA, 420, &[],
        ),
    ]
}

fn q(v: Option<&str>) -> String {
    match v {
        Some(s) => format!("'{}'", s.replace('\'', "''")),
        None => "NULL".into(),
    }
}

fn n(v: Option<u32>) -> String {
    v.map_or("NULL".into(), |x| x.to_string())
}

/// Seiten (`urls`) und einen KI-Lauf (ID 1) in eine leere Crawl-Datei schreiben, dann alle Einträge.
pub fn seed(conn: &store::Connection) {
    let mut sql = String::new();
    for (i, r) in ROWS.iter().enumerate() {
        let a = address(r.path);
        sql.push_str(&format!(
            "INSERT INTO urls (id, url, address, origin, is_internal, state, discovered_via, folder_depth, url_length, \
             status_code, content_class, in_tables, indexability, title_1, title_1_px, meta_desc_1, meta_desc_1_px, h1_1) \
             VALUES ({id}, {a}, {a}, '{HOST}', 1, 'done', 'link', 1, {len}, 200, 'HTML', 1, 'Indexable', {t}, {tp}, {m}, {mp}, 'H1');\n",
            id = i + 1,
            a = q(Some(&a)),
            len = a.chars().count(),
            t = q(r.title),
            tp = n(r.title_px),
            m = q(r.meta),
            mp = n(r.meta_px),
        ));
    }
    sql.push_str("INSERT INTO ai_runs(id, started_at, config_json) VALUES (1, '2026-11-27T09:00:00Z', '{}');\n");
    conn.execute_batch(&sql).unwrap();
    store::save_all(conn, &records()).unwrap();
}

/// Neue Crawl-Datei `name` in `dir` (Zustand `Completed`) mit der Fixture.
pub fn create(dir: &Path, name: &str, config: os_model::CrawlConfig) -> os_store::CrawlFile {
    let created = os_model::Timestamp::from_db("2026-11-27T09:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: os_model::IdSource::new_crawl_id(&os_model::SeededIds::new(7171), created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some(format!("{HOST}/")),
        list_source: None,
        project: "Abnahme V01".into(),
        name: "V01 Zustände".into(),
        config,
        created_at: created,
    };
    let f = os_store::CrawlFile::create(&dir.join(name), &init, os_store::StorageProfile::Balanced).unwrap();
    seed(f.conn());
    f.set_meta("state", "Completed").unwrap();
    f
}

/// Erwartete Zeilen je Filter des Tabs „AI Suggestions“ (Pfade). `Open` nur mit den eindeutigen Fällen (siehe Bericht).
pub fn expected_filters() -> BTreeMap<&'static str, Vec<&'static str>> {
    BTreeMap::from([
        ("All", vec!["/valid-open/", "/accepted/", "/edited/", "/rejected/", "/novalid/", "/pending/", "/error/", "/budget/", "/refusal/", "/skipped/", "/meta-only/", "/formula/"]),
        ("Titles", vec!["/valid-open/", "/accepted/", "/edited/", "/rejected/", "/novalid/", "/pending/", "/error/", "/budget/", "/refusal/", "/skipped/", "/formula/"]),
        ("Meta Descriptions", vec!["/valid-open/", "/meta-only/"]),
        ("H1", vec![]),
        ("Alt Text", vec![]),
        ("No Valid Suggestion", vec!["/novalid/"]),
        ("Accepted", vec!["/accepted/", "/edited/", "/formula/"]),
        ("Rejected", vec!["/rejected/"]),
        ("Errors", vec!["/error/", "/budget/"]),
    ])
}
