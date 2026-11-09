use super::marker;
use serde_json::{json, Value};
use std::{collections::BTreeMap, io, path::Path};

/// Fragment format: {"MOCK-ID": {"question-or-task-id": <answer>, "default": <answer>}}.
#[derive(Default)]
pub struct AnswerBook(BTreeMap<String, BTreeMap<String, Value>>);
impl AnswerBook {
    pub fn load(dir: impl AsRef<Path>) -> io::Result<Self> {
        let mut paths = std::fs::read_dir(dir)?
            .map(|e| e.map(|e| e.path()))
            .collect::<io::Result<Vec<_>>>()?;
        paths.sort();
        let mut book = Self::default();
        for path in paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
        {
            let fragment: BTreeMap<String, BTreeMap<String, Value>> =
                serde_json::from_slice(&std::fs::read(&path)?)?;
            for (id, entries) in fragment {
                for (question, answer) in entries {
                    if book
                        .0
                        .entry(id.clone())
                        .or_default()
                        .insert(question.clone(), answer)
                        .is_some()
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "Duplicate MOCK-ID {id}, question/task {question} in {}",
                                path.display()
                            ),
                        ));
                    }
                }
            }
        }
        Ok(book)
    }
    pub(crate) fn answer(&self, input: &Value, question: &str) -> Value {
        marker(&input.to_string(), "MOCK-ID:")
            .and_then(|id| self.0.get(&id))
            .and_then(|entries| entries.get(question).or_else(|| entries.get("default")))
            .cloned()
            .unwrap_or_else(|| json!({"ok":true}))
    }
    pub(crate) fn vector(&self, text: &str, dimensions: usize) -> Vec<f64> {
        let input = json!(text);
        let preset = self.answer(&input, "embedding");
        if let Ok(v) = serde_json::from_value::<Vec<f64>>(preset) {
            if v.len() == dimensions {
                return v;
            }
        }
        embedding(text, dimensions)
    }
}
/// Count only provider text blocks, never model names, schema metadata or base64 bytes.
pub fn token_count(input: &Value) -> usize {
    fn parts(v: &Value) -> (usize, usize) {
        match v {
            Value::String(s) => (s.chars().count(), 0),
            Value::Array(a) => a
                .iter()
                .map(parts)
                .fold((0, 0), |(a, b), (c, d)| (a + c, b + d)),
            Value::Object(o) => {
                if o.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| matches!(t, "image" | "input_image" | "image_url"))
                {
                    return (0, 1);
                }
                let mut total = (0, 0);
                for key in [
                    "system",
                    "instructions",
                    "messages",
                    "input",
                    "content",
                    "text",
                    "prompt",
                ] {
                    if let Some(v) = o.get(key) {
                        let (c, i) = parts(v);
                        total.0 += c;
                        total.1 += i;
                    }
                }
                if let Some(images) = o.get("images").and_then(Value::as_array) {
                    total.1 += images.len();
                }
                total
            }
            _ => (0, 0),
        }
    }
    let (text, images) = parts(input);
    text.div_ceil(4) + images * 85
}
/// SHA-256 counter expansion, stable across platforms and process runs. Unit length.
pub fn embedding(text: &str, dimensions: usize) -> Vec<f64> {
    let mut v = Vec::with_capacity(dimensions);
    for n in 0..dimensions {
        let hash = super::super::common::pkce_challenge(&format!("{text}:{n}"));
        // SHA-256 encoded bytes retain deterministic entropy without extra dependencies.
        let x = hash
            .as_bytes()
            .iter()
            .take(8)
            .fold(0u64, |a, b| a.wrapping_mul(257).wrapping_add(u64::from(*b)));
        v.push((x as f64 / u64::MAX as f64) * 2.0 - 1.0);
    }
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}
