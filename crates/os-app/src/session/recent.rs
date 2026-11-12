//! CRAWL-127 „Zuletzt geöffnet“: eine Crawl-Datei erscheint genau einmal, gleich über welche Pfadschreibweise sie
//! geöffnet oder angelegt wurde (z. B. `/tmp/…` und `/private/tmp/…` unter macOS).
//!
//! Verglichen wird über den aufgelösten Pfad ([`identity`]); angezeigt wird weiterhin die Schreibweise, die der Nutzer
//! kennt: unter mehreren Schreibweisen derselben Datei gewinnt die nicht aufgelöste (eingegebene oder konfigurierte)
//! vor der vom Kern aufgelösten, und eine aufgelöste Windows-Form `\\?\…` wird nie angezeigt. Die Position des
//! zusammengeführten Eintrags ist die des jüngsten (die Liste ist neueste zuerst).

use serde_json::Value;
use std::path::{Path, PathBuf};

/// Höchstzahl der Einträge (CRAWL-127: 10 Einträge).
pub const MAX_RECENT: usize = 10;
/// Höchstens so viele gespeicherte Einträge werden aufgelöst (Schutz vor ausufernden Listen).
const MAX_SCANNED: usize = 4 * MAX_RECENT;
/// Schlüssel der Liste im UI-Zustand.
pub const RECENT_KEY: &str = "recentCrawls";

/// Vergleichsschlüssel einer Pfadschreibweise: der aufgelöste Pfad. Fehlt die Datei, der aufgelöste Elternordner mit
/// dem Dateinamen; fehlt auch er, der Pfad wie geschrieben. Schlägt nie fehl.
pub fn identity(path: &str) -> PathBuf {
    let path = Path::new(path);
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return resolved;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if !parent.as_os_str().is_empty() {
            if let Ok(parent) = std::fs::canonicalize(parent) {
                return parent.join(name);
            }
        }
    }
    path.to_path_buf()
}

/// Entfernt das Präfix der erweiterten Windows-Pfadform (`\\?\C:\…` → `C:\…`, `\\?\UNC\srv\…` → `\\srv\…`).
/// Andere Formen bleiben unverändert. Reine Zeichenkettenfunktion, damit sie auf jeder Plattform testbar ist.
pub fn strip_verbatim(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    match path.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_owned(),
        _ => path.to_owned(),
    }
}

/// Anzeigeform einer gespeicherten Schreibweise: unter Windows ohne `\\?\`-Präfix, sonst unverändert.
pub fn display_form(path: &str) -> String {
    if cfg!(windows) {
        strip_verbatim(path)
    } else {
        path.to_owned()
    }
}

/// Die zu merkende Schreibweise einer Datei, die als `requested` geöffnet und zu `resolved` aufgelöst wurde und als
/// `opened` geöffnet ist: die eigene absolute Schreibweise des Nutzers, wenn sie genau die geöffnete Datei nennt;
/// sonst (relativer Pfad, migrierte Kopie) die geöffnete Datei in Anzeigeform.
pub fn remembered_spelling(requested: &Path, resolved: &Path, opened: &str) -> String {
    if requested.is_absolute() && Path::new(opened) == resolved {
        requested.to_string_lossy().into_owned()
    } else {
        display_form(opened)
    }
}

struct Group {
    key: PathBuf,
    entry: serde_json::Map<String, Value>,
    path: String,
    user_spelling: bool,
}

/// Führt Einträge derselben Datei zusammen (neueste zuerst bleibt neueste zuerst, höchstens [`MAX_RECENT`]).
/// Der zusammengeführte Eintrag steht an der Position des jüngsten; fehlende Felder (z. B. `label`) ergänzt ein
/// älterer Eintrag derselben Datei. Einträge ohne Pfad entfallen; fehlende Dateien bleiben erhalten.
pub fn merge(entries: &[Value]) -> Vec<Value> {
    let mut groups: Vec<Group> = Vec::new();
    for entry in entries.iter().take(MAX_SCANNED) {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let Some(path) = object
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
        else {
            continue;
        };
        let key = identity(path);
        let user_spelling = Path::new(path) != key;
        if let Some(group) = groups.iter_mut().find(|g| g.key == key) {
            if user_spelling && !group.user_spelling {
                group.path = path.to_owned();
                group.user_spelling = true;
            }
            for (name, value) in object {
                group
                    .entry
                    .entry(name.clone())
                    .or_insert_with(|| value.clone());
            }
            continue;
        }
        groups.push(Group {
            key,
            entry: object.clone(),
            path: path.to_owned(),
            user_spelling,
        });
    }
    groups
        .into_iter()
        .take(MAX_RECENT)
        .map(|mut group| {
            group
                .entry
                .insert("path".into(), Value::String(display_form(&group.path)));
            Value::Object(group.entry)
        })
        .collect()
}

/// Führt die Liste „Zuletzt geöffnet“ im UI-Zustand zusammen, falls vorhanden (Laden, Speichern, Merken).
pub fn merge_in_state(state: &mut Value) {
    let Some(object) = state.as_object_mut() else {
        return;
    };
    if let Some(Value::Array(list)) = object.get(RECENT_KEY) {
        let merged = merge(list);
        object.insert(RECENT_KEY.into(), Value::Array(merged));
    }
}

/// Setzt `path` an die Spitze der Liste und führt zusammen.
pub fn remember(state: &mut Value, path: &str) {
    let Some(object) = state.as_object_mut() else {
        return;
    };
    let mut list = vec![serde_json::json!({ "path": path })];
    if let Some(Value::Array(old)) = object.get(RECENT_KEY) {
        list.extend(old.iter().cloned());
    }
    object.insert(RECENT_KEY.into(), Value::Array(merge(&list)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // spec: CRAWL-127
    #[test]
    fn verbatim_windows_forms_are_not_displayed() {
        assert_eq!(
            strip_verbatim(r"\\?\C:\Crawls\a.ospider"),
            r"C:\Crawls\a.ospider"
        );
        assert_eq!(
            strip_verbatim(r"\\?\UNC\server\share\a.ospider"),
            r"\\server\share\a.ospider"
        );
        assert_eq!(strip_verbatim(r"\\?\Volume{x}\a"), r"\\?\Volume{x}\a");
        assert_eq!(strip_verbatim("/tmp/a.ospider"), "/tmp/a.ospider");
    }

    // spec: CRAWL-127
    #[test]
    fn missing_files_keep_their_entry_and_identical_spellings_merge() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("gone.ospider");
        let gone = gone.to_string_lossy().into_owned();
        let merged = merge(&[
            json!({"path": gone}),
            json!({"path": "relative.ospider", "label": "x"}),
            json!({"path": gone, "label": "older"}),
            json!({"path": ""}),
            json!("no object"),
        ]);
        assert_eq!(
            merged,
            vec![
                json!({"path": gone, "label": "older"}),
                json!({"path": "relative.ospider", "label": "x"}),
            ]
        );
    }

    // spec: CRAWL-127
    #[test]
    fn at_most_ten_distinct_entries_newest_first() {
        let list: Vec<Value> = (0..15)
            .map(|n| json!({"path": format!("missing-{n}.ospider")}))
            .collect();
        let merged = merge(&list);
        assert_eq!(merged.len(), MAX_RECENT);
        assert_eq!(merged[0]["path"], "missing-0.ospider");
    }
}
