// fix-sel (1.0.3): `api.fetch` mit Auswahl nimmt die Crawl-ID der Ansicht mit, aus der die Auswahl stammt. Hält die
// Sitzung einen anderen Crawl, lehnt der Kern ab, bevor eine Quelle abgefragt (kostenpflichtiger Abruf) oder etwas
// geschrieben wird. Ohne die ID gilt das bisherige Verhalten.
#![allow(clippy::disallowed_methods)]
#[path = "cmd_api_support.rs"]
mod support;
use support::*;

async fn api_values(h: &Harness) -> u64 {
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
// spec: INTEG-008
#[tokio::test]
async fn a_selection_from_another_crawl_fetches_and_writes_nothing() {
    let h = Harness::new().await;
    h.connect().await;
    let source = h.source(ApiKind::Psi);
    let selection = json!({"kind":"selection","selection":{"kind":"ids","ids":[1,2,3]}});
    let err = h
        .call(
            "api.fetch",
            json!({"kinds":["psi"],"scope":selection,"crawlId":"another-crawl"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(err.message_key, "err-selection-other-crawl");
    assert_eq!(source.calls.load(Ordering::SeqCst), 0);
    assert!(h
        .app
        .ops()
        .active_statuses()
        .iter()
        .all(|op| op.label_key != "op-api-fetch"));
    assert_eq!(api_values(&h).await, 0);

    // The crawl of the session: as before.
    let crawl = h.key.0.clone();
    let result = h
        .op(
            "api.fetch",
            json!({"kinds":["psi"],"scope":selection,"crawlId":crawl}),
        )
        .await
        .unwrap();
    assert_eq!(result["fetched"], 3);
    assert!(source.calls.load(Ordering::SeqCst) > 0);
    assert_eq!(api_values(&h).await, 3);

    // Without a crawl id (older callers): as before.
    let result = h
        .op("api.fetch", json!({"kinds":["psi"],"scope":selection}))
        .await
        .unwrap();
    assert_eq!(result["fetched"], 3);
    h.server.stop().await;
}
