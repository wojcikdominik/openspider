//! Ergebnis-Cache über einer echten Crawl-Datei (AI-015): Treffer, Fehler nie, Refusals mit Kennzeichen,
//! „Refusals erneut versuchen“, Leeren, Statistik, Übernahme beim Recrawl und Option „Cache verwenden“.
//!
//! Die Eingaben stammen aus der Fixture-Site `tests/fixtures/ai/site/` (|G| = 25, |S| = 22). Ein Zähler steht für
//! den Anbieter; die Lauflogik (Geltungsbereich, Fragen) gehört M4-D01 und wird hier nur so weit nachgebildet, wie der
//! Cache sie braucht.
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use os_ai::cache::{
    input_hash, CacheKey, CacheKeyInput, CacheOptions, CachedRun, ExecutionPath, Lookup,
    ResultCache,
};
use os_ai::provider::Input;
use os_ai::AiStatus;
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::{CrawlFile, CrawlInit, StorageProfile};
use serde_json::{json, Value};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("os-ai-k06-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn now() -> Timestamp {
    Timestamp::from_db("2026-11-24T10:00:00.000Z").unwrap()
}

fn crawl(dir: &Path, name: &str) -> CrawlFile {
    let init = CrawlInit {
        crawl_id: SeededIds::new(6).new_crawl_id(now()),
        mode: CrawlMode::Spider,
        start_url: Some("http://ai.test:8080/".into()),
        list_source: None,
        project: "K06".into(),
        name: name.into(),
        config: CrawlConfig::default(),
        created_at: now(),
    };
    CrawlFile::create(&dir.join(name), &init, StorageProfile::default()).unwrap()
}

fn repo_path(parts: &[&str]) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("..");
    for part in parts {
        path.push(part);
    }
    path
}

struct Page {
    url: String,
    text: String,
    in_scope: bool,
}

/// Alle 25 Seiten-URLs der Fixture-Site mit ihrem Dateitext; S = Status 200 und indexierbar.
fn site_pages() -> Vec<Page> {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_path(&[
            "tests",
            "fixtures",
            "ai",
            "site",
            "manifest.json",
        ]))
        .unwrap(),
    )
    .unwrap();
    manifest["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let mut parts = vec!["tests", "fixtures", "ai", "site"];
            let file = p["file"].as_str().unwrap_or("");
            parts.extend(file.split('/'));
            let text = std::fs::read_to_string(repo_path(&parts)).unwrap_or_default();
            Page {
                url: p["url"].as_str().unwrap().to_string(),
                text,
                in_scope: p["status"] == 200 && p["indexable"] == true,
            }
        })
        .collect()
}

#[derive(Clone)]
struct Question {
    version: &'static str,
    model: &'static str,
    params: Value,
}

impl Question {
    fn page_type() -> Self {
        Self {
            version: "1.0.0",
            model: "gpt-6-luna",
            params: json!({}),
        }
    }
}

fn key(question: &Question, text: &str) -> CacheKey {
    CacheKey::new(CacheKeyInput {
        task: "decision".into(),
        definition_id: "page-type".into(),
        definition_version: question.version.into(),
        params: question.params.clone(),
        path: Some(ExecutionPath::DecisionsApi),
        provider: "openai".into(),
        model: question.model.into(),
        endpoint: "https://openai.mock.test/openai/v1".into(),
        result_params: json!({}),
        input_hash: input_hash(&[Input::Text { text: text.into() }]),
    })
}

/// Ein Lauf der Frage über alle Seiten: Seiten außerhalb von S erhalten `Skipped (scope)` ohne Cache und ohne
/// Request; sonst Cache oder „Request“ (Zähler) mit dem Ergebnis aus `answer`.
async fn run(
    cache: &ResultCache<'_>,
    pages: &[Page],
    question: &Question,
    requests: &Cell<u32>,
    answer: impl Fn(&str) -> (AiStatus, Value),
) -> Vec<AiStatus> {
    let mut statuses = Vec::new();
    for page in pages {
        if !page.in_scope {
            statuses.push(AiStatus::SkippedScope);
            continue;
        }
        let k = key(question, &page.text);
        let run = cache
            .get_or_run(&k, now(), || async {
                requests.set(requests.get() + 1);
                let (status, value) = answer(&page.url);
                (status.clone(), value, status)
            })
            .await
            .unwrap();
        statuses.push(match run {
            CachedRun::Hit { status, .. } => status,
            CachedRun::Ran { outcome, .. } => outcome,
        });
    }
    statuses
}

fn ok(_: &str) -> (AiStatus, Value) {
    (AiStatus::Success, json!({"page_type": "product"}))
}

fn count(statuses: &[AiStatus], status: &AiStatus) -> usize {
    statuses.iter().filter(|s| *s == status).count()
}

// spec: AI-015
// Erster Lauf → 22 Requests (|S|); zweiter Lauf → 0 Requests, 22 × `Success (cached)`, 3 × `Skipped (scope)`.
// Auch nach „Neustart“ (neue Verbindung auf dieselbe Datei) bleibt es bei 0 Requests.
#[tokio::test]
async fn second_run_and_restart_send_no_requests() {
    let pages = site_pages();
    assert_eq!(pages.len(), 25);
    assert_eq!(pages.iter().filter(|p| p.in_scope).count(), 22);
    let dir = TempDir::new("second-run");
    let file = crawl(&dir.0, "site.ospider");
    let q = Question::page_type();
    let requests = Cell::new(0);
    {
        let cache = ResultCache::new(file.conn(), CacheOptions::default());
        let first = run(&cache, &pages, &q, &requests, ok).await;
        assert_eq!(requests.get(), 22);
        assert_eq!(count(&first, &AiStatus::Success), 22);
        let second = run(&cache, &pages, &q, &requests, ok).await;
        assert_eq!(requests.get(), 22, "zweiter Lauf ohne Request");
        assert_eq!(count(&second, &AiStatus::SuccessCached), 22);
        assert_eq!(count(&second, &AiStatus::SkippedScope), 3);
        let stats = cache.stats().unwrap();
        assert_eq!((stats.hits, stats.entries, stats.refusals), (22, 22, 0));
        assert!(stats.size_bytes > 22 * 64);
    }
    let path = file.path().to_path_buf();
    drop(file);
    let reopened = CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
    let cache = ResultCache::new(reopened.conn(), CacheOptions::default());
    let third = run(&cache, &pages, &q, &requests, ok).await;
    assert_eq!(requests.get(), 22, "Neustart ohne Request");
    assert_eq!(count(&third, &AiStatus::SuccessCached), 22);
}

// spec: AI-015
// Neue Version (geänderte `instructions`) → 22 Requests; Modellwechsel → 22, Rückwechsel → 0 (beide im Cache).
// Schwellwerte sind kein Schlüsselbestandteil: dieselben Schlüssel → 0 Requests.
#[tokio::test]
async fn version_and_model_switch_requests_and_switch_back_is_cached() {
    let pages = site_pages();
    let dir = TempDir::new("switch");
    let file = crawl(&dir.0, "site.ospider");
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let requests = Cell::new(0);
    let q = Question::page_type();
    run(&cache, &pages, &q, &requests, ok).await;
    assert_eq!(requests.get(), 22);
    // Nur Schwellwerte geändert: gleiche Frage-Version, gleiche Schlüssel.
    run(&cache, &pages, &q, &requests, ok).await;
    assert_eq!(requests.get(), 22);
    let new_version = Question {
        version: "1.1.0",
        ..q.clone()
    };
    run(&cache, &pages, &new_version, &requests, ok).await;
    assert_eq!(requests.get(), 44);
    let other_model = Question {
        model: "gpt-6-luna-mini",
        ..q.clone()
    };
    run(&cache, &pages, &other_model, &requests, ok).await;
    assert_eq!(requests.get(), 66);
    let back = run(&cache, &pages, &q, &requests, ok).await;
    assert_eq!(requests.get(), 66, "Rückwechsel ohne Request");
    assert_eq!(count(&back, &AiStatus::SuccessCached), 22);
}

// spec: AI-015
// Parametrisierte Systemfrage: identischer Text, Parameter `code` de bzw. en → 2 Einträge; erneut → 0 Requests.
#[tokio::test]
async fn parameterised_question_caches_per_parameter_value() {
    let dir = TempDir::new("params");
    let file = crawl(&dir.0, "site.ospider");
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let pages = [
        Page {
            url: "/de".into(),
            text: "Gleicher Text".into(),
            in_scope: true,
        },
        Page {
            url: "/en".into(),
            text: "Gleicher Text".into(),
            in_scope: true,
        },
    ];
    let requests = Cell::new(0);
    for _ in 0..2 {
        for (page, code) in pages.iter().zip(["de", "en"]) {
            let q = Question {
                params: json!({"code": code}),
                ..Question::page_type()
            };
            run(&cache, std::slice::from_ref(page), &q, &requests, ok).await;
        }
    }
    assert_eq!(requests.get(), 2);
    assert_eq!(cache.stats().unwrap().entries, 2);
}

// spec: AI-015
// Ein Eintrag mit Fehlerstatus wird beim nächsten Lauf erneut angefragt; ein Refusal nur nach „Refusals erneut
// versuchen“; erfolgreiche Ergebnisse bleiben dabei im Cache.
#[tokio::test]
async fn errors_are_never_cached_and_refusals_only_retried_on_request() {
    let pages = site_pages();
    let dir = TempDir::new("errors");
    let file = crawl(&dir.0, "site.ospider");
    let q = Question::page_type();
    let requests = Cell::new(0);
    let partial = |url: &str| match url {
        "/products/1" | "/products/2" => (
            AiStatus::HttpError {
                code: 500,
                message: "Temporary error".into(),
            },
            Value::Null,
        ),
        "/products/3" => (AiStatus::ConnectionFailed, Value::Null),
        "/guides/1" => (
            AiStatus::Refusal,
            json!({"refusal_reason": "Fixture refusal"}),
        ),
        _ => ok(url),
    };
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let first = run(&cache, &pages, &q, &requests, partial).await;
    assert_eq!(requests.get(), 22);
    assert_eq!(count(&first, &AiStatus::Success), 18);
    assert_eq!(count(&first, &AiStatus::Refusal), 1);
    let stats = cache.stats().unwrap();
    assert_eq!(
        (stats.entries, stats.refusals),
        (19, 1),
        "Fehler nie im Cache"
    );
    // Nächster Lauf: nur die 3 Fehler erneut, der Refusal kommt aus dem Cache.
    let second = run(&cache, &pages, &q, &requests, ok).await;
    assert_eq!(requests.get(), 25);
    assert_eq!(count(&second, &AiStatus::Refusal), 1);
    assert_eq!(count(&second, &AiStatus::SuccessCached), 18);
    assert_eq!(count(&second, &AiStatus::Success), 3);
    // „Refusals erneut versuchen“: nur der Refusal wird angefragt.
    let retry = ResultCache::new(
        file.conn(),
        CacheOptions {
            retry_refusals: true,
            ..CacheOptions::default()
        },
    );
    let third = run(&retry, &pages, &q, &requests, ok).await;
    assert_eq!(requests.get(), 26);
    assert_eq!(count(&third, &AiStatus::SuccessCached), 21);
    assert_eq!(count(&third, &AiStatus::Success), 1);
    assert_eq!(
        retry.stats().unwrap().refusals,
        0,
        "Refusal durch Erfolg ersetzt"
    );
}

// spec: AI-015
// Nur `Success` und `Refusal` werden gespeichert; alle Fehler- und sonstigen Status nie.
#[test]
fn only_success_and_refusal_are_stored() {
    let dir = TempDir::new("store-filter");
    let file = crawl(&dir.0, "site.ospider");
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let k = key(&Question::page_type(), "x");
    let never = [
        AiStatus::SuccessCached,
        AiStatus::Pending,
        AiStatus::PendingBatch,
        AiStatus::SkippedScope,
        AiStatus::SkippedPolicy,
        AiStatus::BudgetExceeded,
        AiStatus::QuotaExceeded,
        AiStatus::MissingApiKey,
        AiStatus::Timeout { seconds: 30 },
        AiStatus::InvalidOutput,
        AiStatus::MissingAnswer,
        AiStatus::BatchExpired,
        AiStatus::ConnectionFailed,
        AiStatus::HttpError {
            code: 500,
            message: "x".into(),
        },
    ];
    for status in &never {
        assert!(
            !cache.store(&k, status, &Value::Null, now()).unwrap(),
            "{status}"
        );
    }
    assert_eq!(cache.lookup(&k).unwrap(), Lookup::Miss);
    assert!(cache
        .store(&k, &AiStatus::Refusal, &Value::Null, now())
        .unwrap());
    assert_eq!(
        cache.lookup(&k).unwrap(),
        Lookup::Hit {
            status: AiStatus::Refusal,
            result: Value::Null
        }
    );
    assert!(cache
        .store(&k, &AiStatus::Success, &json!({"a": 1}), now())
        .unwrap());
    assert_eq!(
        cache.lookup(&k).unwrap(),
        Lookup::Hit {
            status: AiStatus::SuccessCached,
            result: json!({"a": 1})
        }
    );
}

// spec: AI-015
// Option „Cache verwenden“ aus: jeder Lauf fragt an, nichts wird gelesen oder geschrieben.
#[tokio::test]
async fn cache_disabled_neither_reads_nor_writes() {
    let pages = site_pages();
    let dir = TempDir::new("disabled");
    let file = crawl(&dir.0, "site.ospider");
    let q = Question::page_type();
    let requests = Cell::new(0);
    let on = ResultCache::new(file.conn(), CacheOptions::default());
    run(&on, &pages, &q, &requests, ok).await;
    let off = ResultCache::new(
        file.conn(),
        CacheOptions {
            use_cache: false,
            ..CacheOptions::default()
        },
    );
    let statuses = run(&off, &pages, &q, &requests, ok).await;
    assert_eq!(requests.get(), 44);
    assert_eq!(count(&statuses, &AiStatus::Success), 22);
    assert_eq!(off.stats().unwrap().hits, 0);
    assert!(!CacheOptions::default().retry_refusals && CacheOptions::default().use_cache);
}

// spec: AI-015
// „Cache für Aufgabe/Frage leeren“ betrifft nur diese Frage (optional nur eine Version).
#[tokio::test]
async fn clearing_one_question_keeps_the_others() {
    let pages = site_pages();
    let dir = TempDir::new("clear");
    let file = crawl(&dir.0, "site.ospider");
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let requests = Cell::new(0);
    let q = Question::page_type();
    run(&cache, &pages, &q, &requests, ok).await;
    run(
        &cache,
        &pages,
        &Question {
            version: "2.0.0",
            ..q.clone()
        },
        &requests,
        ok,
    )
    .await;
    let other = CacheKey::new(CacheKeyInput {
        definition_id: "search-intent".into(),
        ..key(&q, "x").input().clone()
    });
    cache
        .store(&other, &AiStatus::Success, &json!(1), now())
        .unwrap();
    assert_eq!(cache.stats().unwrap().entries, 45);
    assert_eq!(
        cache.clear_definition("page-type", Some("2.0.0")).unwrap(),
        22
    );
    assert_eq!(cache.clear_definition("page-type", None).unwrap(), 22);
    assert_eq!(cache.stats().unwrap().entries, 1);
    assert!(matches!(cache.lookup(&other).unwrap(), Lookup::Hit { .. }));
}

// spec: AI-015
// Recrawl, bei dem sich der Text von 3 der 22 Seiten in S geändert hat, mit übernommenem Cache → genau 3 Requests.
// Die übernommene Datei bleibt byte-identisch.
#[tokio::test]
async fn recrawl_with_imported_cache_requests_only_changed_pages() {
    let pages = site_pages();
    let dir = TempDir::new("recrawl");
    let q = Question::page_type();
    let requests = Cell::new(0);
    let old_path = {
        let old = crawl(&dir.0, "old.ospider");
        let cache = ResultCache::new(old.conn(), CacheOptions::default());
        run(&cache, &pages, &q, &requests, ok).await;
        old.path().to_path_buf()
    };
    assert_eq!(requests.get(), 22);
    let before = std::fs::read(&old_path).unwrap();
    // site-changed: drei Seiten in S mit geändertem Text.
    let changed = ["/products/2", "/guides/1", "/contact"];
    let recrawled: Vec<Page> = pages
        .iter()
        .map(|p| Page {
            url: p.url.clone(),
            text: if changed.contains(&p.url.as_str()) {
                format!("{} (geändert)", p.text)
            } else {
                p.text.clone()
            },
            in_scope: p.in_scope,
        })
        .collect();
    assert_eq!(
        recrawled
            .iter()
            .filter(|p| p.in_scope && p.text.ends_with("(geändert)"))
            .count(),
        3
    );
    let new = crawl(&dir.0, "new.ospider");
    let cache = ResultCache::new(new.conn(), CacheOptions::default());
    let report = cache.import_from(&old_path).unwrap();
    assert_eq!(
        (report.read, report.imported, report.kept_existing),
        (22, 22, 0)
    );
    let statuses = run(&cache, &recrawled, &q, &requests, ok).await;
    assert_eq!(requests.get(), 25, "genau 3 Requests");
    assert_eq!(count(&statuses, &AiStatus::SuccessCached), 19);
    assert_eq!(
        std::fs::read(&old_path).unwrap(),
        before,
        "Quelle byte-identisch"
    );
}
