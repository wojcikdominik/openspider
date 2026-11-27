//! Unabhängige Abnahmetests M4-V01 (1.1-Etappe): Tab „AI Suggestions“ – Filter, Spalten, Sortierung, Zähler.
//!
//! Erwartungen aus `docs/spec/08-ai.md` AI-070 (Filter `Titles`, `Meta Descriptions`, `H1`, `Alt Text`,
//! `No Valid Suggestion`, `Accepted`, `Open`, `Rejected`), AI-071/AI-072 (Spalten, Präfix `Meta Description …`,
//! Basisspalten), AI-076 (`Review Status` ∈ {Open, Accepted, Edited, Rejected}), AI-020 (Filter „Errors“ = `Error: …`,
//! `Quota exceeded`, `Budget exceeded`; nicht `Skipped (…)` und `Refusal`) und `docs/spec/05-ui.md` UI-004 [M4].
//! Festlegung des Orchestrators: „Kandidat 1“ = gewählter Kandidat.
// acceptance: independent

#[path = "../../../tests/fixtures/ai/v01-acceptance/states.rs"]
mod states;

use os_analyze::defs::SortKey;
use os_analyze::views::{filter_counts, query_view, ColumnSelection, ViewQuery};
use os_model::{CrawlConfig, Value};
use states::*;

const TAB: &str = "AI Suggestions";

fn fixture() -> (tempfile::TempDir, os_store::CrawlFile) {
    let dir = tempfile::tempdir().unwrap();
    let f = create(dir.path(), "tab.ospider", CrawlConfig::default());
    (dir, f)
}

fn query(f: &os_store::CrawlFile, filter: &str, sort: Vec<SortKey>) -> (Vec<String>, Vec<Vec<Value>>) {
    let mut q = ViewQuery::new(TAB, filter);
    q.columns = ColumnSelection::All;
    q.sort = sort;
    q.limit = 500;
    let page = query_view(f, &CrawlConfig::default(), &q).unwrap_or_else(|e| panic!("{filter}: {e:?}"));
    (page.columns, page.rows)
}

fn col(columns: &[String], name: &str) -> usize {
    columns
        .iter()
        .position(|c| c == name)
        .unwrap_or_else(|| panic!("Spalte {name} fehlt: {columns:?}"))
}

fn text(v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        Value::Null => String::new(),
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        other => format!("{other:?}"),
    }
}

fn addresses_of(f: &os_store::CrawlFile, filter: &str) -> Vec<String> {
    let (columns, rows) = query(f, filter, Vec::new());
    let a = col(&columns, "Address");
    let mut v: Vec<String> = rows.iter().map(|r| text(&r[a])).collect();
    v.sort();
    v
}

// spec: AI-070 [M4]
// spec: AI-020 [M4]
// spec: UI-004 [M4]
// acceptance: independent
// Jeder Filter liefert genau die erwarteten Zeilen der Fixture mit allen Zuständen; der Filter „Errors“ enthält genau
// `Error: …` und `Budget exceeded`, nicht `Refusal`, `Skipped (…)` oder `Pending`.
#[test]
fn every_filter_yields_exactly_the_expected_rows() {
    let (_d, f) = fixture();
    for (filter, paths) in expected_filters() {
        assert_eq!(addresses_of(&f, filter), addresses(&paths), "Filter {filter}");
    }
}

// spec: AI-070 [M4]
// spec: AI-076 [M4]
// acceptance: independent
// Filter `Open`: übernommene, bearbeitete und abgelehnte Einträge gehören nicht dazu; offene Einträge mit Kandidaten
// (`Valid`, `No valid suggestion`) gehören dazu. (Einträge ohne Kandidaten mit `Review Status = Open` – Pending, Fehler –
// sind in der Spec nicht eindeutig zugeordnet, siehe Bericht B2.)
#[test]
fn open_filter_contains_unreviewed_suggestions_only() {
    let (_d, f) = fixture();
    let open = addresses_of(&f, "Open");
    for p in ["/valid-open/", "/novalid/", "/meta-only/"] {
        assert!(open.contains(&address(p)), "{p} fehlt in Open: {open:?}");
    }
    for p in ["/accepted/", "/edited/", "/rejected/", "/formula/"] {
        assert!(!open.contains(&address(p)), "{p} gehört nicht in Open");
    }
}

// spec: AI-071 [M4]
// spec: AI-072 [M4]
// spec: AI-076 [M4]
// acceptance: independent
// Spalten nach AI-071 (Title) und AI-072 (Präfix `Meta Description Suggestion`, Basisspalten `Meta Description 1`,
// `Meta Description 1 Pixel Width`); `Title Suggestion 1` ist der gewählte Kandidat, danach die übrigen; je Kandidat
// `… Length` und `… Pixel Width`; `Review Status` mit den kanonischen Werten; `Accepted Title`.
#[test]
fn columns_carry_values_of_the_chosen_candidate_and_review() {
    let (_d, f) = fixture();
    let (c, rows) = query(&f, "All", Vec::new());
    let row = |p: &str| {
        let a = col(&c, "Address");
        rows.iter()
            .find(|r| text(&r[a]) == address(p))
            .unwrap_or_else(|| panic!("Zeile {p} fehlt"))
            .clone()
    };
    let open = row("/valid-open/");
    assert_eq!(text(&open[col(&c, "Title 1")]), "Startseite");
    assert_eq!(text(&open[col(&c, "Title 1 Pixel Width")]), "99");
    assert_eq!(text(&open[col(&c, "Title Suggestion 1")]), V_OPEN_1);
    assert_eq!(
        text(&open[col(&c, "Title Suggestion 1 Length")]),
        V_OPEN_1.chars().count().to_string()
    );
    assert_eq!(text(&open[col(&c, "Title Suggestion 1 Pixel Width")]), "401");
    assert_eq!(text(&open[col(&c, "Title Suggestion 2")]), V_OPEN_2);
    assert_eq!(text(&open[col(&c, "Title Suggestion 2 Pixel Width")]), "399");
    assert_eq!(text(&open[col(&c, "Title Suggestion Status")]), "Valid");
    assert_eq!(text(&open[col(&c, "Review Status")]), "Open");
    assert_eq!(text(&open[col(&c, "Meta Description 1")]), "Alte Beschreibung der Startseite");
    assert_eq!(text(&open[col(&c, "Meta Description 1 Pixel Width")]), "222");
    assert_eq!(text(&open[col(&c, "Meta Description Suggestion 1")]), V_META);
    assert_eq!(text(&open[col(&c, "Meta Description Suggestion 1 Pixel Width")]), "610");
    assert_eq!(text(&open[col(&c, "Meta Description Suggestion Status")]), "Valid");

    // Gewählt ist Kandidat 2 der Antwort → er steht als Kandidat 1 vorn.
    let acc = row("/accepted/");
    assert_eq!(text(&acc[col(&c, "Title Suggestion 1")]), V_ACC);
    assert_eq!(text(&acc[col(&c, "Title Suggestion 2")]), V_ACC_INVALID);
    assert_eq!(text(&acc[col(&c, "Review Status")]), "Accepted");
    assert_eq!(text(&acc[col(&c, "Accepted Title")]), V_ACC);

    let ed = row("/edited/");
    assert_eq!(text(&ed[col(&c, "Review Status")]), "Edited");
    assert_eq!(text(&ed[col(&c, "Accepted Title")]), V_EDITED);
    assert_eq!(text(&row("/rejected/")[col(&c, "Review Status")]), "Rejected");

    // Ohne gültigen Kandidaten: sichtbar der mit den wenigsten Verstößen, mit Verstoßliste.
    let nv = row("/novalid/");
    assert_eq!(text(&nv[col(&c, "Title Suggestion Status")]), "No valid suggestion");
    assert_eq!(text(&nv[col(&c, "Title Suggestion 1")]), V_NOVALID_2);
    assert_eq!(text(&nv[col(&c, "Title Suggestion Violations")]), "length>60");

    assert_eq!(
        text(&row("/error/")[col(&c, "Title Suggestion Status")]),
        "Error: 500 Internal Server Error"
    );
    for p in ["/pending/", "/error/", "/budget/"] {
        assert_eq!(text(&row(p)[col(&c, "Title Suggestion 1")]), "", "{p}: leere Ergebniszelle");
    }
}

// spec: AI-070 [M4]
// spec: UI-004 [M4]
// acceptance: independent
// Sortierung nach einer Kandidatenspalte (Zahl) auf- und absteigend; Zähler des Tabs stimmen mit den Zeilen überein.
#[test]
fn sorting_and_counters() {
    let (_d, f) = fixture();
    let key = "Title Suggestion 1 Pixel Width";
    let order = |descending: bool| {
        let (c, rows) = query(&f, "Titles", vec![SortKey { column: key.into(), descending }]);
        let i = col(&c, key);
        rows.iter()
            .filter_map(|r| match r[i] {
                Value::Int(n) => Some(n),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let asc = order(false);
    let mut sorted = asc.clone();
    sorted.sort();
    assert_eq!(asc, sorted, "aufsteigend");
    assert_eq!(asc.len(), 6, "{asc:?}");
    let desc = order(true);
    let mut rev = sorted.clone();
    rev.reverse();
    assert_eq!(desc, rev, "absteigend");

    let counts = filter_counts(&f, &CrawlConfig::default()).unwrap();
    for (filter, paths) in expected_filters() {
        let c = counts
            .iter()
            .find(|c| c.tab == TAB && c.filter == filter)
            .unwrap_or_else(|| panic!("Zähler {filter} fehlt"));
        assert_eq!(c.count, Some(paths.len() as u64), "Zähler {filter}");
    }
}
