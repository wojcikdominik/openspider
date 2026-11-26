//! M4-D01: Fragenformat (AI-030) – Schema, Regeln beim Speichern, Vorlagen und Instanzen.
#[path = "decisions_support.rs"]
mod support;

use os_ai::decisions::format::{
    self, instantiate, parse, validate, validate_active, wire_name, Candidate, FieldError,
    ParamValue, ParamValues, QuestionDef,
};
use proptest::prelude::*;
use serde_json::Value;
use support::*;

fn fields(errors: &[FieldError]) -> Vec<String> {
    errors.iter().map(|e| e.field.clone()).collect()
}

// spec: AI-030 [M4]
// Jede gültige Fixture-Frage validiert gegen Schema und Regeln; das Schema liegt als Datei vor und ist das eingebaute.
#[test]
fn valid_fixtures_and_bundled_schema() {
    let dir = fixture("questions/valid");
    let mut n = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let q = parse(&text).unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
        // Export → Import ergibt dieselbe Definition.
        assert_eq!(parse(&q.canonical_json()).unwrap(), q);
        n += 1;
    }
    assert!(n >= 7);
    let file = std::fs::read_to_string(
        repo_root().join("resources/ai/questions/schema/question.schema.json"),
    )
    .unwrap();
    assert_eq!(file, format::SCHEMA_JSON);
    let _: Value = serde_json::from_str(&file).unwrap();
}

// spec: AI-030 [M4]
// Je ein Verstoß pro Datei (auch Vorlagenverstöße) wird beim Speichern abgelehnt, mit Feldangabe.
#[test]
fn every_invalid_fixture_is_rejected_with_a_field() {
    let dir = fixture("questions/invalid");
    let mut n = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let errors = parse(&text).expect_err(&path.display().to_string());
        assert!(!errors.is_empty());
        n += 1;
    }
    assert!(n >= 25, "{n} ungültige Fixtures");
    let read = |name: &str| parse(&fixture_text(&format!("questions/invalid/{name}.json")));
    assert!(fields(&read("bad-id").unwrap_err()).contains(&"/id".to_string()));
    assert!(fields(&read("name-too-long").unwrap_err()).contains(&"/name".to_string()));
    assert!(fields(&read("choice-duplicate-value").unwrap_err())
        .iter()
        .any(|f| f.starts_with("/choices")));
    assert!(
        fields(&read("instructions-too-long").unwrap_err()).contains(&"/instructions".to_string())
    );
}

fn base() -> QuestionDef {
    question("soft-404")
}

// spec: AI-030 [M4]
// `id` entspricht ^[a-z][a-z0-9-]{0,47}$; `name` 1–60 Zeichen; `instructions` 1–2.000 Zeichen (Zeichen, nicht Bytes).
#[test]
fn id_name_and_instruction_limits() {
    for (id, ok) in [
        ("a", true),
        ("page-type", true),
        ("a2-b3", true),
        (&*"a".repeat(48), true),
        (&*"a".repeat(49), false),
        ("2page", false),
        ("Page", false),
        ("page_type", false),
        ("-page", false),
        ("", false),
    ] {
        let mut q = base();
        q.id = id.to_string();
        assert_eq!(validate(&q).is_ok(), ok, "id {id:?}");
    }
    let mut q = base();
    q.name = "ä".repeat(60);
    assert!(validate(&q).is_ok());
    q.name = "ä".repeat(61);
    assert!(validate(&q).is_err());
    q = base();
    q.instructions = "ü".repeat(2000);
    assert!(validate(&q).is_ok());
    q.instructions = "ü".repeat(2001);
    assert!(validate(&q).is_err());
}

// spec: AI-030 [M4]
// `choice` 2–20 eindeutige Werte nach ^[a-z0-9_-]{1,32}$, `score` 2–10 eindeutige Labels, `predicate` ohne beides.
#[test]
fn value_and_level_rules() {
    let mut q = question("page-type");
    let make = |n: usize| -> Vec<format::ChoiceDef> {
        (0..n)
            .map(|i| format::ChoiceDef {
                value: format!("v{i}"),
                description: String::new(),
            })
            .collect()
    };
    for (n, ok) in [(1, false), (2, true), (20, true), (21, false)] {
        q.choices = Some(make(n));
        assert_eq!(validate(&q).is_ok(), ok, "{n} Werte");
    }
    q.choices = Some(make(2));
    q.choices.as_mut().unwrap()[1].value = "a".repeat(33);
    assert!(validate(&q).is_err());
    q.choices.as_mut().unwrap()[1].value = "a_b-1".into();
    assert!(validate(&q).is_ok());
    let mut s = question("quality");
    for (n, ok) in [(1, false), (2, true), (10, true), (11, false)] {
        s.levels = Some(
            (0..n)
                .map(|i| format::LevelDef {
                    label: format!("L{i}"),
                    description: String::new(),
                })
                .collect(),
        );
        assert_eq!(validate(&s).is_ok(), ok, "{n} Levels");
    }
    let mut p = base();
    p.choices = Some(make(2));
    assert!(fields(&validate(&p).unwrap_err()).contains(&"/choices".to_string()));
}

// spec: AI-030 [M4]
// `subject = image` erlaubt nur Bild-Eingaben plus Kontextfelder; `subject = page` keine Bild-Kontextfelder.
#[test]
fn subject_and_input_fields() {
    let alt = question("alt-fits");
    assert!(validate(&alt).is_ok());
    let read = |name: &str| parse(&fixture_text(&format!("questions/invalid/{name}.json")));
    assert!(read("image-without-image").is_err());
    assert!(read("image-with-page-field").is_err());
    assert!(read("page-with-alt").is_err());
    // Ausdrücklich gewähltes Seitenbild in einer Seitenfrage ist erlaubt.
    let mut page = base();
    page.inputs
        .push(format::InputSpec::field(os_ai::policy::InputField::Image));
    assert!(validate(&page).is_ok());
}

// spec: AI-030 [M4]
// `name` eindeutig unter allen aktiven Fragen (sonst Speicherfehler mit Feldangabe); Fragenname für Anbieter aus `id`.
#[test]
fn names_unique_among_active_questions_and_wire_name() {
    let a = question("page-type");
    let mut b = question("soft-404");
    assert!(validate_active(&[a.clone(), b.clone()]).is_ok());
    b.name = a.name.clone();
    let errors = validate_active(&[a.clone(), b]).unwrap_err();
    assert_eq!(errors[0].field, "/1/name");
    assert_eq!(wire_name("page-type"), "page_type");
    assert_eq!(a.wire_name(), "page_type");
    assert_eq!(a.to_wire().name, "page_type");
}

// spec: AI-030 [M4]
// Vorlagen: Platzhalter in `value`, `label` oder `name` sowie ohne passenden Eintrag in `params` werden abgelehnt;
// eine Kandidaten-Vorlage mit anderen Werten als c1…ck, none ebenfalls.
#[test]
fn template_rules() {
    assert!(question("lang-match").is_template());
    assert!(question("redirect-target").is_template());
    let read = |name: &str| parse(&fixture_text(&format!("questions/invalid/{name}.json")));
    for name in [
        "placeholder-in-value",
        "placeholder-in-label",
        "placeholder-in-name",
        "placeholder-without-param",
        "candidates-wrong-values",
        "candidates-without-none",
        "candidates-in-instructions",
    ] {
        assert!(read(name).is_err(), "{name}");
    }
}

fn candidate_strategy() -> impl Strategy<Value = Candidate> {
    let text = "[a-zA-Z0-9äöüÄÖÜß /?&=%#{}.:-]{0,40}";
    (text, text, text, text).prop_map(|(path, title, h1, excerpt)| Candidate {
        address: format!("https://ai.test/ü/{path}?q=ä&x={{{{1}}}}"),
        title,
        h1,
        excerpt,
    })
}

proptest! {
    // spec: AI-030 [M4]
    // Nach dem Einsetzen der Parameter ist jede Instanz eine gültige Frage (Vorlage aus AI-082 mit k = 1…10
    // Kandidaten, Adressen mit Sonderzeichen, Umlauten und Query-Strings).
    #[test]
    fn every_instance_is_a_valid_question(cands in proptest::collection::vec(candidate_strategy(), 1..=10)) {
        let template = question("redirect-target");
        let mut values = ParamValues::new();
        values.insert("candidates".into(), ParamValue::Candidates(cands.clone()));
        let instance = instantiate(&template, &values).unwrap();
        prop_assert!(format::validate_instance(&instance).is_ok());
        prop_assert!(format::schema_errors(&serde_json::to_value(&instance).unwrap()).is_ok());
        prop_assert!(!instance.is_template());
        let choices = instance.choices.as_ref().unwrap();
        prop_assert_eq!(choices.len(), cands.len() + 1);
        for (i, c) in cands.iter().enumerate() {
            prop_assert_eq!(&choices[i].value, &format!("c{}", i + 1));
            prop_assert!(choices[i].description.contains(&c.address));
        }
        prop_assert_eq!(&choices[cands.len()].value, "none");
        prop_assert_eq!(instance.version, template.version);
    }
}

// spec: AI-030 [M4]
// Zeichenkettenparameter: Einsetzen in `instructions`; fehlender Wert wird abgelehnt; mehr als k Kandidaten ebenso.
#[test]
fn string_parameters_and_wrong_values() {
    let template = question("lang-match");
    let mut values = ParamValues::new();
    values.insert("lang".into(), ParamValue::String("fr".into()));
    let instance = instantiate(&template, &values).unwrap();
    assert!(instance.instructions.contains("language fr"));
    assert!(instantiate(&template, &ParamValues::new()).is_err());
    let mut too_many = ParamValues::new();
    too_many.insert(
        "candidates".into(),
        ParamValue::Candidates(
            (0..11)
                .map(|i| Candidate {
                    address: format!("https://ai.test/{i}"),
                    title: String::new(),
                    h1: String::new(),
                    excerpt: String::new(),
                })
                .collect(),
        ),
    );
    assert!(instantiate(&question("redirect-target"), &too_many).is_err());
}
