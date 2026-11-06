//! JSON-Schema der `.osconfig` (OUT-221), erzeugt aus den Strukturen mit `schemars`.
//!
//! Das Schema wird zur Laufzeit gebaut und um die Wertebereiche aus `validate::INT_RANGES` ergänzt. Die
//! eingecheckte Datei `docs/spec/schemas/osconfig.schema.json` ist die Ausgabe von [`json_schema_string`];
//! ein Test hält beide synchron (`UPDATE_GOLDEN=1` schreibt sie neu). Unbekannte Schlüssel sind absichtlich
//! erlaubt (Vorwärtskompatibilität, CRAWL-132): das Schema setzt kein `additionalProperties: false`.
//!
//! Außerdem liefert das Modul die Schema-Navigation, auf der Laden (unbekannte Schlüssel) und `set`
//! (Typ eines Schlüssels) aufsetzen.

use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use crate::config::validate::{int_range, FLOAT_RANGES, INT_RANGES};
use crate::config::CrawlConfig;

static SCHEMA: OnceLock<Value> = OnceLock::new();

/// Das JSON-Schema der Crawl-Konfiguration (Draft 2020-12).
pub fn json_schema() -> &'static Value {
    SCHEMA.get_or_init(build)
}

/// Das Schema als formatierter JSON-Text mit abschließendem Zeilenumbruch (Inhalt der eingecheckten Datei).
pub fn json_schema_string() -> String {
    let mut s = serde_json::to_string_pretty(json_schema()).unwrap_or_default();
    s.push('\n');
    s
}

fn build() -> Value {
    let mut v = serde_json::to_value(schemars::schema_for!(CrawlConfig)).unwrap_or(Value::Null);
    if let Some(root) = v.as_object_mut() {
        root.insert("title".into(), json!("OpenSpider .osconfig"));
        root.insert(
            "description".into(),
            json!(
                "Crawl-Konfiguration von OpenSpider (TOML, OUT-221). Fehlende Schlüssel erhalten Standardwerte; \
                 unbekannte Schlüssel werden beim Laden gemeldet und ignoriert."
            ),
        );
    }
    // Der Standardwert von `generator` ist die Version des schreibenden Programms; im Schema stünde sonst
    // je Release ein anderer Wert. Die Datei unter docs/spec bleibt dadurch versionsunabhängig.
    if let Some(node) = property_node_mut(&mut v, "generator") {
        node.remove("default");
    }
    for range in INT_RANGES {
        if let Some(node) = property_node_mut(&mut v, range.path) {
            node.insert("minimum".into(), json!(range.min));
            match range.max {
                Some(max) => node.insert("maximum".into(), json!(max)),
                None => node.remove("maximum"),
            };
        }
    }
    for range in FLOAT_RANGES {
        if let Some(node) = property_node_mut(&mut v, range.path) {
            node.insert("minimum".into(), json!(range.min));
            node.insert("maximum".into(), json!(range.max));
            if let Some(multiple) = range.multiple {
                node.insert("multipleOf".into(), json!(multiple));
            }
        }
    }
    if let Some(node) = property_node_mut(&mut v, "speed.max_urls_per_sec") {
        node.insert("exclusiveMinimum".into(), json!(0));
    }
    v
}

/// Der Knoten der Eigenschaft `path` (Punktnotation) mit Auflösung von `$ref` auf dem Weg dorthin.
fn property_node_mut<'a>(root: &'a mut Value, path: &str) -> Option<&'a mut Map<String, Value>> {
    let mut ptr = String::new();
    for seg in path.split('.') {
        if let Some(r) = root
            .pointer(&ptr)
            .and_then(|n| n.get("$ref"))
            .and_then(Value::as_str)
        {
            ptr = r.trim_start_matches('#').to_string();
        }
        ptr = format!("{ptr}/properties/{seg}");
    }
    root.pointer_mut(&ptr).and_then(Value::as_object_mut)
}

/// Folgt `$ref`, `allOf` mit einem Element und `anyOf`/`oneOf` der Form „Typ oder null“ bis zum eigentlichen
/// Knoten. Der zweite Wert sagt, ob `null` erlaubt ist.
pub(crate) fn deref<'a>(root: &'a Value, node: &'a Value) -> (&'a Value, bool) {
    let mut node = node;
    let mut nullable = false;
    for _ in 0..16 {
        if let Some(r) = node.get("$ref").and_then(Value::as_str) {
            match root.pointer(r.trim_start_matches('#')) {
                Some(t) => {
                    node = t;
                    continue;
                }
                None => break,
            }
        }
        if let Some([only]) = node
            .get("allOf")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
        {
            node = only;
            continue;
        }
        let alts = node
            .get("anyOf")
            .or_else(|| node.get("oneOf"))
            .and_then(Value::as_array);
        if let Some(alts) = alts {
            let is_null = |a: &Value| a.get("type").and_then(Value::as_str) == Some("null");
            if alts.iter().any(is_null) {
                nullable = true;
                let mut rest = alts.iter().filter(|a| !is_null(a));
                if let (Some(first), None) = (rest.next(), rest.next()) {
                    node = first;
                    continue;
                }
            }
        }
        break;
    }
    if node
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|t| t.iter().any(|x| x.as_str() == Some("null")))
    {
        nullable = true;
    }
    (node, nullable)
}

/// `properties` eines Objekt-Knotens (nach Auflösung).
pub(crate) fn object_properties<'a>(
    root: &'a Value,
    node: &'a Value,
) -> Option<&'a Map<String, Value>> {
    deref(root, node)
        .0
        .get("properties")
        .and_then(Value::as_object)
}

/// Elementknoten eines Arrays (nach Auflösung).
pub(crate) fn array_items<'a>(root: &'a Value, node: &'a Value) -> Option<&'a Value> {
    deref(root, node).0.get("items")
}

/// Zulässige Zeichenketten eines Enum-Knotens (`enum` oder `oneOf` aus `const`).
pub(crate) fn enum_values(node: &Value) -> Option<Vec<String>> {
    if let Some(list) = node.get("enum").and_then(Value::as_array) {
        return list
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect();
    }
    let alts = node.get("oneOf").and_then(Value::as_array)?;
    alts.iter()
        .map(|a| a.get("const").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// Ob `type` den Namen `t` enthält (`"type": "integer"` oder `["integer","null"]`).
pub(crate) fn has_type(node: &Value, t: &str) -> bool {
    match node.get("type") {
        Some(Value::String(s)) => s == t,
        Some(Value::Array(a)) => a.iter().any(|x| x.as_str() == Some(t)),
        _ => false,
    }
}

/// Grenzen einer Ganzzahl-Option aus dem Schema: Tabelle zuerst, sonst `format`/`minimum`/`maximum`.
pub(crate) fn integer_bounds(path: &str, node: &Value) -> (i64, Option<i64>) {
    if let Some(r) = int_range(path) {
        return (r.min, r.max);
    }
    let min = node.get("minimum").and_then(Value::as_i64).unwrap_or(0);
    let max = node.get("maximum").and_then(Value::as_i64).or_else(|| {
        match node.get("format").and_then(Value::as_str) {
            Some("uint32") => Some(i64::from(u32::MAX)),
            Some("uint64") => Some(i64::MAX), // TOML-Ganzzahlen sind i64
            _ => None,
        }
    });
    (min, max)
}
