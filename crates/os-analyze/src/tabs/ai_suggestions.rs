//! Tab „AI Suggestions“ (08 AI-070 … AI-076; M4-V01): eine Zeile je URL mit Vorschlägen, Spalten je Element mit
//! Präfix (`Title Suggestion …`, AI-071; `Meta Description Suggestion …`, AI-072), Filter je Element und je
//! Prüfstatus.
//!
//! Die Daten stehen in `ai_suggestions` (Zeilenaufbau siehe `os_store::ai::suggestions`): `candidate = 0` ist der
//! Elementeintrag, `candidate ≥ 1` die Kandidaten in Antwortreihenfolge; `Suggestion 1` ist der gewählte bzw. sichtbare
//! Kandidat (`$.order[0]` im Elementeintrag), danach folgen die übrigen in Antwortreihenfolge. Der Tab ist nur mit
//! eingeschaltetem Schalter „KI (Vorschau)“ sichtbar (os-app `commands::ai_preview`).
use std::sync::OnceLock;

use crate::defs::{ColumnDef, ColumnKind, FilterDef, FilterKind, TabDef};

/// Kandidatenspalten je Element (Bereich der Kandidatenzahl 1–5, AI-070).
pub const MAX_CANDIDATES: usize = 5;

/// Element-Kennung in `ai_suggestions.element`, Präfix der Spalten, Basisspalten.
struct ElementColumns {
    id: &'static str,
    /// `Title` bzw. `Meta Description`.
    prefix: &'static str,
    base: &'static str,
    base_px: &'static str,
    review: &'static str,
    accepted: &'static str,
}

const TITLE: ElementColumns = ElementColumns {
    id: "title",
    prefix: "Title",
    base: "u.title_1",
    base_px: "COALESCE(u.title_1_px, 0)",
    // AI-071 nennt die Spalte `Review Status` ohne Präfix.
    review: "Review Status",
    accepted: "Accepted Title",
};

const META: ElementColumns = ElementColumns {
    id: "meta_description",
    prefix: "Meta Description",
    base: "u.meta_desc_1",
    base_px: "COALESCE(u.meta_desc_1_px, 0)",
    review: "Meta Description Review Status",
    accepted: "Accepted Meta Description",
};

fn record(element: &str, expr: &str) -> String {
    format!(
        "(SELECT {expr} FROM ai_suggestions r WHERE r.url_id = u.id AND r.element = '{element}' AND r.candidate = 0)"
    )
}

/// SQL der Anzeigeposition `slot` (1 …): Position 1 ist `$.order[0]`, danach die übrigen in Antwortreihenfolge.
fn candidate(element: &str, slot: usize, expr: &str) -> String {
    let first = format!(
        "(SELECT json_extract(r.violations_json, '$.order[0]') FROM ai_suggestions r \
         WHERE r.url_id = u.id AND r.element = '{element}' AND r.candidate = 0)"
    );
    if slot == 1 {
        format!(
            "(SELECT {expr} FROM ai_suggestions c WHERE c.url_id = u.id AND c.element = '{element}' \
             AND c.candidate > 0 AND c.candidate = {first})"
        )
    } else {
        format!(
            "(SELECT {expr} FROM ai_suggestions c WHERE c.url_id = u.id AND c.element = '{element}' \
             AND c.candidate > 0 AND c.candidate IS NOT {first} ORDER BY c.candidate LIMIT 1 OFFSET {})",
            slot - 2
        )
    }
}

fn element_columns(e: &ElementColumns, visible: bool) -> Vec<ColumnDef> {
    let p = e.prefix;
    let mut columns = vec![
        ColumnDef::dynamic(&format!("{p} 1"), e.base, ColumnKind::Text, visible),
        ColumnDef::dynamic(
            &format!("{p} 1 Pixel Width"),
            e.base_px,
            ColumnKind::Int,
            visible,
        ),
    ];
    for k in 1..=MAX_CANDIDATES {
        let shown = visible && k <= 3;
        columns.push(ColumnDef::dynamic(
            &format!("{p} Suggestion {k}"),
            &candidate(e.id, k, "c.value"),
            ColumnKind::Text,
            shown,
        ));
        columns.push(ColumnDef::dynamic(
            &format!("{p} Suggestion {k} Length"),
            &candidate(e.id, k, "json_extract(c.violations_json, '$.length')"),
            ColumnKind::Int,
            shown,
        ));
        columns.push(ColumnDef::dynamic(
            &format!("{p} Suggestion {k} Pixel Width"),
            &candidate(e.id, k, "json_extract(c.violations_json, '$.pixel_width')"),
            ColumnKind::Int,
            shown,
        ));
        // AI-075: Kennzeichnung je Kandidat (gültig/ungültig).
        columns.push(ColumnDef::dynamic(
            &format!("{p} Suggestion {k} Valid"),
            &candidate(e.id, k, "(c.status = 'valid')"),
            ColumnKind::Bool,
            false,
        ));
    }
    columns.push(ColumnDef::dynamic(
        &format!("{p} Suggestion Status"),
        &record(e.id, "r.status"),
        ColumnKind::Text,
        visible,
    ));
    // AI-071: Status und Kandidat 1 sind bis zum Nachlauf vorläufig.
    columns.push(ColumnDef::dynamic(
        &format!("{p} Suggestion Provisional"),
        &record(
            e.id,
            "COALESCE(json_extract(r.violations_json, '$.provisional'), 0)",
        ),
        ColumnKind::Bool,
        false,
    ));
    columns.push(ColumnDef::dynamic(
        &format!("{p} Suggestion Violations"),
        &record(
            e.id,
            "(SELECT group_concat(j.value, '; ') FROM json_each(r.violations_json, '$.violations') j)",
        ),
        ColumnKind::Text,
        visible,
    ));
    columns.push(ColumnDef::dynamic(
        e.review,
        &record(
            e.id,
            "CASE r.review_status WHEN 'accepted' THEN 'Accepted' WHEN 'edited' THEN 'Edited' \
             WHEN 'rejected' THEN 'Rejected' ELSE 'Open' END",
        ),
        ColumnKind::Text,
        visible,
    ));
    columns.push(ColumnDef::dynamic(
        e.accepted,
        &record(e.id, "r.accepted_value"),
        ColumnKind::Text,
        visible,
    ));
    columns
}

fn keys(columns: &[ColumnDef]) -> Vec<&'static str> {
    columns.iter().map(|c| c.key).collect()
}

/// Standardspalten der Filter `Titles` und `Meta Descriptions` (einmal je Prozess festgehalten).
fn default_columns() -> &'static (Vec<&'static str>, Vec<&'static str>) {
    static COLUMNS: OnceLock<(Vec<&'static str>, Vec<&'static str>)> = OnceLock::new();
    COLUMNS.get_or_init(|| {
        let pick = |e: &ElementColumns| {
            let mut v = vec!["Address"];
            v.extend(
                keys(&element_columns(e, true))
                    .into_iter()
                    .filter(|k| !k.ends_with(" Valid") && !k.ends_with(" Provisional")),
            );
            v.retain(|k| {
                // Nur Kandidaten 1–3 in der Standardansicht.
                !(k.contains("Suggestion 4") || k.contains("Suggestion 5"))
            });
            v
        };
        (pick(&TITLE), pick(&META))
    })
}

fn exists(predicate: &str) -> &'static str {
    crate::defs::intern(&format!(
        "EXISTS (SELECT 1 FROM ai_suggestions s WHERE s.url_id = u.id AND s.candidate = 0 AND {predicate})"
    ))
}

fn filter(id: &'static str, predicate: &'static str) -> FilterDef {
    FilterDef {
        predicate,
        ..FilterDef::placeholder(id, FilterKind::Live, &[])
    }
}

/// Definition des Tabs „AI Suggestions“.
pub fn tab() -> TabDef {
    let mut tab = super::stub_tab("AI Suggestions");
    tab.population =
        "u.is_internal = 1 AND EXISTS (SELECT 1 FROM ai_suggestions s WHERE s.url_id = u.id \
         AND s.candidate = 0 AND s.element IN ('title', 'meta_description', 'h1', 'alt'))";
    let mut columns = tab.columns;
    columns.extend(element_columns(&TITLE, true));
    columns.extend(element_columns(&META, true));
    tab.columns = columns;
    let (title_cols, meta_cols) = default_columns();
    let real = "s.element IN ('title', 'meta_description', 'h1', 'alt')";
    let mut titles = filter("Titles", exists("s.element = 'title'"));
    titles.default_columns = title_cols.as_slice();
    let mut metas = filter(
        "Meta Descriptions",
        exists("s.element = 'meta_description'"),
    );
    metas.default_columns = meta_cols.as_slice();
    tab.filters = vec![
        FilterDef::placeholder("All", FilterKind::Live, &[]),
        titles,
        metas,
        // H1 (AI-074) und Alt-Text (AI-073) füllt M4-V02.
        filter("H1", exists("s.element = 'h1'")),
        filter("Alt Text", exists("s.element = 'alt'")),
        filter(
            "No Valid Suggestion",
            exists(&format!("{real} AND s.status = 'No valid suggestion'")),
        ),
        filter(
            "Accepted",
            exists(&format!("{real} AND s.review_status IN ('accepted', 'edited')")),
        ),
        filter(
            "Open",
            exists(&format!(
                "{real} AND s.status IN ('Valid', 'No valid suggestion') \
                 AND s.review_status NOT IN ('accepted', 'edited', 'rejected')"
            )),
        ),
        filter(
            "Rejected",
            exists(&format!("{real} AND s.review_status = 'rejected'")),
        ),
        // AI-020: `Error: …`, `Quota exceeded`, `Budget exceeded`; `Skipped (…)` und `Refusal` zählen nicht dazu.
        filter(
            "Errors",
            exists(&format!(
                "{real} AND (s.status LIKE 'Error: %' OR s.status IN ('Quota exceeded', 'Budget exceeded'))"
            )),
        ),
    ];
    tab
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-070 [M4]
    // spec: AI-071 [M4]
    // spec: AI-072 [M4]
    // spec: UI-004 [M4]
    #[test]
    fn tab_carries_the_spec_columns_and_filters() {
        let tab = tab();
        let keys: Vec<&str> = tab.columns.iter().map(|c| c.key).collect();
        for k in [
            "Address",
            "Title 1",
            "Title 1 Pixel Width",
            "Title Suggestion 1",
            "Title Suggestion 1 Length",
            "Title Suggestion 1 Pixel Width",
            "Title Suggestion 5",
            "Title Suggestion Status",
            "Title Suggestion Violations",
            "Review Status",
            "Accepted Title",
            "Meta Description 1",
            "Meta Description 1 Pixel Width",
            "Meta Description Suggestion 1",
            "Meta Description Suggestion Status",
            "Meta Description Suggestion Violations",
            "Accepted Meta Description",
        ] {
            assert!(keys.contains(&k), "Spalte {k} fehlt");
        }
        let unique: std::collections::BTreeSet<&str> = keys.iter().copied().collect();
        assert_eq!(unique.len(), keys.len(), "doppelte Spalte");
        let filters: Vec<&str> = tab.filters.iter().map(|f| f.id).collect();
        assert_eq!(
            filters,
            [
                "All",
                "Titles",
                "Meta Descriptions",
                "H1",
                "Alt Text",
                "No Valid Suggestion",
                "Accepted",
                "Open",
                "Rejected",
                "Errors"
            ]
        );
        assert!(tab.filters.iter().all(|f| !f.is_stub()));
        let titles = tab.filters.iter().find(|f| f.id == "Titles").unwrap();
        assert!(titles.default_columns.contains(&"Title Suggestion 1"));
        assert!(!titles
            .default_columns
            .contains(&"Meta Description Suggestion 1"));
    }
}
