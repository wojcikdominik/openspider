//! Unabhängige Abnahme M4-D01 – Speicherung der Fragenversionen (`ai_questions`) und Antworten (`ai_answers`) in der
//! Crawl-Datei (AI-031, AI-030 parametrisierte Systemfragen; 08 „Datenhaltung in der Crawl-Datei“).
//! Erwartungen nur aus `docs/spec/08-ai.md`.
// acceptance: independent

use os_store::ai::answers::{self, AnswerRow};
use os_store::ai::questions::{self, Ensured};

fn crawl_file(dir: &std::path::Path) -> os_store::CrawlFile {
    use os_model::time::{IdSource, SeededIds};
    let created = os_model::Timestamp::from_db("2026-11-26T10:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: SeededIds::new(1002).new_crawl_id(created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "Abnahme".into(),
        name: "D01 Store".into(),
        config: os_model::CrawlConfig::default(),
        created_at: created,
    };
    let f = os_store::CrawlFile::create(
        &dir.join("d01-store.ospider"),
        &init,
        os_store::StorageProfile::Balanced,
    )
    .unwrap();
    f.conn()
        .execute_batch(
            "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length)
               VALUES(1,'https://ai.test:8080/a','https://ai.test:8080/a','https://ai.test:8080',1,'Done','Seed',1,22),
                     (2,'https://ai.test:8080/b','https://ai.test:8080/b','https://ai.test:8080',1,'Done','Seed',1,22);",
        )
        .unwrap();
    f
}

const TEMPLATE_V1: &str = r#"{"id":"redirect-target","version":"1.0.0","name":"Redirect Target","type":"choice","instructions":"Which candidate fits?","choices":[{"value":"c1","description":"{{candidates}}"},{"value":"none","description":"No candidate."}],"subject":"page","inputs":["url"],"params":[{"name":"candidates","type":"candidates","source":"redirect.candidates"}]}"#;

// spec: AI-031 [M4]
// acceptance: independent
#[test]
fn ai031_one_immutable_row_per_used_version() {
    let dir = tempfile::tempdir().unwrap();
    let f = crawl_file(dir.path());
    let conn = f.conn();
    assert_eq!(
        questions::ensure(conn, "redirect-target", "1.0.0", TEMPLATE_V1).unwrap(),
        Ensured::Inserted
    );
    assert_eq!(
        questions::ensure(conn, "redirect-target", "1.0.0", TEMPLATE_V1).unwrap(),
        Ensured::Present
    );
    assert_eq!(questions::count_versions(conn, "redirect-target").unwrap(), 1);
    // Gleiche Version, andere Definition → Fehler, die Kopie bleibt.
    let changed = TEMPLATE_V1.replace("Which candidate fits?", "Changed?");
    assert!(questions::ensure(conn, "redirect-target", "1.0.0", &changed).is_err());
    let stored = questions::get(conn, "redirect-target", "1.0.0")
        .unwrap()
        .expect("gespeichert");
    assert_eq!(stored.definition_json, TEMPLATE_V1);
    // Neue Version: zweite Zeile, die erste bleibt.
    let v2 = changed.replace("\"1.0.0\"", "\"1.1.0\"");
    assert_eq!(
        questions::ensure(conn, "redirect-target", "1.1.0", &v2).unwrap(),
        Ensured::Inserted
    );
    assert_eq!(questions::count_versions(conn, "redirect-target").unwrap(), 2);
    assert_eq!(questions::list(conn).unwrap().len(), 2);
    // Änderung oder Löschung über SQL ist ausgeschlossen bzw. ändert die Kopie nicht.
    assert!(conn
        .execute_batch("UPDATE ai_questions SET definition_json = '{}' WHERE version = '1.0.0';")
        .is_err());
    assert_eq!(
        questions::get(conn, "redirect-target", "1.0.0")
            .unwrap()
            .unwrap()
            .definition_json,
        TEMPLATE_V1
    );
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_answers_per_parameter_set_on_one_template_version() {
    let dir = tempfile::tempdir().unwrap();
    let f = crawl_file(dir.path());
    let conn = f.conn();
    questions::ensure(conn, "redirect-target", "1.0.0", TEMPLATE_V1).unwrap();
    let params_a = r#"{"candidates":[{"address":"https://ai.test:8080/x"}]}"#;
    let params_b = r#"{"candidates":[{"address":"https://ai.test:8080/y"}]}"#;
    let rows = [
        AnswerRow::status_only(1, "redirect-target", "1.0.0", "hash-a", params_a, "Pending"),
        AnswerRow::status_only(1, "redirect-target", "1.0.0", "hash-b", params_b, "Pending"),
        AnswerRow::status_only(2, "redirect-target", "1.0.0", "hash-a", params_a, "Skipped (scope)"),
    ];
    answers::upsert_all(conn, &rows).unwrap();
    let listed = answers::list_for_question(conn, "redirect-target", Some("1.0.0")).unwrap();
    assert_eq!(listed.len(), 3, "eine Zeile je (URL, Frage, Version, Parameter)");
    // Upsert ersetzt dieselbe Zeile.
    let mut done = rows[0].clone();
    done.status = "Success".into();
    done.answer_json = Some(r#"{"type":"choice","choice":"c1"}"#.into());
    answers::upsert(conn, &done).unwrap();
    let again = answers::get(conn, 1, "redirect-target", "1.0.0", "hash-a")
        .unwrap()
        .unwrap();
    assert_eq!(again.status, "Success");
    assert_eq!(again.params_json, params_a);
    let counts = answers::status_counts(conn, "redirect-target", "1.0.0").unwrap();
    assert_eq!(counts.get("Success"), Some(&1));
    assert_eq!(counts.get("Pending"), Some(&1));
    assert_eq!(counts.get("Skipped (scope)"), Some(&1));
    // Die Vorlage steht weiterhin genau einmal in ai_questions.
    assert_eq!(questions::count_versions(conn, "redirect-target").unwrap(), 1);
}
