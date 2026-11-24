//! Abnahme fix-sel (unabhängiger Testagent, Festlegung des Orchestrators vom 24.11.2026): `export.detail` trägt die
//! Crawl-ID der View, aus der die Auswahl stammt. Weicht sie vom Crawl der Sitzung ab, lehnt der Kern mit
//! `err-selection-other-crawl` ab und schreibt keine Datei. Gleiche ID (wie die View sie nennt) oder keine ID: bisheriges
//! Verhalten. Annahme bis zum Endstand: die ID steht als `crawlId` auf oberster Ebene der Argumente.
#[path = "cmd_catalog_fixture.rs"]
mod fixture;
use fixture::*;
use os_app::dto::{CmdError, OpHandle, OpState};
use serde_json::{json, Value};

const KEY: &str = "err-selection-other-crawl";

fn detail(path: &std::path::Path, crawl: Option<&str>) -> Value {
    let mut a = json!({"spec":{"kind":"detail","detail":"headers","urlIds":[1],"params":{},"sort":[]},
        "selection":{"kind":"ids","ids":[1]},"path":path,"format":"csv"});
    if let Some(crawl) = crawl {
        a["crawlId"] = json!(crawl);
    }
    a
}

async fn refused(fx: &Fx, args: Value) -> CmdError {
    match fx
        .app
        .dispatch(os_app::CallCtx::for_window("main"), "export.detail", args)
        .await
    {
        Err(e) => e,
        Ok(v) => {
            let op: OpHandle = serde_json::from_value(v).unwrap();
            let status = fx.app.ops().wait(&op.op_id).await.unwrap();
            assert_eq!(status.state, OpState::Failed, "must be refused: {status:?}");
            status.error.unwrap()
        }
    }
}

// spec: UI-018
// spec: OUT-021
#[tokio::test]
async fn a_selection_from_another_crawl_writes_no_file() {
    let fx = Fx::new();
    for (i, other) in ["01JC0THERCRAWL0000000000AA", "not-a-crawl"]
        .iter()
        .enumerate()
    {
        let path = fx.dir.path().join(format!("other-{i}.csv"));
        let e = refused(&fx, detail(&path, Some(other))).await;
        assert_eq!(e.message_key, KEY, "{other}: {e:?}");
        assert!(!path.exists(), "no export file for {other}");
    }
    let leftovers: Vec<_> = std::fs::read_dir(fx.dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("other-"))
        .collect();
    assert!(leftovers.is_empty(), "no partial files: {leftovers:?}");
}

// spec: UI-018
// spec: OUT-021
#[tokio::test]
async fn the_crawl_id_the_view_names_or_none_exports_as_before() {
    let fx = Fx::new();
    // A view names its crawl; that name is what the table sends along.
    let h = fx.open(tab(), json!(["Address"])).await;
    let named = h["crawlId"]
        .as_str()
        .expect("view handle names its crawl")
        .to_string();
    for (name, crawl) in [("same", Some(named.as_str())), ("none", None)] {
        let path = fx.dir.path().join(format!("{name}.csv"));
        fx.op("export.detail", detail(&path, crawl)).await.unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("Address,Direction,Header Name,Header Value"),
            "{name}: {text}"
        );
    }
}
