//! M4-D01: Fragenbibliothek, Benutzerfragen und Versionierung (AI-031).
#[path = "decisions_support.rs"]
mod support;

use os_ai::decisions::exec::{ActiveQuestion, RunControl, RunItem, RunOptions};
use os_ai::decisions::format::{self, Candidate, ParamValue, ParamValues, Version};
use os_ai::decisions::library::{
    self, export, load_dir, next_version, update_notice, Import, ImportChoice, Library,
    LibraryError, Origin,
};
use os_ai::decisions::routing::RoutingSetting;
use support::*;

// spec: AI-031 [M4]
// Änderung der `instructions` 1.2.0 → 1.3.0 (Minor); Typwechsel oder entfernter `choice`-Wert → 2.0.0;
// Schwellwert- und Anzeigeänderung → Version bleibt.
#[test]
fn version_rules() {
    let mut old = question("page-type");
    old.id = "my-type".into();
    old.version = Version::new(1, 2, 0);
    let mut changed = old.clone();
    changed.instructions.push_str(" More detail.");
    assert_eq!(next_version(&old, &changed), Version::new(1, 3, 0));
    let mut removed = old.clone();
    removed.choices.as_mut().unwrap().pop();
    assert_eq!(next_version(&old, &removed), Version::new(2, 0, 0));
    let mut retyped = question("soft-404");
    retyped.id = old.id.clone();
    assert_eq!(next_version(&old, &retyped), Version::new(2, 0, 0));
    let mut threshold = old.clone();
    threshold.thresholds = Some(serde_json::json!({"low_confidence_below": 0.8}));
    threshold.name = "Renamed".into();
    assert_eq!(next_version(&old, &threshold), Version::new(1, 2, 0));
    let mut added = old.clone();
    added.choices.as_mut().unwrap().push(format::ChoiceDef {
        value: "faq".into(),
        description: "FAQ".into(),
    });
    assert_eq!(next_version(&old, &added), Version::new(1, 3, 0));
    let mut inputs = old.clone();
    inputs.inputs.pop();
    assert_eq!(next_version(&old, &inputs), Version::new(1, 3, 0));
    // Nachtrag 26.11.2026: geänderter `subject` → Major.
    let mut subject = old.clone();
    subject.subject = format::Subject::Image;
    subject.inputs = vec![format::InputSpec::field(os_ai::policy::InputField::Image)];
    assert_eq!(next_version(&old, &subject), Version::new(2, 0, 0));
    let mut scoped = old.clone();
    scoped.scope.indexable_only = Some(false);
    assert_eq!(next_version(&old, &scoped), Version::new(1, 2, 0));

    // Über die Benutzerbibliothek angewendet.
    let mut lib = Library::new(vec![], vec![old.clone()]).unwrap();
    assert_eq!(
        lib.save_user(changed).unwrap().version,
        Version::new(1, 3, 0)
    );
}

// spec: AI-031 [M4]
// Nachtrag 26.11.2026: Entfernen eines Levels → Major; geänderte Level-Beschreibung oder ein neues Level → Minor.
#[test]
fn level_rules() {
    let mut old = question("quality");
    old.id = "my-quality".into();
    old.version = Version::new(1, 4, 2);
    let mut removed = old.clone();
    removed.levels.as_mut().unwrap().pop();
    assert_eq!(next_version(&old, &removed), Version::new(2, 0, 0));
    let mut described = old.clone();
    described.levels.as_mut().unwrap()[0].description = "Very low.".into();
    assert_eq!(next_version(&old, &described), Version::new(1, 5, 0));
    let mut added = old.clone();
    added.levels.as_mut().unwrap().push(format::LevelDef {
        label: "excellent".into(),
        description: "Excellent.".into(),
    });
    assert_eq!(next_version(&old, &added), Version::new(1, 5, 0));
}

// spec: AI-031 [M4]
// Bibliotheksfragen sind schreibgeschützt; „Bearbeiten“ erzeugt eine Benutzerkopie mit Suffix `-custom`.
#[test]
fn library_questions_are_read_only_and_edit_makes_a_custom_copy() {
    let system = load_dir(&fixture("lib-v1")).unwrap();
    let mut lib = Library::new(system, vec![]).unwrap();
    let mut q = question("page-type");
    q.instructions.push_str(" changed");
    assert!(matches!(lib.save_user(q), Err(LibraryError::ReadOnly(_))));
    let copy = lib.edit_copy("page-type").unwrap().clone();
    assert_eq!(copy.id, "page-type-custom");
    assert_eq!(copy.version, Version::new(1, 0, 0));
    assert_ne!(copy.name, question("page-type").name);
    assert!(matches!(
        lib.get("page-type-custom"),
        Some((Origin::User, _))
    ));
    assert!(matches!(lib.get("page-type"), Some((Origin::System, _))));
    // Beide aktiv gültig (eindeutige Namen).
    assert!(format::validate_active(&[question("page-type"), copy.clone()]).is_ok());
    let mut edited = copy;
    edited.instructions.push_str(" Mine.");
    assert_eq!(
        lib.save_user(edited).unwrap().version,
        Version::new(1, 1, 0)
    );
}

// spec: AI-031 [M4]
// Export → Import einer Nutzerfrage ergibt eine identische Definition; gleiche `id` mit anderer Definition fragt
// „Ersetzen / als Kopie speichern“.
#[test]
fn export_import_roundtrip_and_conflict() {
    let mut mine = question("soft-404");
    mine.id = "my-soft".into();
    let text = export(&mine);
    let mut empty = Library::default();
    assert_eq!(
        empty.import(&text).unwrap(),
        Import::Added(Box::new(mine.clone()))
    );
    assert_eq!(format::parse(&text).unwrap(), mine);
    assert_eq!(empty.import(&text).unwrap(), Import::Identical);
    let mut other = mine.clone();
    other.instructions = "Something else entirely.".into();
    let Import::Conflict {
        origin, incoming, ..
    } = empty.import(&export(&other)).unwrap()
    else {
        panic!("Rückfrage erwartet");
    };
    assert_eq!(origin, Origin::User);
    let copy = empty
        .resolve(*incoming.clone(), ImportChoice::SaveAsCopy)
        .unwrap()
        .clone();
    assert_eq!(copy.id, "my-soft-copy");
    let replaced = empty.resolve(*incoming, ImportChoice::Replace).unwrap();
    assert_eq!(replaced.instructions, "Something else entirely.");
    assert_eq!(replaced.version, Version::new(1, 1, 0));
}

// spec: AI-031 [M4]
// Nach einem App-Update mit neuer Bibliotheksversion zeigt ein vorhandener Crawl weiter die alten Ergebnisse mit
// Hinweis „Frage aktualisiert (1.0.0 → 1.1.0) – neu analysieren?“; erst „Neu analysieren“ sendet Requests.
#[tokio::test]
async fn library_update_keeps_old_results_until_reanalysis() {
    let v1 = load_dir(&fixture("lib-v1")).unwrap();
    let v2 = load_dir(&fixture("lib-v2")).unwrap();
    let pt1 = v1.iter().find(|q| q.id == "page-type").unwrap().clone();
    let pt2 = v2.iter().find(|q| q.id == "page-type").unwrap().clone();
    assert_eq!(pt2.version, Version::new(1, 1, 0));
    let env = Env::new(&["openai"]).await;
    let items: Vec<RunItem> = vec![text_item(1, "MOCK-ID:product-1 Wanderrucksack")];
    let dir = TempDir::new("lib");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let q1 = [ActiveQuestion {
        def: pt1.clone(),
        routing: RoutingSetting::Automatic,
    }];
    env.run(
        file.conn(),
        run,
        &q1,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    assert_eq!(env.mock.posts(DECISIONS).len(), 1);
    // App-Update: der Crawl behält v1; Hinweis für die Ansicht.
    let stored = os_store::ai::answers::list_for_url(file.conn(), 1).unwrap();
    assert_eq!(stored[0].question_version, "1.0.0");
    assert_eq!(
        update_notice(Version::new(1, 0, 0), pt2.version).as_deref(),
        Some("Frage aktualisiert (1.0.0 → 1.1.0) – neu analysieren?")
    );
    assert_eq!(update_notice(pt2.version, pt2.version), None);
    assert_eq!(
        env.mock.posts(DECISIONS).len(),
        1,
        "kein Request ohne Neuanalyse"
    );
    // „Neu analysieren“ mit v2: Requests, neue Kopie in ai_questions, alte bleibt.
    let q2 = [ActiveQuestion {
        def: pt2.clone(),
        routing: RoutingSetting::Automatic,
    }];
    env.run(
        file.conn(),
        run,
        &q2,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    assert_eq!(env.mock.posts(DECISIONS).len(), 2);
    assert_eq!(
        os_store::ai::questions::count_versions(file.conn(), "page-type").unwrap(),
        2
    );
    let old = library::stored_version(file.conn(), "page-type", "1.0.0")
        .unwrap()
        .unwrap();
    assert_eq!(old, pt1);
}

// spec: AI-031 [M4]
// Die Crawl-Datei enthält für jede verwendete Fragenversion genau einen Eintrag in `ai_questions`; parametrisierte
// Systemfrage mit 30 Einträgen → 1 Eintrag (Vorlage) und 30 Antworten mit eigenen Parameterwerten; eine andere
// Kandidatenliste erhöht die Vorlagenversion nicht, eine Änderung des Vorlagentexts schon.
#[tokio::test]
async fn template_is_stored_once_with_answers_per_parameter_set() {
    let template = question("redirect-target");
    let env = Env::new(&["openai"]).await;
    let items: Vec<RunItem> = (1..=30)
        .map(|i| {
            let mut item = text_item(i, &format!("Entfernte Seite {i}"));
            let mut values = ParamValues::new();
            values.insert(
                "candidates".into(),
                ParamValue::Candidates(vec![Candidate {
                    address: format!("https://ai.test/ziel/{i}?a=ä"),
                    title: format!("Ziel {i}"),
                    h1: String::new(),
                    excerpt: String::new(),
                }]),
            );
            item.params.insert(template.id.clone(), values);
            item
        })
        .collect();
    let dir = TempDir::new("tpl");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let qs = [ActiveQuestion {
        def: template.clone(),
        routing: RoutingSetting::Automatic,
    }];
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    let conn = file.conn();
    assert_eq!(os_store::ai::questions::list(conn).unwrap().len(), 1);
    let stored = os_store::ai::questions::get(conn, "redirect-target", "1.0.0")
        .unwrap()
        .unwrap();
    assert_eq!(stored.definition_json, template.canonical_json());
    let answers = os_store::ai::answers::list_for_question(conn, "redirect-target", None).unwrap();
    assert_eq!(answers.len(), 30);
    let hashes: std::collections::BTreeSet<_> =
        answers.iter().map(|a| a.params_hash.clone()).collect();
    assert_eq!(hashes.len(), 30);
    assert!(answers.iter().all(|a| a.params_json.contains("candidates")));
    assert!(answers.iter().all(|a| a.status == "Success"));
    // Andere Kandidaten: gleiche Vorlagenversion; geänderter Text: neue Version.
    let mut edited = template.clone();
    assert_eq!(next_version(&template, &edited), template.version);
    edited.instructions.push_str(" Prefer the same topic.");
    assert_eq!(next_version(&template, &edited), Version::new(1, 1, 0));
}

// spec: AI-030 [M4]
// Vorlagen sind nur als Systemfragen erlaubt; eine Kopie bleibt eine Vorlage mit denselben `params`.
#[test]
fn templates_only_from_the_system_library() {
    let template = question("redirect-target");
    let mut lib = Library::default();
    let mut user = template.clone();
    user.id = "my-redirect".into();
    assert!(matches!(
        lib.save_user(user.clone()),
        Err(LibraryError::TemplateNotAllowed(_))
    ));
    let mut lib = Library::new(vec![template], vec![]).unwrap();
    assert!(lib.save_user(user).unwrap().is_template());
    assert!(lib.edit_copy("redirect-target").unwrap().is_template());
}

// spec: AI-030 [M4]
// CI-Test der Bibliothek: Jede mitgelieferte Frage unter `resources/ai/questions/library/` validiert gegen das Schema
// (Inhalte liefert M4-D03; ohne Verzeichnis prüft der Test nichts).
#[test]
fn shipped_library_validates() {
    let dir = repo_root().join("resources/ai/questions/library");
    if dir.is_dir() {
        let questions = load_dir(&dir).unwrap();
        Library::new(questions, vec![]).unwrap();
    }
}
