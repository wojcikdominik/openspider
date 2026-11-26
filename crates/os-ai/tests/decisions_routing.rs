//! M4-D01: Routing je Frage (AI-035).
#[path = "decisions_support.rs"]
mod support;

use os_ai::catalog::{AvailableModel, Endpoint, ModelRef};
use os_ai::decisions::exec::{ActiveQuestion, RunControl, RunOptions};
use os_ai::decisions::routing::{route, RoutingPolicy, RoutingSetting};
use os_ai::AiStatus;
use support::*;

fn with(setting: RoutingSetting) -> ActiveQuestion {
    ActiveQuestion {
        def: question("page-type"),
        routing: setting,
    }
}

// spec: AI-035 [M4]
// Mit OpenAI- und Anthropic-Schlüssel und Routing „automatisch“ gehen alle Fragen an `/v1/decisions` (0 Requests an
// `/anthropic/v1/messages`); wird der OpenAI-Schlüssel entfernt, laufen neu angestoßene Fragen über Anthropic,
// vorhandene Ergebnisse bleiben bis zur Neuanalyse unverändert.
#[tokio::test]
async fn automatic_prefers_decisions_then_fallback() {
    let mut env = Env::new(&["openai", "anthropic"]).await;
    let pt = question("page-type");
    let qs = [with(RoutingSetting::Automatic)];
    let first = vec![text_item(1, "MOCK-ID:product-1 Seite")];
    let mut items = first.clone();
    items.push(text_item(2, "MOCK-ID:d01-fb-choice Seite"));
    let dir = TempDir::new("auto");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    env.run(
        file.conn(),
        run,
        &qs,
        &first,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    assert_eq!(env.mock.posts(DECISIONS).len(), 1);
    assert_eq!(env.mock.posts(MESSAGES).len(), 0);
    // OpenAI-Schlüssel entfernt: neu angestoßene Frage (URL 2) über Anthropic; URL 1 bleibt.
    env.providers = providers(&["anthropic"], Vec::new());
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    assert_eq!(env.mock.posts(DECISIONS).len(), 1);
    assert_eq!(env.mock.posts(MESSAGES).len(), 1);
    let get = |id| {
        os_store::ai::answers::get(file.conn(), id, &pt.id, "1.0.0", "")
            .unwrap()
            .unwrap()
    };
    assert_eq!(get(1).source.as_deref(), Some("decisions-api"));
    assert_eq!(get(2).source.as_deref(), Some("self-reported"));
    assert_eq!(get(2).model.as_deref(), Some("claude-haiku-5-5"));
}

// spec: AI-035 [M4]
// Routing „immer Decisions API“ ohne OpenAI-Schlüssel → `Error: Missing API key` (kein stiller Wechsel auf den
// Ersatzweg); „immer strukturierte Ausgabe mit Modell X“ nutzt X auch mit OpenAI-Schlüssel.
#[tokio::test]
async fn fixed_routes() {
    let env = Env::new(&["anthropic"]).await;
    let qs = [with(RoutingSetting::DecisionsApi)];
    let items = vec![text_item(1, "Seite")];
    let dir = TempDir::new("fixed");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    let pt = question("page-type");
    assert_eq!(
        count(&counts(file.conn(), &pt), "Error: Missing API key"),
        1
    );
    assert!(env.mock.log().is_empty());

    let env = Env::new(&["openai", "anthropic"]).await;
    let qs = [with(RoutingSetting::StructuredOutput {
        model: ModelRef::new("anthropic", "claude-haiku-5-5", Endpoint::Messages),
    })];
    let items = vec![text_item(1, "MOCK-ID:d01-fb-choice Seite")];
    let dir = TempDir::new("fixed2");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    assert_eq!(env.mock.posts(DECISIONS).len(), 0);
    assert_eq!(env.mock.posts(MESSAGES).len(), 1);
    assert_eq!(count(&counts(file.conn(), &pt), "Success"), 1);
}

// spec: AI-035 [M4]
// Unter Richtlinie „Nur lokal“ werden Decision-Fragen automatisch an Ollama geroutet oder, ohne lokales Modell,
// `Skipped (policy)`; eine verbotene Decisions API (Richtlinie) fällt im Automatikmodus auf die Ersatzliste.
#[tokio::test]
async fn local_only_and_policy() {
    let mut env = Env::new(&["openai", "anthropic"]).await;
    env.policy = RoutingPolicy {
        local_only: true,
        decisions_forbidden: false,
    };
    let q = question("page-type");
    assert_eq!(
        route(&q, &RoutingSetting::Automatic, &env.routing()).unwrap_err(),
        AiStatus::SkippedPolicy
    );
    env.providers = providers(
        &["openai", "ollama"],
        vec![AvailableModel {
            model: model(
                "ollama",
                "mock-llama",
                "chat-completions",
                caps(false, false, true),
            ),
            max_output_tokens: None,
        }],
    );
    let r = route(&q, &RoutingSetting::Automatic, &env.routing()).unwrap();
    assert_eq!(r.provider(), "ollama");
    assert_eq!(r.source(), "self-reported");
    assert_eq!(
        route(&q, &RoutingSetting::DecisionsApi, &env.routing()).unwrap_err(),
        AiStatus::SkippedPolicy
    );
    // Lauf über Ollama (lokal, Kosten 0).
    let qs = [with(RoutingSetting::Automatic)];
    let items = vec![text_item(1, "MOCK-ID:d01-fb-choice Seite")];
    let dir = TempDir::new("local");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 0);
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    assert_eq!(env.mock.posts(OLLAMA_CHAT).len(), 1);
    assert_eq!(env.mock.posts(DECISIONS).len(), 0);
    assert_eq!(count(&counts(file.conn(), &q), "Success"), 1);

    let mut env = Env::new(&["openai", "anthropic"]).await;
    env.policy = RoutingPolicy {
        local_only: false,
        decisions_forbidden: true,
    };
    assert_eq!(
        route(&q, &RoutingSetting::Automatic, &env.routing())
            .unwrap()
            .provider(),
        "anthropic"
    );
}

// spec: AI-035 [M4]
// Ein Wechsel des Wegs ändert den Cache-Schlüssel (AI-015): gleiche Eingabe über Decisions und Ersatzweg ergibt zwei
// Requests, ein zweiter Lauf auf demselben Weg 0 (`Success (cached)`).
#[tokio::test]
async fn path_change_changes_the_cache_key() {
    let env = Env::new(&["openai", "anthropic"]).await;
    let pt = question("page-type");
    let items = vec![text_item(1, "MOCK-ID:d01-both Seite")];
    let dir = TempDir::new("cache");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let all = RunOptions {
        selection: os_ai::decisions::exec::Selection::All,
        ..RunOptions::default()
    };
    for setting in [
        RoutingSetting::Automatic,
        RoutingSetting::StructuredOutput {
            model: ModelRef::new("anthropic", "claude-haiku-5-5", Endpoint::Messages),
        },
        RoutingSetting::Automatic,
    ] {
        let qs = [with(setting)];
        env.run(
            file.conn(),
            run,
            &qs,
            &items,
            &ledger,
            RunControl::new(),
            all.clone(),
        )
        .run()
        .await
        .unwrap();
    }
    assert_eq!(env.mock.posts(DECISIONS).len(), 1);
    assert_eq!(env.mock.posts(MESSAGES).len(), 1);
    let row = os_store::ai::answers::get(file.conn(), 1, &pt.id, "1.0.0", "")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "Success (cached)");
    assert_eq!(row.source.as_deref(), Some("decisions-api"));
}
