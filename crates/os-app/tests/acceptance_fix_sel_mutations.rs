//! Abnahme fix-sel (unabhängiger Testagent, nur aus der Festlegung des Orchestrators vom 24.11.2026): Jede Aktion auf
//! einer Zeilenauswahl trägt die Crawl-ID der View, aus der die Auswahl stammt. Weicht sie vom Crawl der Sitzung ab,
//! lehnt der Kern mit `err-selection-other-crawl` ab und ändert nichts (kein Entfernen, kein erneuter Abruf). Gleiche
//! ID oder keine ID: bisheriges Verhalten. Fixture: zwei echte Crawls von `fx-ui-basic` über `os-testsite`.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use os_app::dto::{CmdError, OpHandle, OpState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const KEY: &str = "err-selection-other-crawl";

struct Two {
    app: os_app::App,
    site: os_testsite::TestSite,
    _dir: tempfile::TempDir,
    a: String,
    a_path: PathBuf,
    b: String,
    b_path: PathBuf,
}

/// Crawl A, then crawl B in the same window; the session holds B.
async fn two_crawls() -> Two {
    let site = os_testsite::TestSite::start_dir(fixture(), 0)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    cfg().save_default(&app.config().paths.settings).unwrap();
    let mut ids = Vec::new();
    for _ in 0..2 {
        call(
            &app,
            "crawl.start",
            serde_json::to_value(request(site.url("/"))).unwrap(),
        )
        .await;
        completed(&app).await;
        let s = app.sessions().current(&window()).unwrap();
        ids.push((s.key.0.clone(), s.path.clone()));
    }
    let (b, b_path) = ids.pop().unwrap();
    let (a, a_path) = ids.pop().unwrap();
    assert_ne!(a, b, "two distinct crawls");
    Two {
        app,
        site,
        _dir: dir,
        a,
        a_path,
        b,
        b_path,
    }
}

/// Every row of every table of the session's crawl file, as text (row-wise comparison).
async fn dump(app: &os_app::App) -> Vec<String> {
    app.read_pool()
        .run(key(app), |f, _| {
            let conn = f.conn();
            let mut out = Vec::new();
            let tables: Vec<String> = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            for table in tables {
                let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\""))?;
                let n = stmt.column_count();
                let mut rows: Vec<String> = stmt
                    .query_map([], |r| {
                        let mut cells = Vec::with_capacity(n);
                        for i in 0..n {
                            let v: rusqlite::types::Value = r.get(i)?;
                            cells.push(format!("{v:?}"));
                        }
                        Ok(cells.join("|"))
                    })?
                    .collect::<Result<_, _>>()?;
                rows.sort();
                out.extend(rows.into_iter().map(|row| format!("{table}: {row}")));
            }
            Ok(out)
        })
        .await
        .unwrap()
}

/// Rows of a crawl file that is not the session's (read directly, read-only).
fn urls_of(path: &Path) -> Vec<(i64, String)> {
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let mut stmt = conn
        .prepare("SELECT id, address FROM urls ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The command is refused: either at once or as a failed operation; the error names `err-selection-other-crawl`.
async fn refused(app: &os_app::App, name: &str, args: Value) -> CmdError {
    match app.dispatch(ctx(), name, args).await {
        Err(e) => e,
        Ok(v) => {
            let op: OpHandle = serde_json::from_value(v).unwrap();
            let status = app.ops().wait(&op.op_id).await.unwrap();
            assert_eq!(
                status.state,
                OpState::Failed,
                "{name} must be refused: {status:?}"
            );
            status.error.unwrap()
        }
    }
}

fn args(ids: Value, crawl: Option<&str>) -> Value {
    let mut a = json!({ "selection": ids });
    if let Some(crawl) = crawl {
        a["crawlId"] = json!(crawl);
    }
    a
}

// spec: UI-018
// spec: UI-114
// spec: CRAWL-131
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selection_from_crawl_a_is_refused_while_the_core_holds_crawl_b() {
    let t = two_crawls().await;
    let before = dump(&t.app).await;
    let a_before = urls_of(&t.a_path);
    let a_bytes = std::fs::read(&t.a_path).unwrap();
    let requests = t.site.requests().len();
    for name in ["crawl.recrawl", "crawl.remove"] {
        for sel in [
            json!({"kind":"ids","ids":[1,2,3]}),
            json!({"kind":"all","viewId":"v-a","version":1,"except":[]}),
        ] {
            let e = refused(&t.app, name, args(sel.clone(), Some(&t.a))).await;
            assert_eq!(e.message_key, KEY, "{name} {sel}: {e:?}");
        }
    }
    assert_eq!(dump(&t.app).await, before, "crawl B row-wise unchanged");
    assert_eq!(
        t.site.requests().len(),
        requests,
        "no fetch for a refused recrawl"
    );
    assert_eq!(urls_of(&t.a_path), a_before, "crawl A unchanged");
    assert_eq!(
        std::fs::read(&t.a_path).unwrap(),
        a_bytes,
        "crawl A byte-wise unchanged"
    );
    assert_eq!(key(&t.app).0, t.b, "the session still holds crawl B");
}

// spec: UI-018
// spec: CRAWL-131
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_crawl_id_is_refused_as_well() {
    let t = two_crawls().await;
    let before = dump(&t.app).await;
    for name in ["crawl.recrawl", "crawl.remove"] {
        let e = refused(
            &t.app,
            name,
            args(json!({"kind":"ids","ids":[1]}), Some("not-a-crawl")),
        )
        .await;
        assert_eq!(e.message_key, KEY, "{name}: {e:?}");
    }
    assert_eq!(dump(&t.app).await, before);
}

// spec: UI-018
// spec: UI-114
// spec: CRAWL-131
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_crawl_id_or_none_behaves_as_before() {
    let t = two_crawls().await;
    let a_before = urls_of(&t.a_path);
    for crawl in [Some(t.b.as_str()), None] {
        let requests = t.site.requests().len();
        let r = operation(
            &t.app,
            "crawl.recrawl",
            args(json!({"kind":"ids","ids":[1]}), crawl),
        )
        .await;
        assert_eq!(r.state, OpState::Done, "{crawl:?}: {:?}", r.error);
        assert_eq!(r.result.unwrap()["count"], 1);
        assert!(
            t.site.requests().len() > requests,
            "recrawl fetched again ({crawl:?})"
        );
    }
    let r = operation(
        &t.app,
        "crawl.remove",
        args(json!({"kind":"ids","ids":[1]}), Some(&t.b)),
    )
    .await;
    assert_eq!(r.state, OpState::Done, "{:?}", r.error);
    assert_eq!(r.result.unwrap()["count"], 1);
    let r = operation(
        &t.app,
        "crawl.remove",
        args(json!({"kind":"ids","ids":[2]}), None),
    )
    .await;
    assert_eq!(r.state, OpState::Done, "{:?}", r.error);
    assert_eq!(r.result.unwrap()["count"], 1);
    let ids: Vec<i64> = urls_of(&t.b_path).into_iter().map(|(id, _)| id).collect();
    assert!(
        !ids.contains(&1) && !ids.contains(&2),
        "removed in crawl B: {ids:?}"
    );
    assert_eq!(urls_of(&t.a_path), a_before, "crawl A untouched");
}

// spec: UI-018
// spec: CRAWL-131
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_crawl_switch_racing_with_remove_never_touches_another_crawl() {
    // The session switches from B to A while a remove for B is on its way. Whatever wins, the removal never lands in
    // crawl A (which the selection does not come from); in crawl B it may or may not have happened.
    for remove_first in [true, false] {
        let t = two_crawls().await;
        let a_before = urls_of(&t.a_path);
        let remove = {
            let app = t.app.clone();
            let b = t.b.clone();
            async move {
                match app
                    .dispatch(
                        ctx(),
                        "crawl.remove",
                        args(json!({"kind":"ids","ids":[1,2,3]}), Some(&b)),
                    )
                    .await
                {
                    Ok(v) => {
                        let op: OpHandle = serde_json::from_value(v).unwrap();
                        let s = app.ops().wait(&op.op_id).await.unwrap();
                        format!("{:?} {:?}", s.state, s.error.map(|e| e.message_key))
                    }
                    Err(e) => format!("refused {}", e.message_key),
                }
            }
        };
        let open = {
            let app = t.app.clone();
            let path = t.a_path.clone();
            async move {
                app.dispatch(ctx(), "crawl.open", json!({"kind":"path","path":path}))
                    .await
                    .map(|_| ())
                    .map_err(|e| e.message_key)
            }
        };
        let (removed, opened) = if remove_first {
            let r = tokio::spawn(remove);
            tokio::task::yield_now().await;
            let o = open.await;
            (r.await.unwrap(), o)
        } else {
            let o = tokio::spawn(open);
            tokio::task::yield_now().await;
            let r = remove.await;
            (r, o.await.unwrap())
        };
        assert_eq!(
            urls_of(&t.a_path),
            a_before,
            "crawl A never loses URLs (remove: {removed}, open: {opened:?})"
        );
        let b_now = urls_of(&t.b_path);
        assert!(
            b_now.len() >= urls_of(&t.a_path).len() - 3,
            "crawl B lost at most the selected URLs: {removed}"
        );
    }
}
