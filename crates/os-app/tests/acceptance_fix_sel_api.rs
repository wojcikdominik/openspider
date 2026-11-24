//! Abnahme fix-sel (unabhängiger Testagent, Festlegung des Orchestrators vom 24.11.2026): `api.fetch` mit Auswahl trägt
//! die Crawl-ID der View. Weicht sie vom Crawl der Sitzung ab, lehnt der Kern mit `err-selection-other-crawl` ab: kein
//! API-Abruf (Quelle und Mock-Server bleiben unberührt, also keine Kosten), keine gespeicherten Werte. Gleiche ID oder
//! keine ID: bisheriges Verhalten.
//! Annahme bis zum Endstand: die ID steht wie bei `SelectionArgs` als `crawlId` auf oberster Ebene der Argumente.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
#[path = "cmd_api_support.rs"]
mod support;
use support::*;

const KEY: &str = "err-selection-other-crawl";

fn fetch(crawl: Option<&str>) -> Value {
    let mut a = json!({"kinds":["psi"],"scope":{"kind":"selection","selection":{"kind":"ids","ids":[1,2,3]}}});
    if let Some(crawl) = crawl {
        a["crawlId"] = json!(crawl);
    }
    a
}

async fn stored(h: &Harness) -> u64 {
    h.app
        .read_pool()
        .run(h.key.clone(), |file, _| {
            Ok(file
                .conn()
                .query_row("SELECT COUNT(*) FROM api_values", [], |r| {
                    r.get::<_, u64>(0)
                })?)
        })
        .await
        .unwrap()
}

// spec: UI-018
// spec: INTEG-009
#[tokio::test]
async fn a_selection_from_another_crawl_fetches_nothing() {
    let h = Harness::new().await;
    h.connect().await;
    let source = h.source(ApiKind::Psi);
    let mock_calls = h.mock.log.calls().len();
    for other in ["01JC0THERCRAWL0000000000AA", "not-a-crawl"] {
        let e = h.op("api.fetch", fetch(Some(other))).await.unwrap_err();
        assert_eq!(e.message_key, KEY, "{other}: {e:?}");
    }
    assert_eq!(source.calls.load(Ordering::SeqCst), 0, "no API call");
    assert_eq!(
        h.mock.log.calls().len(),
        mock_calls,
        "no request at the mock"
    );
    assert_eq!(stored(&h).await, 0, "no stored value");
    h.server.stop().await;
}

// spec: UI-018
// spec: INTEG-009
#[tokio::test]
async fn the_same_crawl_id_or_none_fetches_as_before() {
    let h = Harness::new().await;
    h.connect().await;
    let source = h.source(ApiKind::Psi);
    let same = h.key.0.clone();
    let r = h.op("api.fetch", fetch(Some(&same))).await.unwrap();
    assert_eq!(r["fetched"], 3, "{r}");
    assert_eq!(source.calls.load(Ordering::SeqCst), 3);
    let r = h.op("api.fetch", fetch(None)).await.unwrap();
    assert_eq!(r["fetched"], 3, "{r}");
    assert_eq!(source.calls.load(Ordering::SeqCst), 6);
    assert_eq!(stored(&h).await, 3);
    h.server.stop().await;
}
