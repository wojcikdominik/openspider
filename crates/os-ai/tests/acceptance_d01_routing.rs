//! Unabhängige Abnahme M4-D01 – Routing je Frage (AI-035): automatisch gegen fest, nicht verfügbarer Standardweg,
//! „Nur lokal“, Cache-Schlüssel je Weg, Nachvollziehbarkeit im Protokoll. Erwartungen nur aus `docs/spec/08-ai.md`.
// acceptance: independent
mod acceptance_d01_support;

use acceptance_d01_support as s;
use os_ai::cache::ExecutionPath;
use os_ai::catalog::Endpoint;
use os_ai::decisions::exec::{RunOptions, Selection};
use os_ai::decisions::format::QuestionDef;
use os_ai::decisions::routing::{route, RoutingContext, RoutingPolicy, RoutingSetting};
use os_ai::AiStatus;

const VALUES: [&str; 4] = ["product", "category", "other", "article"];

fn eq_question() -> QuestionDef {
    s::choice("d01t-eq-choice", "Eq Choice", &VALUES, &["title"])
}

fn ctx(bench: &s::Bench) -> RoutingContext<'_> {
    RoutingContext {
        defaults: &bench.defaults,
        settings: &bench.settings,
        providers: &bench.providers,
        policy: bench.policy,
    }
}

fn all() -> RunOptions {
    RunOptions {
        selection: Selection::All,
        ..RunOptions::default()
    }
}

// spec: AI-035 [M4]
// acceptance: independent
#[tokio::test]
async fn ai035_route_decisions_when_openai_key_present() {
    let mut bench = s::Bench::new(&["openai", "anthropic"], 1).await;
    let q = eq_question();
    let r = route(&q, &RoutingSetting::Automatic, &ctx(&bench)).expect("Weg");
    assert_eq!(r.path(), ExecutionPath::DecisionsApi);
    assert_eq!(r.source(), "decisions-api");
    assert_eq!(r.provider(), "openai");
    assert_eq!(r.model_id(), "gpt-6-luna");
    // Ohne OpenAI-Schlüssel: erstes verfügbares Modell der Ersatzliste.
    bench.set_keys(&["anthropic"]);
    let r = route(&q, &RoutingSetting::Automatic, &ctx(&bench)).expect("Weg");
    assert_eq!(r.path(), ExecutionPath::StructuredOutput);
    assert_eq!(r.source(), "self-reported");
    assert_eq!(r.provider(), "anthropic");
    assert_eq!(r.model_id(), "mock-claude");
    // „immer Decisions API“ ohne Schlüssel → Fehler, kein stiller Wechsel.
    assert_eq!(
        route(&q, &RoutingSetting::DecisionsApi, &ctx(&bench)).map(|r| r.source()),
        Err(AiStatus::MissingApiKey)
    );
    // Richtlinie verbietet die Decisions API → Ersatzweg, auch mit OpenAI-Schlüssel.
    bench.set_keys(&["openai", "anthropic"]);
    bench.policy = RoutingPolicy {
        decisions_forbidden: true,
        ..RoutingPolicy::default()
    };
    let r = route(&q, &RoutingSetting::Automatic, &ctx(&bench)).expect("Weg");
    assert_eq!(r.path(), ExecutionPath::StructuredOutput);
    // „immer strukturierte Ausgabe mit Modell X“.
    bench.policy = RoutingPolicy::default();
    let fixed = s::structured("anthropic", "mock-claude-b", Endpoint::Messages);
    let r = route(&q, &fixed, &ctx(&bench)).expect("Weg");
    assert_eq!(r.provider(), "anthropic");
    assert_eq!(r.model_id(), "mock-claude-b");
}

// spec: AI-035 [M4]
// acceptance: independent
#[tokio::test]
async fn ai035_local_only_routes_to_ollama_or_skips() {
    let mut bench = s::Bench::new(&["openai", "anthropic", "ollama"], 1).await;
    bench.policy = RoutingPolicy {
        local_only: true,
        ..RoutingPolicy::default()
    };
    let q = eq_question();
    let r = route(&q, &RoutingSetting::Automatic, &ctx(&bench)).expect("Weg");
    assert_eq!(r.provider(), "ollama");
    assert_eq!(r.path(), ExecutionPath::StructuredOutput);
    bench.set_keys(&["openai", "anthropic"]);
    assert_eq!(
        route(&q, &RoutingSetting::Automatic, &ctx(&bench)).map(|r| r.source()),
        Err(AiStatus::SkippedPolicy)
    );
    // Im Lauf: ohne lokales Modell `Skipped (policy)` und kein Request.
    let items = [s::page(1, "Local MOCK-ID:d01t-eq", "Text.")];
    bench.run(&[s::active(q.clone())], &items, s::options()).await;
    assert_eq!(bench.status(1, "d01t-eq-choice"), "Skipped (policy)");
    assert_eq!(bench.mock.provider_posts(), 0);
    // Mit lokalem Modell: Request an Ollama, keiner an Cloud-Anbieter.
    bench.set_keys(&["openai", "anthropic", "ollama"]);
    bench.run(&[s::active(q)], &items, all()).await;
    assert_eq!(bench.mock.posts(s::OLLAMA_CHAT).len(), 1);
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 0);
    assert_eq!(bench.mock.posts(s::MESSAGES).len(), 0);
}

// spec: AI-035 [M4]
// acceptance: independent
#[tokio::test]
async fn ai035_automatic_with_both_keys_uses_decisions_only() {
    let bench = s::Bench::new(&["openai", "anthropic"], 1).await;
    let items = s::site(|i| format!("Page {i}."));
    let questions = [
        s::active(eq_question()),
        s::active(s::predicate("auto-pred", "Auto Pred", &["title"])),
    ];
    bench.run(&questions, &items, s::options()).await;
    assert_eq!(
        bench.mock.posts(s::MESSAGES).len(),
        0,
        "0 Requests an /anthropic/v1/messages für Decision-Fragen"
    );
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 22);
    for row in bench.sent_rows() {
        assert_eq!(row.record.provider, "openai");
        assert_eq!(row.record.endpoint, "decisions");
    }
}

// spec: AI-035 [M4]
// acceptance: independent
#[tokio::test]
async fn ai035_removed_openai_key_new_entries_via_anthropic_old_unchanged() {
    let mut bench = s::Bench::new(&["openai", "anthropic"], 1).await;
    let q = eq_question();
    let first: Vec<_> = (1..=2)
        .map(|i| s::page(i, &format!("Old {i} MOCK-ID:d01t-eq"), "Text."))
        .collect();
    bench
        .run(&[s::active(q.clone())], &first, s::options())
        .await;
    let before: Vec<_> = first
        .iter()
        .map(|i| bench.answer(i.url_id, "d01t-eq-choice"))
        .collect();
    bench.set_keys(&["anthropic"]);
    let mut items = first.clone();
    items.push(s::page(3, "New 3 MOCK-ID:d01t-eq", "Text."));
    bench.run(&[s::active(q)], &items, s::options()).await;
    let new = bench.answer(3, "d01t-eq-choice");
    assert_eq!(new.status, "Success");
    assert_eq!(new.source.as_deref(), Some("self-reported"));
    assert_eq!(new.model.as_deref(), Some("mock-claude"));
    assert_eq!(bench.mock.posts(s::MESSAGES).len(), 1, "nur der neue Eintrag");
    for old in before {
        let now = bench.answer(old.url_id, "d01t-eq-choice");
        assert_eq!(now, old, "vorhandenes Ergebnis verändert");
        assert_eq!(now.source.as_deref(), Some("decisions-api"));
    }
    // Die Protokollzeile zeigt den gewählten Weg.
    let anthropic_rows: Vec<_> = bench
        .sent_rows()
        .into_iter()
        .filter(|r| r.record.url_id == Some(3))
        .collect();
    assert_eq!(anthropic_rows.len(), 1);
    assert_eq!(anthropic_rows[0].record.provider, "anthropic");
    assert_eq!(anthropic_rows[0].record.model, "mock-claude");
    assert_eq!(anthropic_rows[0].record.endpoint, "messages");
}

// spec: AI-035 [M4]
// acceptance: independent
#[tokio::test]
async fn ai035_forced_decisions_without_key_no_silent_fallback() {
    let bench = s::Bench::new(&["anthropic", "openrouter"], 1).await;
    let q = s::active_with(eq_question(), RoutingSetting::DecisionsApi);
    let items = [s::page(1, "Forced MOCK-ID:d01t-eq", "Text.")];
    bench.run(&[q], &items, s::options()).await;
    assert_eq!(bench.status(1, "d01t-eq-choice"), "Error: Missing API key");
    assert_eq!(bench.mock.provider_posts(), 0);
}

// spec: AI-035 [M4]
// acceptance: independent
#[tokio::test]
async fn ai035_changing_the_path_changes_the_cache_key() {
    let mut bench = s::Bench::new(&["openai", "anthropic"], 1).await;
    let q = eq_question();
    let items = [s::page(1, "Cache MOCK-ID:d01t-eq", "Text.")];
    bench.run(&[s::active(q.clone())], &items, s::options()).await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 1);
    // Gleicher Weg, „Neu analysieren“ → Cache-Treffer, kein Request.
    bench.run(&[s::active(q.clone())], &items, all()).await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 1);
    // Anderer Weg → anderer Schlüssel → ein Request über Anthropic.
    bench.set_keys(&["anthropic"]);
    bench.run(&[s::active(q.clone())], &items, all()).await;
    assert_eq!(bench.mock.posts(s::MESSAGES).len(), 1);
    assert_eq!(
        bench.answer(1, "d01t-eq-choice").source.as_deref(),
        Some("self-reported")
    );
}
