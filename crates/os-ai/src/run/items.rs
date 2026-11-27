//! Von den Eingabefeldern einer Seite ([`PageAiFields`], gelesen bzw. geparst von der Engine) zu den Einträgen eines
//! Decision-Laufs ([`RunItem`]); aktive Fragen aus dem Profil; `Skipped (input unavailable)` nach 08 AI-032, Nachtrag
//! „Quelle der Eingabefelder“ (Punkt 3, K4-26).
use std::collections::BTreeMap;

use os_model::ai::hook::{PageAiFields, IMAGE_CONTEXT_MAX_CHARS};
use os_model::ai::AiStatus;
use os_model::CrawlConfig;
use os_store::ai::answers::{self, AnswerRow};
use os_store::ai::cache::Connection;
use os_store::StoreError;

use crate::decisions::exec::{ActiveQuestion, RunItem};
use crate::decisions::format::{self, QuestionDef, Subject};
use crate::decisions::input::SubjectData;
use crate::decisions::routing::RoutingSetting;
use crate::policy::InputField;

/// Aktive Fragen des Profils in Reihenfolge von `ai.active_questions` (nur bei `ai.enabled`), eingeschränkt auf `only`
/// (leer = alle). Nicht auflösbare bzw. ungültige Fragen stehen in der zweiten Liste (Kennung: Meldung).
pub fn active_questions(cfg: &CrawlConfig, only: &[String]) -> (Vec<ActiveQuestion>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    if !cfg.ai.enabled {
        return (out, errors);
    }
    for id in &cfg.ai.active_questions {
        if !only.is_empty() && !only.contains(id) {
            continue;
        }
        let Some(json) = cfg
            .ai
            .questions
            .iter()
            .find(|q| q.get("id").and_then(serde_json::Value::as_str) == Some(id.as_str()))
        else {
            errors.push(format!("{id}: Frage nicht im Profil"));
            continue;
        };
        match format::parse(&json.to_string()) {
            Ok(def) => out.push(ActiveQuestion {
                def,
                routing: RoutingSetting::Automatic,
            }),
            Err(e) => errors.push(format!(
                "{id}: {}",
                e.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
        }
    }
    (out, errors)
}

fn truncate(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((byte, _)) => text[..byte].to_string(),
        None => text.to_string(),
    }
}

/// Eintrag eines Decision-Laufs für eine Seiten-URL (Bildfragen: siehe Bericht, nicht in D05).
pub fn run_item(page: &PageAiFields) -> RunItem {
    // Bildkontext einer Seitenfrage gibt es nicht (AI-030: `image_context` nur für Bildfragen); die Kontexte stehen in
    // den gespeicherten Eingaben für spätere Bildfragen bereit.
    let image_context = page
        .images
        .iter()
        .filter_map(|i| i.context.as_deref())
        .find(|c| !c.is_empty())
        .map(|c| truncate(c, IMAGE_CONTEXT_MAX_CHARS));
    RunItem {
        url_id: page.url_id,
        is_image: false,
        content_class: page.content_class.clone(),
        status_code: page.status_code,
        indexable: page.indexable,
        segments: Vec::new(),
        data: SubjectData {
            url: page.url.clone(),
            title: page.title.clone(),
            meta_description: page.meta_description.clone(),
            h1: page.h1.clone(),
            h2: page.h2.clone(),
            page_text: page.page_text.clone(),
            word_count: page.word_count,
            html_lang: page.html_lang.clone(),
            hreflang_self: page.hreflang_self.clone(),
            structured_data_types: page.structured_data_types.clone().unwrap_or_default(),
            image: None,
            alt: None,
            image_context,
        },
        params: BTreeMap::new(),
    }
}

/// Fehlt der Frage für diese Seite ein Eingabefeld, das weder gespeichert noch ableitbar ist (AI-032, Punkt 3)?
/// Titel, Meta Description, H1, URL, Status, Wortzahl und `hreflang_self` stehen immer in den Spalten der Crawl-Datei.
pub fn input_unavailable(question: &QuestionDef, page: &PageAiFields) -> bool {
    question.inputs.iter().any(|i| match i.field {
        InputField::PageText | InputField::H2 | InputField::HtmlLang | InputField::ImageContext => {
            !page.has_page_inputs()
        }
        InputField::StructuredDataTypes => page.structured_data_types.is_none(),
        // Bildbytes speichert die Crawl-Datei nicht; ein KI-Lauf ruft keine Seiten bzw. Bilder neu ab (INTEG-008).
        InputField::Image => question.subject == Subject::Page,
        _ => false,
    })
}

/// Liegt die Seite im Geltungsbereich der Frage (08 Konventionen, `scope`, ohne Segment)?
pub fn in_scope(question: &QuestionDef, item: &RunItem) -> bool {
    if question.subject == Subject::Image {
        return false;
    }
    let scope = question.scope();
    scope.content_types.contains(&item.content_class)
        && item
            .status_code
            .is_some_and(|c| scope.status_codes.contains(&c))
        && (!scope.indexable_only || item.indexable)
        && scope.segment.is_none()
}

fn is_open(status: &str) -> bool {
    matches!(
        status,
        "Pending" | "Pending (confirmation)" | "Pending (consent)"
    )
}

/// Schreibt `Skipped (input unavailable)` für die Paare (Seite, Frage) im Geltungsbereich, denen ein Eingabefeld fehlt –
/// nur über offene bzw. fehlende Einträge, nie über fertige. Der Decision-Lauf wählt diese Einträge danach nicht mehr
/// (`Selection::Open`). Rückgabe: Zahl der Einträge.
pub fn mark_unavailable(
    conn: &Connection,
    questions: &[ActiveQuestion],
    pages: &[PageAiFields],
) -> Result<usize, StoreError> {
    let mut rows = Vec::new();
    let status = AiStatus::SkippedInputUnavailable.to_string();
    for q in questions.iter().filter(|q| !q.def.is_template()) {
        let version = q.def.version.to_string();
        for page in pages {
            if !input_unavailable(&q.def, page) || !in_scope(&q.def, &run_item(page)) {
                continue;
            }
            let existing = answers::get(conn, page.url_id, &q.def.id, &version, "")?;
            if existing.as_ref().is_some_and(|r| !is_open(&r.status)) {
                continue;
            }
            rows.push(AnswerRow::status_only(
                page.url_id,
                &q.def.id,
                &version,
                "",
                "{}",
                &status,
            ));
        }
    }
    if !rows.is_empty() {
        for q in questions {
            crate::decisions::library::record_used(conn, &q.def)?;
        }
        answers::upsert_all(conn, &rows)?;
    }
    Ok(rows.len())
}
