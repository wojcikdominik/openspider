//! Unabhängige Abnahmetests M4-K06: Ergebnis-Cache (AI-015) über die öffentliche Schnittstelle `os_ai::cache`.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-015, AI-020, „Datenhaltung“). Erwartete Schlüssel bildet
//! dieser Test selbst (eigene kanonische JSON-Bildung, eigenes SHA-256-Hex) bzw. liest sie aus der unabhängig mit
//! Python berechneten Fixture `tests/fixtures/ai/k06-acceptance/golden-cache-keys.json`.
//!
//! Ein „Lauf“ ist hier eine Schleife über Einträge, die je Eintrag `ResultCache::get_or_run` aufruft; der Zähler
//! steht für die Requests an den Anbieter (Mock-Zähler).
// acceptance: independent

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use os_ai::cache::{
    canonical_json, input_hash, CacheKey, CacheKeyInput, CacheOptions, CachedRun, ExecutionPath,
    Lookup, ResultCache, KEY_FORMAT_VERSION,
};
use os_ai::provider::Input;
use os_ai::AiStatus;
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------------------------------------------
// Hilfen

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("os-ai-k06-acc-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn now() -> Timestamp {
    Timestamp::from_db("2026-11-24T12:00:00.000Z").unwrap()
}

fn create_crawl(path: &Path) -> CrawlFile {
    let init = CrawlInit {
        crawl_id: SeededIds::new(42).new_crawl_id(now()),
        mode: CrawlMode::Spider,
        start_url: Some("http://ai.test:8080/".into()),
        list_source: None,
        project: "K06-Abnahme".into(),
        name: "acceptance".into(),
        config: CrawlConfig::default(),
        created_at: now(),
    };
    CrawlFile::create(path, &init, StorageProfile::default()).unwrap()
}

fn repo_file(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

/// Eigenes SHA-256-Hex (Kleinbuchstaben).
fn my_sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Eigene kanonische JSON-Bildung: Objektschlüssel nach Unicode-Codepunkten sortiert, keine Leerzeichen,
/// Nicht-ASCII-Zeichen unverändert in UTF-8.
fn my_canonical(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => my_string(s),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(my_canonical).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.chars().cmp(b.chars()));
            let parts: Vec<String> = keys
                .iter()
                .map(|k| format!("{}:{}", my_string(k), my_canonical(&map[*k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

fn my_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Erwarteter Schlüssel nach AI-015, selbst gebildet: SHA-256 über das kanonische JSON aller Bestandteile.
/// Die Feldnamen sind eine Implementierungsfestlegung (Spec-Lücke, siehe Bericht): öffentliche Felder von
/// `CacheKeyInput` plus `v` = `KEY_FORMAT_VERSION`.
fn my_key(input: &CacheKeyInput) -> String {
    let mut obj = Map::new();
    obj.insert("v".into(), json!(KEY_FORMAT_VERSION));
    obj.insert("task".into(), json!(input.task));
    obj.insert("definition_id".into(), json!(input.definition_id));
    obj.insert("definition_version".into(), json!(input.definition_version));
    obj.insert("params".into(), input.params.clone());
    obj.insert(
        "path".into(),
        match input.path {
            Some(ExecutionPath::DecisionsApi) => json!("decisions-api"),
            Some(ExecutionPath::StructuredOutput) => json!("structured-output"),
            None => Value::Null,
        },
    );
    obj.insert("provider".into(), json!(input.provider));
    obj.insert("model".into(), json!(input.model));
    obj.insert("endpoint".into(), json!(input.endpoint));
    obj.insert("result_params".into(), input.result_params.clone());
    obj.insert("input_hash".into(), json!(input.input_hash));
    my_sha256_hex(my_canonical(&Value::Object(obj)).as_bytes())
}

fn text_hash(text: &str) -> String {
    input_hash(&[Input::Text {
        text: text.to_string(),
    }])
}

fn base_input(text: &str) -> CacheKeyInput {
    CacheKeyInput {
        task: "decision".into(),
        definition_id: "page-type".into(),
        definition_version: "1.0.0".into(),
        params: json!({}),
        path: Some(ExecutionPath::DecisionsApi),
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
        endpoint: "https://openai.mock.test/openai/v1".into(),
        result_params: json!({}),
        input_hash: text_hash(text),
    }
}

/// 22 Einträge (|S| der Fixture-Site laut AI-015) mit unterschiedlichem Seitentext.
fn pages() -> Vec<String> {
    (1..=22)
        .map(|i| format!("MOCK-ID:page-{i:02} Seitentext der Testseite {i}"))
        .collect()
}

/// Ein Lauf über `inputs`; `answer` liefert je Eintrag den Endstatus des (simulierten) Requests.
/// Ergebnis: (Requests, Statuswerte je Eintrag).
async fn run(
    cache: &ResultCache<'_>,
    inputs: &[CacheKeyInput],
    answer: impl Fn(usize) -> AiStatus,
) -> (u32, Vec<AiStatus>) {
    let requests = Cell::new(0u32);
    let mut statuses = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        let key = CacheKey::new(input.clone());
        let status = answer(i);
        let out = cache
            .get_or_run(&key, now(), || {
                requests.set(requests.get() + 1);
                let status = status.clone();
                async move {
                    let result = match &status {
                        AiStatus::Success => json!({"page_type": "product", "n": i}),
                        AiStatus::Refusal => json!("model refused"),
                        _ => Value::Null,
                    };
                    (status.clone(), result, status)
                }
            })
            .await
            .unwrap();
        statuses.push(match out {
            CachedRun::Hit { status, .. } => status,
            CachedRun::Ran { outcome, .. } => outcome,
        });
    }
    (requests.get(), statuses)
}

fn all_success(_: usize) -> AiStatus {
    AiStatus::Success
}

fn inputs_for(texts: &[String], f: impl Fn(&mut CacheKeyInput)) -> Vec<CacheKeyInput> {
    texts
        .iter()
        .map(|t| {
            let mut i = base_input(t);
            f(&mut i);
            i
        })
        .collect()
}

fn hash_file(path: &Path) -> String {
    my_sha256_hex(&std::fs::read(path).unwrap())
}

fn companions(path: &Path) -> Vec<(PathBuf, String)> {
    ["-wal", "-shm", "-journal"]
        .iter()
        .filter_map(|suffix| {
            let mut name = path.as_os_str().to_owned();
            name.push(suffix);
            let p = PathBuf::from(name);
            p.exists().then(|| {
                let h = hash_file(&p);
                (p, h)
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Schlüssel

// spec: AI-015
// acceptance: independent
#[test]
fn key_is_sha256_over_canonical_json_of_all_components() {
    let mut inputs = vec![base_input("a")];
    let mut i = base_input("b");
    i.params = json!({"code": "de", "liste": [3, 1, 2], "ä": {"z": 1, "a": null}});
    i.result_params = json!({"temperature": 0.2, "dimensions": 256});
    i.path = None;
    inputs.push(i);
    for input in inputs {
        let key = CacheKey::new(input.clone());
        assert_eq!(key.as_str().len(), 64, "SHA-256 als 64 Hex-Zeichen");
        assert!(key
            .as_str()
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
        // Der Schlüssel ist SHA-256 über die kanonische Darstellung, die er selbst nennt …
        assert_eq!(key.as_str(), my_sha256_hex(key.canonical().as_bytes()));
        // … und diese Darstellung ist kanonisch (sortiert, ohne Leerraum) nach eigener Bildung.
        let parsed: Value = serde_json::from_str(&key.canonical()).unwrap();
        assert_eq!(key.canonical(), my_canonical(&parsed));
        // Eigene Bildung über alle Bestandteile ergibt denselben Schlüssel.
        assert_eq!(key.as_str(), my_key(&input), "eigene Schlüsselbildung");
    }
}

// spec: AI-015
// acceptance: independent
#[test]
fn every_component_changes_the_key() {
    let base = base_input("Seitentext");
    let k0 = CacheKey::new(base.clone()).as_str().to_string();
    type Change = (&'static str, Box<dyn Fn(&mut CacheKeyInput)>);
    let changes: Vec<Change> = vec![
        ("Aufgabentyp", Box::new(|i| i.task = "suggest-title".into())),
        (
            "Frage-/Prompt-ID",
            Box::new(|i| i.definition_id = "search-intent".into()),
        ),
        (
            "Version",
            Box::new(|i| i.definition_version = "1.0.1".into()),
        ),
        (
            "Vorlagenparameter",
            Box::new(|i| i.params = json!({"code": "de"})),
        ),
        (
            "Ausführungsweg",
            Box::new(|i| i.path = Some(ExecutionPath::StructuredOutput)),
        ),
        ("Ausführungsweg leer", Box::new(|i| i.path = None)),
        ("Anbieter", Box::new(|i| i.provider = "openrouter".into())),
        ("Modell", Box::new(|i| i.model = "gpt-6-sol".into())),
        (
            "Endpunkt-Basis-URL (EU)",
            Box::new(|i| i.endpoint = "https://eu.openai.mock.test/openai/v1".into()),
        ),
        (
            "ergebnisrelevante Parameter",
            Box::new(|i| i.result_params = json!({"temperature": 0.7})),
        ),
        (
            "Eingabe-Hash",
            Box::new(|i| i.input_hash = text_hash("anderer Seitentext")),
        ),
    ];
    let mut seen = std::collections::BTreeSet::new();
    seen.insert(k0.clone());
    for (name, change) in changes {
        let mut input = base.clone();
        change(&mut input);
        let k = CacheKey::new(input).as_str().to_string();
        assert_ne!(k, k0, "Änderung von {name} muss den Schlüssel ändern");
        assert!(
            seen.insert(k),
            "Änderung von {name} ergibt einen schon gesehenen Schlüssel"
        );
    }
    // Gleiche Bestandteile → gleicher Schlüssel.
    assert_eq!(CacheKey::new(base).as_str(), k0);
}

// spec: AI-015
// acceptance: independent
#[test]
fn object_key_order_in_parameters_does_not_change_the_key() {
    let mut a = base_input("x");
    let mut b = base_input("x");
    a.params = serde_json::from_str(
        r#"{"code":"de","kandidaten":[{"id":"c1","t":"A"},{"t":"B","id":"c2"}]}"#,
    )
    .unwrap();
    b.params = serde_json::from_str(
        r#"{ "kandidaten": [ {"t":"A","id":"c1"}, {"id":"c2","t":"B"} ], "code": "de" }"#,
    )
    .unwrap();
    a.result_params = serde_json::from_str(r#"{"temperature":0,"dimensions":256}"#).unwrap();
    b.result_params = serde_json::from_str(r#"{"dimensions":256,"temperature":0}"#).unwrap();
    assert_eq!(CacheKey::new(a.clone()).as_str(), CacheKey::new(b).as_str());
    // Reihenfolge in Listen ist dagegen Inhalt (Kandidatenliste c1, c2 ≠ c2, c1).
    let mut c = a.clone();
    c.params = serde_json::from_str(
        r#"{"code":"de","kandidaten":[{"id":"c2","t":"B"},{"id":"c1","t":"A"}]}"#,
    )
    .unwrap();
    assert_ne!(CacheKey::new(a).as_str(), CacheKey::new(c).as_str());
    // Die öffentliche Kanonisierung sortiert ebenfalls.
    let v: Value = serde_json::from_str(r#"{"b":1,"a":{"d":[1,{"y":2,"x":1}],"c":"ä"}}"#).unwrap();
    assert_eq!(
        canonical_json(&v),
        r#"{"a":{"c":"ä","d":[1,{"x":1,"y":2}]},"b":1}"#
    );
}

// spec: AI-015
// acceptance: independent
#[test]
fn golden_ten_keys_are_platform_independent() {
    // Fixture: Eingaben und erwartete Schlüssel, unabhängig mit Python berechnet (literal, damit jede Plattform
    // denselben Wert prüft).
    let path = repo_file("tests/fixtures/ai/k06-acceptance/golden-cache-keys.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let keys = doc["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 10);
    for entry in keys {
        let name = entry["name"].as_str().unwrap();
        let input: CacheKeyInput = serde_json::from_value(entry["input"].clone()).unwrap();
        let expected = entry["key"].as_str().unwrap();
        // Die Fixture selbst ist konsistent mit der eigenen Bildung.
        assert_eq!(my_key(&input), expected, "{name}: eigene Bildung ≠ Fixture");
        assert_eq!(
            CacheKey::new(input).as_str(),
            expected,
            "{name}: Schlüssel weicht vom Golden-Wert ab"
        );
    }
}

// spec: AI-015
// acceptance: independent
#[test]
fn input_hash_covers_sent_text_and_image_bytes() {
    let t1 = text_hash("Seitentext A");
    assert_eq!(t1, text_hash("Seitentext A"));
    assert_ne!(t1, text_hash("Seitentext B"));
    // Bilder über ihren Byte-Hash: dieselben Bytes in anders umbrochener Base64-Form → gleicher Hash.
    let img = |b64: &str| {
        input_hash(&[
            Input::Text {
                text: "Alt-Text".into(),
            },
            Input::Image {
                media_type: "image/png".into(),
                base64: b64.into(),
            },
        ])
    };
    // „Zm9vYmFy“ = Bytes `foobar`; mit Zeilenumbruch umbrochen dieselben Bytes; „Zm9vYmF6“ = `foobaz`.
    let a = img("Zm9vYmFy");
    let b = img("Zm9v\nYmFy");
    assert_eq!(a, b, "gleiche Bildbytes → gleicher Eingabe-Hash");
    let c = img("Zm9vYmF6");
    assert_ne!(a, c, "andere Bildbytes → anderer Eingabe-Hash");
}

// ---------------------------------------------------------------------------------------------------------------
// Läufe

// spec: AI-015
// acceptance: independent
#[tokio::test]
async fn second_run_sends_no_requests_and_survives_reopen() {
    let dir = TempDir::new("rerun");
    let path = dir.path().join("crawl.ospider");
    let inputs = inputs_for(&pages(), |_| {});
    {
        let file = create_crawl(&path);
        let cache = ResultCache::new(file.conn(), CacheOptions::default());
        let (n, st) = run(&cache, &inputs, all_success).await;
        assert_eq!(n, 22, "erster Lauf: 22 Requests");
        assert!(st.iter().all(|s| *s == AiStatus::Success));
        let (n, st) = run(&cache, &inputs, all_success).await;
        assert_eq!(n, 0, "zweiter Lauf: 0 Requests");
        assert_eq!(st.len(), 22);
        for s in &st {
            assert_eq!(*s, AiStatus::SuccessCached);
            assert_eq!(s.to_string(), "Success (cached)");
        }
    }
    // Neustart: Crawl-Datei schließen und neu öffnen.
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let (n, st) = run(&cache, &inputs, all_success).await;
    assert_eq!(n, 0, "nach Neuöffnen: 0 Requests");
    assert!(st.iter().all(|s| *s == AiStatus::SuccessCached));
    // Gecachtes Ergebnis ist das gespeicherte.
    match cache.lookup(&CacheKey::new(inputs[3].clone())).unwrap() {
        Lookup::Hit { status, result } => {
            assert_eq!(status, AiStatus::SuccessCached);
            assert_eq!(result, json!({"page_type": "product", "n": 3}));
        }
        Lookup::Miss => panic!("Eintrag fehlt nach Neuöffnen"),
    }
}

// spec: AI-015
// acceptance: independent
#[tokio::test]
async fn new_version_and_model_switch_request_again_switch_back_is_cached() {
    let dir = TempDir::new("model");
    let file = create_crawl(&dir.path().join("c.ospider"));
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let first = inputs_for(&pages(), |_| {});
    assert_eq!(run(&cache, &first, all_success).await.0, 22);
    // Neue Version der Frage (geänderte instructions, AI-031) → 22 Requests.
    let v2 = inputs_for(&pages(), |i| i.definition_version = "1.1.0".into());
    assert_eq!(run(&cache, &v2, all_success).await.0, 22);
    // Modellwechsel → 22, Rückwechsel → 0; beide Ergebnisse bleiben.
    let other = inputs_for(&pages(), |i| i.model = "gpt-6-sol".into());
    assert_eq!(run(&cache, &other, all_success).await.0, 22);
    assert_eq!(run(&cache, &first, all_success).await.0, 0);
    assert_eq!(run(&cache, &other, all_success).await.0, 0);
    assert_eq!(cache.stats().unwrap().entries, 66);
}

// spec: AI-015
// acceptance: independent
#[tokio::test]
async fn parameterised_question_two_language_codes_two_entries() {
    let dir = TempDir::new("lang");
    let file = create_crawl(&dir.path().join("c.ospider"));
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    // Identischer Text, Parameter `code` unterscheidet sich.
    let text = "Identischer Seitentext";
    let mk = |code: &str| {
        let mut i = base_input(text);
        i.task = "decision".into();
        i.definition_id = "page-language-matches".into();
        i.params = json!({"code": code});
        i
    };
    let inputs = vec![mk("de"), mk("en")];
    assert_eq!(run(&cache, &inputs, all_success).await.0, 2);
    assert_eq!(cache.stats().unwrap().entries, 2, "2 Cache-Einträge");
    assert_eq!(
        run(&cache, &inputs, all_success).await.0,
        0,
        "erneuter Lauf: 0"
    );
}

// spec: AI-015
// spec: AI-020
// acceptance: independent
#[tokio::test]
async fn errors_are_never_cached() {
    let dir = TempDir::new("errors");
    let file = create_crawl(&dir.path().join("c.ospider"));
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let errors = [
        AiStatus::HttpError {
            code: 500,
            message: "Internal Server Error".into(),
        },
        AiStatus::ConnectionFailed,
        AiStatus::Timeout { seconds: 30 },
        AiStatus::InvalidOutput,
        AiStatus::MissingAnswer,
        AiStatus::QuotaExceeded,
        AiStatus::BudgetExceeded,
        AiStatus::MissingApiKey,
    ];
    let texts: Vec<String> = (0..errors.len())
        .map(|i| format!("Fehlerseite {i}"))
        .collect();
    let inputs = inputs_for(&texts, |_| {});
    let (n, st) = run(&cache, &inputs, |i| errors[i].clone()).await;
    assert_eq!(n as usize, errors.len());
    assert_eq!(st, errors.to_vec());
    assert_eq!(cache.stats().unwrap().entries, 0, "kein Fehler im Cache");
    // Nächster Lauf fragt alle erneut an; jetzt erfolgreich → gecacht.
    let (n, _) = run(&cache, &inputs, all_success).await;
    assert_eq!(n as usize, errors.len(), "Fehler werden erneut angefragt");
    let (n, _) = run(&cache, &inputs, all_success).await;
    assert_eq!(n, 0);
    // Auch direktes Speichern eines Fehlers legt nichts ab.
    let key = CacheKey::new(base_input("direkt"));
    assert!(!cache
        .store(&key, &AiStatus::ConnectionFailed, &Value::Null, now())
        .unwrap());
    assert_eq!(cache.lookup(&key).unwrap(), Lookup::Miss);
}

// spec: AI-015
// acceptance: independent
#[tokio::test]
async fn refusals_are_cached_with_flag_and_only_retried_on_request() {
    let dir = TempDir::new("refusal");
    let file = create_crawl(&dir.path().join("c.ospider"));
    let texts = pages();
    let inputs = inputs_for(&texts, |_| {});
    // Eintrag 0 wird verweigert, die übrigen gelingen.
    let answer = |i: usize| {
        if i == 0 {
            AiStatus::Refusal
        } else {
            AiStatus::Success
        }
    };
    {
        let cache = ResultCache::new(file.conn(), CacheOptions::default());
        let (n, st) = run(&cache, &inputs, answer).await;
        assert_eq!(n, 22);
        assert_eq!(st[0], AiStatus::Refusal);
        let stats = cache.stats().unwrap();
        assert_eq!(stats.entries, 22, "Refusal wird gecacht");
        assert_eq!(stats.refusals, 1, "mit Kennzeichen");
        // Normaler nächster Lauf: kein Request, Refusal bleibt Refusal (nicht `Success (cached)`).
        let (n, st) = run(&cache, &inputs, answer).await;
        assert_eq!(n, 0);
        assert_eq!(st[0], AiStatus::Refusal);
        assert!(st[1..].iter().all(|s| *s == AiStatus::SuccessCached));
    }
    // „Refusals erneut versuchen“: nur der Refusal wird erneut angefragt.
    let retry = ResultCache::new(
        file.conn(),
        CacheOptions {
            use_cache: true,
            retry_refusals: true,
        },
    );
    let (n, st) = run(&retry, &inputs, all_success).await;
    assert_eq!(n, 1, "nur der Refusal wird erneut angefragt");
    assert_eq!(st[0], AiStatus::Success);
    assert!(st[1..].iter().all(|s| *s == AiStatus::SuccessCached));
    let normal = ResultCache::new(file.conn(), CacheOptions::default());
    assert_eq!(normal.stats().unwrap().refusals, 0);
    assert_eq!(run(&normal, &inputs, all_success).await.0, 0);
}

// spec: AI-015
// acceptance: independent
#[tokio::test]
async fn option_use_cache_defaults_on_and_off_always_requests() {
    assert!(
        CacheOptions::default().use_cache,
        "„Cache verwenden“ Standard an"
    );
    assert!(!CacheOptions::default().retry_refusals);
    let dir = TempDir::new("off");
    let file = create_crawl(&dir.path().join("c.ospider"));
    let inputs = inputs_for(&pages(), |_| {});
    let on = ResultCache::new(file.conn(), CacheOptions::default());
    assert_eq!(run(&on, &inputs, all_success).await.0, 22);
    let off = ResultCache::new(
        file.conn(),
        CacheOptions {
            use_cache: false,
            retry_refusals: false,
        },
    );
    let (n, st) = run(&off, &inputs, all_success).await;
    assert_eq!(n, 22, "Cache aus: jeder Eintrag wird angefragt");
    assert!(st.iter().all(|s| *s == AiStatus::Success));
    let (n, _) = run(&off, &inputs, all_success).await;
    assert_eq!(n, 22);
}

// spec: AI-015
// acceptance: independent
#[tokio::test]
async fn clear_cache_for_one_question_keeps_others() {
    let dir = TempDir::new("clear");
    let file = create_crawl(&dir.path().join("c.ospider"));
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let page_type = inputs_for(&pages(), |_| {});
    let intent = inputs_for(&pages(), |i| i.definition_id = "search-intent".into());
    assert_eq!(run(&cache, &page_type, all_success).await.0, 22);
    assert_eq!(run(&cache, &intent, all_success).await.0, 22);
    let removed = cache.clear_definition("page-type", None).unwrap();
    assert_eq!(removed, 22);
    assert_eq!(cache.stats().unwrap().entries, 22);
    assert_eq!(
        run(&cache, &intent, all_success).await.0,
        0,
        "andere Frage bleibt"
    );
    assert_eq!(
        run(&cache, &page_type, all_success).await.0,
        22,
        "geleerte Frage fragt erneut"
    );
}

// spec: AI-015
// acceptance: independent
#[tokio::test]
async fn statistics_report_hits_entries_and_size() {
    let dir = TempDir::new("stats");
    let file = create_crawl(&dir.path().join("c.ospider"));
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let empty = cache.stats().unwrap();
    assert_eq!((empty.hits, empty.entries), (0, 0));
    let inputs = inputs_for(&pages(), |_| {});
    run(&cache, &inputs, all_success).await;
    let after_first = cache.stats().unwrap();
    assert_eq!(after_first.entries, 22);
    assert_eq!(after_first.hits, 0);
    assert!(after_first.size_bytes > 0, "Größe > 0");
    run(&cache, &inputs[..5], all_success).await;
    let after_hits = cache.stats().unwrap();
    assert_eq!(after_hits.hits, 5, "5 Treffer");
    assert_eq!(after_hits.entries, 22);
    assert_eq!(after_hits.size_bytes, after_first.size_bytes);
    let more = inputs_for(&["neu".to_string()], |_| {});
    run(&cache, &more, all_success).await;
    assert!(cache.stats().unwrap().size_bytes > after_first.size_bytes);
}

// spec: AI-015
// acceptance: independent
#[tokio::test]
async fn imported_cache_from_other_file_is_read_only_and_recrawl_requests_only_changed() {
    let dir = TempDir::new("import");
    let old_path = dir.path().join("vorlauf.ospider");
    let old_pages = pages();
    {
        let old = create_crawl(&old_path);
        let cache = ResultCache::new(old.conn(), CacheOptions::default());
        assert_eq!(
            run(&cache, &inputs_for(&old_pages, |_| {}), all_success)
                .await
                .0,
            22
        );
    }
    let before = hash_file(&old_path);
    let before_companions = companions(&old_path);
    // Recrawl: 3 von 22 Seiten mit geändertem Text.
    let mut new_pages = old_pages.clone();
    for i in [2usize, 9, 17] {
        new_pages[i] = format!("{} – geändert", new_pages[i]);
    }
    let new = create_crawl(&dir.path().join("recrawl.ospider"));
    let cache = ResultCache::new(new.conn(), CacheOptions::default());
    let report = cache.import_from(&old_path).unwrap();
    assert_eq!(report.read, 22);
    assert_eq!(report.imported, 22);
    let (n, st) = run(&cache, &inputs_for(&new_pages, |_| {}), all_success).await;
    assert_eq!(n, 3, "nur die 3 geänderten Seiten verursachen Requests");
    assert_eq!(
        st.iter().filter(|s| **s == AiStatus::SuccessCached).count(),
        19
    );
    drop(cache);
    drop(new);
    assert_eq!(hash_file(&old_path), before, "Quelle bleibt byte-identisch");
    assert_eq!(
        companions(&old_path),
        before_companions,
        "keine neuen oder geänderten Begleitdateien der Quelle"
    );
}
