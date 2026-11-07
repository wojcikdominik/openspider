#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "cmd_catalog_fixture.rs"]
mod fixture;
use fixture::*;
use serde_json::json;

// spec: UI-012
// spec: UI-018
#[tokio::test]
async fn large_partial_ranges_offer_export_and_never_send_unbounded_keys() {
    let fx = Fx::new();
    fx.write("WITH RECURSIVE n(x) AS (SELECT 10 UNION ALL SELECT x+1 FROM n WHERE x<30010) INSERT INTO urls(id,url,address,origin,is_internal,in_tables,state,discovered_via,folder_depth,url_length) SELECT x,'https://site.test/'||x,'https://site.test/'||x,'https://site.test',1,1,'done','link',0,25 FROM n;");
    let h = fx.open(tab(), json!(["Address"])).await;
    for (from, to, count, all, export) in [
        (10, 20009, 20000, false, false),
        (10, 20010, 0, false, true),
        (25000, 10, 0, false, true),
    ] {
        let r = fx
            .call(
                "view.range",
                json!({"viewId":h["viewId"],"from":from,"to":to}),
            )
            .await;
        assert_eq!(r["keys"].as_array().unwrap().len(), count);
        assert_eq!(r["all"], all);
        assert_eq!(r["exportRequired"], export);
    }
    let total = h["total"].as_u64().unwrap();
    let first = fx.rows(&h, 0, 1, json!(["Address"])).await["rowKeys"][0].clone();
    let last = fx.rows(&h, total - 1, 1, json!(["Address"])).await["rowKeys"][0].clone();
    let r = fx
        .call(
            "view.range",
            json!({"viewId":h["viewId"],"from":first,"to":last}),
        )
        .await;
    assert_eq!(r["all"], true);
    assert_eq!(r["exportRequired"], false);
    assert_eq!(r["keys"], json!([]));
}
