//! M4-F02, OUT-203 (OS-Festlegung Schalter „KI (Vorschau)“, Nutzerentscheidung 23.11.2026): Solange der Schalter in
//! den App-Einstellungen aus ist (Standard), sind die KI-Tabs und alle Namen `<KI-Tab>:<Name>` unbekannt. Ein
//! ausdrücklich genannter KI-Name → Exit-Code 2 vor dem Crawl mit der Meldung für einen unbekannten Namen, ohne
//! Netzwerkzugriff; weder Vorschlag noch Liste nennen einen KI-Namen. Die Annahme bei eingeschaltetem Schalter prüfen
//! die Plan-Tests in `src/plan.rs` (kein Crawl nötig).
mod common;

use common::{resolve, site, Env};
use os_ai::PreviewSwitch;
use os_cli::registry::{self, Kind};

const AI_TABS: [&str; 4] = ["AI", "AI Decisions", "AI Suggestions", "AI Search"];

fn is_ai(name: &str) -> bool {
    let group = name.split_once(':').map_or(name, |(g, _)| g).trim();
    AI_TABS.contains(&group)
}

/// Erster Name, den es nur mit eingeschaltetem Schalter gibt.
fn first_ai(on: Vec<String>) -> String {
    on.into_iter()
        .find(|n| is_ai(n))
        .expect("KI-Name im Register")
}

/// Kein KI-Name in stderr außer der zitierten Eingabe selbst.
fn assert_no_ai_name(stderr: &str, input: &str) {
    let quoted = format!("'{input}'");
    for line in stderr.lines() {
        let line = line.replace(&quoted, "''");
        for tab in AI_TABS {
            assert!(
                !line.contains(&format!("{tab}:")) && line.trim() != tab,
                "KI-Name in der Ausgabe: {line}\n{stderr}"
            );
        }
    }
}

// spec: OUT-203
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_the_preview_switch_ai_names_are_unknown_before_any_request() {
    let site = site("out-basic-site").await;
    // Fixture: Datenordner mit `settings.toml` ohne Abschnitt `ai_app` → Schalter aus (Standard).
    let env = Env::new();
    let cases = [
        (
            "--export-tabs",
            first_ai(registry::names_with(Kind::Tab, PreviewSwitch::ON)),
            "unknown export",
        ),
        (
            "--bulk-export",
            first_ai(registry::names_with(Kind::Bulk, PreviewSwitch::ON)),
            "unknown export",
        ),
        (
            "--save-report",
            first_ai(registry::names_with(Kind::Report, PreviewSwitch::ON)),
            "unknown report",
        ),
        (
            "--export-custom-summary",
            first_ai(registry::metric_names_with(PreviewSwitch::ON)),
            "unknown metric",
        ),
    ];
    for (flag, name, message) in cases {
        for input in [name.clone(), "AI:All".to_string()] {
            if flag == "--export-custom-summary" && input == "AI:All" {
                continue;
            }
            let run = env
                .run_async(vec![
                    "--crawl".to_string(),
                    "http://site.test/".to_string(),
                    "--resolve".to_string(),
                    resolve(&site),
                    "--output-folder".to_string(),
                    env.out().to_string_lossy().into_owned(),
                    flag.to_string(),
                    input.clone(),
                ])
                .await;
            assert_eq!(run.code, 2, "{flag} {input}: {}", run.dump());
            assert!(
                run.stderr.contains(&format!("{message} '{input}'")),
                "{flag} {input}: {}",
                run.dump()
            );
            assert_no_ai_name(&run.stderr, &input);
        }
    }
    // Ein KI-Name ohne ähnlichen Nicht-KI-Namen: die ausgegebene Liste (wie `--list-reports`) nennt keinen KI-Namen.
    let input = "AI Suggestions:Zzzzzzzzzzzz";
    let run = env
        .run_async(vec![
            "--crawl".to_string(),
            "http://site.test/".to_string(),
            "--resolve".to_string(),
            resolve(&site),
            "--output-folder".to_string(),
            env.out().to_string_lossy().into_owned(),
            "--save-report".to_string(),
            input.to_string(),
        ])
        .await;
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(run.stderr.contains("no similar name"), "{}", run.dump());
    assert!(
        run.stderr.lines().any(|l| l == "Crawl Overview"),
        "{}",
        run.dump()
    );
    assert_no_ai_name(&run.stderr, input);
    assert_eq!(site.total_requests(), 0);
    assert!(env.crawl_files().is_empty());
}
