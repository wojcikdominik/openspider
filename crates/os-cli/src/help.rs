//! Hilfe und Auflistungen (OUT-181, OUT-203): `--help`, `--help <thema>`, `--list-exports`, `--list-reports`.
//!
//! Alle Listen kommen aus dem Register [`crate::registry`]; die Ausgabe steht auf stdout, eine Zeile je
//! Name, mit `--json` als JSON.

use serde_json::{json, Value};

use crate::exit::CliError;
use crate::registry::{self, Kind};

/// Themen von `--help <thema>`.
pub const TOPICS: [&str; 5] = [
    "export-tabs",
    "bulk-export",
    "save-report",
    "export-custom-summary",
    "set",
];

/// Gesamthilfe (gruppiert nach Quelle, Konfiguration, Vergleich, APIs, Exporte, OS-Erweiterungen).
pub fn general() -> String {
    // Plan M4 E1 (E-2 b, M4-F02): KI-Optionen nur mit Schalter „KI (Vorschau)“.
    let mut cmd = crate::args::command_for(registry::preview());
    let mut text = cmd.render_long_help().to_string();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "\nHilfe zu Namen: --help {}\n",
        TOPICS.join(" | --help ")
    ));
    text
}

fn lines(items: &[String]) -> String {
    let mut out = String::new();
    for item in items {
        out.push_str(item);
        out.push('\n');
    }
    out
}

fn json_text(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    text
}

/// Liste der Namen zu einem Thema.
pub fn topic(name: &str, as_json: bool) -> Result<String, CliError> {
    let items: Vec<String> = match name.trim().to_ascii_lowercase().as_str() {
        "export-tabs" => registry::names(Kind::Tab),
        "bulk-export" => registry::names(Kind::Bulk),
        "save-report" => registry::names(Kind::Report),
        "export-custom-summary" => registry::metric_names_with(registry::preview()),
        "set" => os_model::settable_keys().to_vec(),
        other => {
            return Err(CliError::usage(format!(
                "unbekanntes Hilfethema '{other}' – gültig: {}",
                TOPICS.join(", ")
            )))
        }
    };
    Ok(if as_json {
        json_text(&json!(items))
    } else {
        lines(&items)
    })
}

/// `--list-exports`: Namen für `--export-tabs` und `--bulk-export`, gemeinsam sortiert.
pub fn list_exports(as_json: bool) -> String {
    let tabs = registry::names(Kind::Tab);
    let bulk = registry::names(Kind::Bulk);
    if as_json {
        json_text(&json!({ "export-tabs": tabs, "bulk-export": bulk }))
    } else {
        let mut names = tabs;
        names.extend(bulk);
        names.sort();
        names.dedup();
        lines(&names)
    }
}

/// `--list-reports`: Namen für `--save-report`.
pub fn list_reports(as_json: bool) -> String {
    let reports = registry::names(Kind::Report);
    if as_json {
        json_text(&json!(reports))
    } else {
        lines(&reports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: OUT-181
    #[test]
    fn the_general_help_groups_the_flags_and_marks_os_extensions() {
        let help = general();
        for heading in [
            "Quelle:",
            "Konfiguration:",
            "Vergleich:",
            "APIs:",
            "Exporte:",
            "OS-Erweiterungen:",
        ] {
            assert!(help.contains(heading), "Abschnitt {heading} fehlt:\n{help}");
        }
        let os_section = help.split("OS-Erweiterungen:").nth(1).unwrap();
        assert!(os_section.contains("--resolve") && os_section.contains("--strict"));
        assert!(os_section.contains("[OS-Erweiterung]"));
    }

    // spec: OUT-203
    #[test]
    fn list_exports_is_the_sorted_union_of_both_help_topics() {
        let expected = format!(
            "{}{}",
            topic("export-tabs", false).unwrap(),
            topic("bulk-export", false).unwrap()
        );
        let mut names: Vec<_> = expected.lines().map(str::to_string).collect();
        names.sort();
        names.dedup();
        assert_eq!(list_exports(false), lines(&names));
        assert_eq!(list_reports(false), topic("save-report", false).unwrap());
    }

    #[test]
    fn unknown_topics_are_usage_errors() {
        let err = topic("nope", false).unwrap_err();
        assert_eq!(err.exit, crate::exit::Exit::Usage);
        assert!(err.message.contains("export-tabs"));
    }
}
