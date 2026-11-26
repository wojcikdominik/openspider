//! Unabhängige Abnahme M4-D01 – Fragenbibliothek, Benutzerfragen, Versionierung (AI-031), parametrisierte
//! Systemfragen in `ai_questions`/`ai_answers` (AI-030, AI-031). Erwartungen nur aus `docs/spec/08-ai.md`.
// acceptance: independent
mod acceptance_d01_support;

use std::collections::BTreeSet;

use acceptance_d01_support as s;
use os_ai::decisions::format::{
    parse_value, Candidate, ChoiceDef, ParamValue, ParamValues, QuestionDef, QuestionType, Version,
};
use os_ai::decisions::library::{
    self, change_kind, next_version, record_used, stored_version, update_notice, ChangeKind,
    Import, ImportChoice, Library, LibraryError, Origin,
};
use serde_json::json;

fn user_question() -> QuestionDef {
    parse_value(&json!({
        "id": "my-intent",
        "version": "1.2.0",
        "name": "My Intent",
        "type": "choice",
        "instructions": "Which intent does the page serve?",
        "choices": [
            {"value": "buy", "description": "Transactional."},
            {"value": "learn", "description": "Informational."},
            {"value": "other", "description": "Anything else."}
        ],
        "subject": "page",
        "inputs": ["title", "page_text"],
        "thresholds": {"low_confidence_below": 0.6}
    }))
    .unwrap()
}

fn system_question() -> QuestionDef {
    parse_value(&json!({
        "id": "soft-404",
        "version": "1.0.0",
        "name": "Soft 404",
        "type": "predicate",
        "instructions": "The page says the requested content does not exist.",
        "subject": "page",
        "inputs": ["title", "page_text"]
    }))
    .unwrap()
}

fn template(instructions: &str) -> QuestionDef {
    let mut list: Vec<serde_json::Value> = (1..=10)
        .map(|i| json!({"value": format!("c{i}"), "description": "{{candidates}}"}))
        .collect();
    list.push(json!({"value": "none", "description": "No candidate replaces the old page."}));
    parse_value(&json!({
        "id": "redirect-target",
        "version": "1.0.0",
        "name": "Redirect Target",
        "type": "choice",
        "instructions": instructions,
        "choices": list,
        "subject": "page",
        "inputs": ["url", "title", "page_text"],
        "params": [{"name": "candidates", "type": "candidates", "source": "redirect.candidates"}]
    }))
    .unwrap()
}

// spec: AI-031 [M4]
// acceptance: independent
#[test]
fn ai031_version_rules() {
    let old = user_question();
    // instructions → Minor (1.2.0 → 1.3.0).
    let mut changed = old.clone();
    changed.instructions = "Which search intent does the page serve primarily?".into();
    assert_eq!(next_version(&old, &changed), Version::new(1, 3, 0));
    assert_eq!(change_kind(&old, &changed), ChangeKind::Content);
    // type → Major (2.0.0).
    let mut typed = old.clone();
    typed.kind = QuestionType::Predicate;
    typed.choices = None;
    assert_eq!(next_version(&old, &typed), Version::new(2, 0, 0));
    // Entfernen eines choice-Werts → Major.
    let mut removed = old.clone();
    removed.choices.as_mut().unwrap().remove(1);
    assert_eq!(next_version(&old, &removed), Version::new(2, 0, 0));
    // Hinzufügen eines Werts, geänderte inputs → inhaltlich (Minor).
    let mut added = old.clone();
    added.choices.as_mut().unwrap().push(ChoiceDef {
        value: "visit".into(),
        description: "Navigational.".into(),
    });
    assert_eq!(next_version(&old, &added), Version::new(1, 3, 0));
    let mut inputs = old.clone();
    inputs.inputs.pop();
    assert_eq!(next_version(&old, &inputs), Version::new(1, 3, 0));
    // Schwellwert- und Anzeigeänderungen → Version bleibt.
    let mut thresholds = old.clone();
    thresholds.thresholds = Some(json!({"low_confidence_below": 0.8}));
    assert_eq!(next_version(&old, &thresholds), Version::new(1, 2, 0));
    let mut display = old.clone();
    display.name = "My Search Intent".into();
    assert_eq!(next_version(&old, &display), Version::new(1, 2, 0));
    // Nachtrag 26.11.2026 (Auslegung 4 bestätigt): scope ändert die Version nicht; subject → Major.
    let mut scoped = old.clone();
    scoped.scope.indexable_only = Some(false);
    assert_eq!(next_version(&old, &scoped), Version::new(1, 2, 0));
    let mut subject = old.clone();
    subject.subject = os_ai::decisions::format::Subject::Image;
    subject.inputs = vec![os_ai::decisions::format::InputSpec::field(
        os_ai::policy::InputField::Image,
    )];
    assert_eq!(next_version(&old, &subject), Version::new(2, 0, 0));
}

// spec: AI-031 [M4]
// acceptance: independent
#[test]
fn ai031_levels_and_params_changes() {
    let score = parse_value(&json!({
        "id": "my-score",
        "version": "1.4.2",
        "name": "My Score",
        "type": "score",
        "instructions": "Rate it.",
        "levels": [
            {"label": "low", "description": "Low."},
            {"label": "mid", "description": "Mid."},
            {"label": "high", "description": "High."}
        ],
        "subject": "page",
        "inputs": ["page_text"]
    }))
    .unwrap();
    let mut removed = score.clone();
    removed.levels.as_mut().unwrap().pop();
    assert_eq!(next_version(&score, &removed), Version::new(2, 0, 0), "Level entfernt → Major");
    let mut described = score.clone();
    described.levels.as_mut().unwrap()[0].description = "Very low.".into();
    assert_eq!(next_version(&score, &described), Version::new(1, 5, 0));
    // params einer Vorlage gehören zur inhaltlichen Änderung.
    let tpl = template("Which candidate fits?");
    let mut params = tpl.clone();
    params.params[0].source = "redirect.other_candidates".into();
    assert_eq!(next_version(&tpl, &params), Version::new(1, 1, 0));
}

// spec: AI-031 [M4]
// acceptance: independent
#[test]
fn ai031_saving_a_changed_user_question_bumps_version() {
    let mut lib = Library::new(vec![system_question()], vec![user_question()]).unwrap();
    let mut changed = user_question();
    changed.instructions = "Which intent does the page serve? Ignore navigation.".into();
    let saved = lib.save_user(changed).unwrap().clone();
    assert_eq!(saved.version, Version::new(1, 3, 0));
    let mut thresholds = saved.clone();
    thresholds.thresholds = Some(json!({"low_confidence_below": 0.7}));
    let saved = lib.save_user(thresholds).unwrap().clone();
    assert_eq!(saved.version, Version::new(1, 3, 0), "Schwellwert ändert die Version nicht");
}

// spec: AI-031 [M4]
// acceptance: independent
#[test]
fn ai031_system_questions_are_read_only_edit_creates_custom_copy() {
    let mut lib = Library::new(vec![system_question()], vec![]).unwrap();
    let mut edited = system_question();
    edited.instructions = "Changed.".into();
    assert!(
        matches!(lib.save_user(edited), Err(LibraryError::ReadOnly(_))),
        "Systemfrage überschrieben"
    );
    let copy = lib.edit_copy("soft-404").unwrap().clone();
    assert_eq!(copy.id, "soft-404-custom");
    assert_eq!(copy.name, "Soft 404 (custom)");
    assert_eq!(copy.instructions, system_question().instructions);
    let (origin, _) = lib.get("soft-404-custom").expect("Kopie gespeichert");
    assert_eq!(origin, Origin::User);
    let (origin, original) = lib.get("soft-404").unwrap();
    assert_eq!(origin, Origin::System);
    assert_eq!(*original, system_question(), "Systemfrage unverändert");
}

// spec: AI-031 [M4]
// acceptance: independent
#[test]
fn ai031_export_import_roundtrip_and_conflict() {
    let q = user_question();
    let text = library::export(&q);
    // In eine leere Bibliothek: identische Definition.
    let mut empty = Library::new(vec![], vec![]).unwrap();
    match empty.import(&text).unwrap() {
        Import::Added(added) => assert_eq!(*added, q),
        other => panic!("erwartet Added, erhalten {other:?}"),
    }
    assert_eq!(empty.get("my-intent").unwrap().1, &q);
    // Gleiche id, gleiche Definition → nichts zu tun.
    assert_eq!(empty.import(&text).unwrap(), Import::Identical);
    // Gleiche id, andere Definition → Rückfrage „Ersetzen / als Kopie speichern“.
    let mut other = q.clone();
    other.instructions = "Different criteria.".into();
    let conflict = empty.import(&library::export(&other)).unwrap();
    let incoming = match conflict {
        Import::Conflict { incoming, .. } => *incoming,
        other => panic!("erwartet Conflict, erhalten {other:?}"),
    };
    let copy = empty
        .resolve(incoming.clone(), ImportChoice::SaveAsCopy)
        .unwrap()
        .clone();
    assert_eq!(copy.id, "my-intent-copy");
    let second = empty
        .resolve(incoming.clone(), ImportChoice::SaveAsCopy)
        .unwrap()
        .clone();
    assert_eq!(second.id, "my-intent-copy-2", "erneute Kollision");
    assert_eq!(copy.instructions, "Different criteria.");
    assert_eq!(empty.get("my-intent").unwrap().1.instructions, q.instructions);
    let replaced = empty
        .resolve(incoming, ImportChoice::Replace)
        .unwrap()
        .clone();
    assert_eq!(replaced.id, "my-intent");
    assert_eq!(replaced.instructions, "Different criteria.");
}

// spec: AI-031 [M4]
// acceptance: independent
#[test]
fn ai031_update_notice_text() {
    assert_eq!(
        update_notice(Version::new(1, 0, 0), Version::new(1, 1, 0)).as_deref(),
        Some("Frage aktualisiert (1.0.0 → 1.1.0) – neu analysieren?")
    );
    assert_eq!(update_notice(Version::new(1, 1, 0), Version::new(1, 1, 0)), None);
}

// spec: AI-031 [M4]
// acceptance: independent
#[test]
fn ai030_templates_only_as_system_questions() {
    let mut lib = Library::new(vec![template("Which candidate fits?")], vec![]).unwrap();
    let own = parse_value(&json!({
        "id": "my-language",
        "version": "1.0.0",
        "name": "My Language",
        "type": "predicate",
        "instructions": "Is the content written in {{lang}}?",
        "subject": "page",
        "inputs": ["page_text"],
        "params": [{"name": "lang", "type": "string", "source": "html_lang"}]
    }))
    .unwrap();
    assert!(
        lib.save_user(own).is_err(),
        "eigene Vorlage als Benutzerfrage gespeichert"
    );
    // Die Kopie einer Systemvorlage bleibt eine Vorlage mit denselben params.
    let copy = lib.edit_copy("redirect-target").unwrap().clone();
    assert!(copy.is_template());
    assert_eq!(copy.params, template("Which candidate fits?").params);
}

// spec: AI-031 [M4]
// acceptance: independent
#[tokio::test]
async fn ai031_ai_questions_one_immutable_row_per_used_version() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let conn = bench.conn();
    let v1 = user_question();
    assert!(record_used(conn, &v1).unwrap(), "erste Ablage ist neu");
    assert!(!record_used(conn, &v1).unwrap(), "zweite Ablage legt nichts an");
    assert_eq!(os_store::ai::questions::count_versions(conn, "my-intent").unwrap(), 1);
    let mut v2 = v1.clone();
    v2.instructions = "Changed criteria.".into();
    v2.version = next_version(&v1, &v2);
    assert!(record_used(conn, &v2).unwrap());
    assert_eq!(os_store::ai::questions::count_versions(conn, "my-intent").unwrap(), 2);
    // Die alte Kopie ist unverändert.
    assert_eq!(stored_version(conn, "my-intent", "1.2.0").unwrap(), Some(v1.clone()));
    assert_eq!(stored_version(conn, "my-intent", "1.3.0").unwrap(), Some(v2));
    // Eine abweichende Definition unter derselben Version wird nicht gespeichert (Kopie bleibt).
    let mut forged = v1.clone();
    forged.instructions = "Forged.".into();
    let _ = os_store::ai::questions::ensure(conn, "my-intent", "1.2.0", &forged.canonical_json())
        .expect_err("gleiche Version mit anderer Definition");
    assert_eq!(stored_version(conn, "my-intent", "1.2.0").unwrap(), Some(v1));
    // Direkte Änderung der Zeile ist ausgeschlossen („wird nie verändert“).
    let update = conn.execute_batch(
        "UPDATE ai_questions SET definition_json = '{}' WHERE id = 'my-intent';",
    );
    assert!(update.is_err(), "ai_questions-Zeile ließ sich ändern");
}

fn candidates_for(item: i64, k: usize) -> Vec<Candidate> {
    (1..=k)
        .map(|i| Candidate {
            address: format!("https://ai.test:8080/neu/{item}/ziel-{i}?ref=ü"),
            title: format!("Ziel {i} für {item}"),
            h1: format!("H1 {i}"),
            excerpt: format!("Auszug {i}"),
        })
        .collect()
}

// spec: AI-031 [M4]
// acceptance: independent
#[tokio::test]
async fn ai031_template_30_sources_one_question_row_30_answers() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let tpl = template("Which candidate replaces the old page best?");
    let mut items = Vec::new();
    for i in 1..=30 {
        let mut item = s::page(i, &format!("Old page {i}"), &format!("Old content {i}."));
        let mut values = ParamValues::new();
        values.insert(
            "candidates".into(),
            ParamValue::Candidates(candidates_for(i, 3)),
        );
        item.params.insert("redirect-target".into(), values);
        items.push(item);
    }
    let report = bench
        .run(&[s::active(tpl.clone())], &items, s::options())
        .await;
    assert_eq!(report.end, os_ai::decisions::exec::RunEnd::Completed);
    let rows = os_store::ai::questions::list(bench.conn()).unwrap();
    let tpl_rows: Vec<_> = rows.iter().filter(|r| r.id == "redirect-target").collect();
    assert_eq!(tpl_rows.len(), 1, "genau 1 Eintrag (die Vorlage) in ai_questions");
    assert_eq!(tpl_rows[0].version, "1.0.0");
    assert!(
        tpl_rows[0].definition_json.contains("{{candidates}}"),
        "gespeichert ist die Vorlage, nicht eine Instanz"
    );
    assert!(!tpl_rows[0].definition_json.contains("ziel-1"));
    let answers = bench.answers("redirect-target");
    assert_eq!(answers.len(), 30, "30 Einträge in ai_answers");
    let params: BTreeSet<_> = answers.iter().map(|a| a.params_json.clone()).collect();
    assert_eq!(params.len(), 30, "jeder Eintrag mit eigenen Parameterwerten");
    for a in &answers {
        assert!(a.params_json.contains(&format!("/neu/{}/ziel-1", a.url_id)));
        assert_eq!(a.question_version, "1.0.0");
        assert_eq!(a.status, "Success");
    }
    // Eine andere Kandidatenliste ändert die Vorlagenversion nicht; ein anderer Vorlagentext schon.
    let changed = template("Which candidate is the best successor of the old page?");
    assert_eq!(next_version(&tpl, &tpl.clone()), Version::new(1, 0, 0));
    assert_eq!(next_version(&tpl, &changed), Version::new(1, 1, 0));
}

// spec: AI-031 [M4]
// acceptance: independent
#[tokio::test]
async fn ai031_old_answers_stay_with_old_version() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let v1 = s::predicate("my-check", "My Check", &["title"]);
    let items: Vec<_> = (1..=2)
        .map(|i| s::page(i, &format!("Title {i}"), "Text."))
        .collect();
    bench.run(&[s::active(v1.clone())], &items, s::options()).await;
    let before = s::Mock::posts(&bench.mock, s::DECISIONS).len();
    assert_eq!(before, 2);
    // Unveränderte Frage, erneuter Lauf → keine Requests (Ergebnisse bleiben).
    let mut again = s::options();
    again.selection = os_ai::decisions::exec::Selection::All;
    bench.run(&[s::active(v1.clone())], &items, again).await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 2, "gleiche Version: aus dem Cache");
    // Neue Version (instructions geändert) → neue Ergebnisse unter 1.1.0, die alten bleiben 1.0.0 zugeordnet.
    let mut v2 = v1.clone();
    v2.instructions = "A different observable criterion.".into();
    v2.version = next_version(&v1, &v2);
    assert_eq!(v2.version, Version::new(1, 1, 0));
    let mut all = s::options();
    all.selection = os_ai::decisions::exec::Selection::All;
    bench.run(&[s::active(v2)], &items, all).await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 4);
    let old = os_store::ai::answers::list_for_question(bench.conn(), "my-check", Some("1.0.0"))
        .unwrap();
    let new = os_store::ai::answers::list_for_question(bench.conn(), "my-check", Some("1.1.0"))
        .unwrap();
    assert_eq!(old.len(), 2);
    assert_eq!(new.len(), 2);
    assert!(old.iter().all(|r| r.status == "Success" && r.answer_json.is_some()));
    assert_eq!(
        os_store::ai::questions::count_versions(bench.conn(), "my-check").unwrap(),
        2
    );
}
