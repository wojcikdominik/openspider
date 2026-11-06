//! Messung B4-LIVEDETAIL (M2-N32, UI-145): Zeitaufteilung von `view.open` für Inlinks/Outlinks während eines
//! laufenden Crawls, ohne UI. Nur Bericht, läuft nicht im normalen Testlauf (wie `os-store/tests/bench_links_build.rs`).
//!
//! ```text
//! cargo test -p os-app --release --test b4_livedetail_core -- --ignored --nocapture
//! # Größe/Rate/Threads wie e2e/beta/n04-live.spec.ts (Standard): OS_LIVEDETAIL_PAGES=5000 OS_LIVEDETAIL_RATE=150
//! # OS_LIVEDETAIL_THREADS=4; 0 bei der Rate = ungedrosselt.
//! ```
//!
//! Je Messpunkt (Anteil gecrawlter Seiten) und Ansicht (Inlinks/Outlinks der Start-URL und einer Seite aus der Mitte)
//! fünf Wiederholungen von:
//! - `open_ms`: `view.open` über den Dispatcher (Kernabfrage der Link-IDs, erste Seite, Erstfenster), bis zum `Value`;
//! - `ser_ms`/`bytes`: Serialisierung der Antwort zu JSON (Anteil der IPC-Serialisierung im Kern);
//! - `ids_ms`/`ids`: nur die Link-ID-Abfrage (`selected_link_ids`) auf einer Leseverbindung;
//! - `page_ms`: nur die erste Seite (200 Zeilen, `selected_links_by_ids`);
//! - `rows_ms`: das erste `view.rows` (0–199) nach dem Öffnen, das aus dem Erstfenster bedient wird.
//!
//! Ausgabe: eine JSON-Zeile `B4_LIVEDETAIL_CORE {...}` je Messpunkt auf stderr (Median und Maximum je Wert).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1e6).round() / 1e3
}

fn stats(values: &mut [f64]) -> Value {
    values.sort_by(|a, b| a.total_cmp(b));
    json!({"median": values[values.len() / 2], "max": values[values.len() - 1]})
}

async fn crawled(app: &os_app::App) -> u64 {
    app.read_pool()
        .run(key(app), |f, _| {
            Ok(f.conn()
                .query_row("SELECT COUNT(*) FROM urls WHERE state='done'", [], |r| {
                    r.get::<_, u64>(0)
                })?)
        })
        .await
        .unwrap()
}

/// Direkte Store-Abfragen derselben Auswahl wie `view.open` mit Filter „All“ und Standardsortierung.
async fn store_split(app: &os_app::App, url: i64, incoming: bool) -> (f64, usize, f64) {
    app.read_pool()
        .run(key(app), move |f, _| {
            use os_store::queries::{app_support, PageReq};
            let selection = app_support::LinkSelection {
                url_ids: vec![os_model::UrlId(url)],
                incoming,
                exclude_resources: !incoming,
                ..Default::default()
            };
            let started = Instant::now();
            let ids = app_support::selected_link_ids(f.conn(), &selection, &PageReq::new(0, 1))?;
            let ids_ms = ms(started.elapsed());
            let started = Instant::now();
            let first: Vec<i64> = ids.iter().take(200).copied().collect();
            app_support::selected_links_by_ids(f.conn(), &selection, &first)?;
            Ok((ids_ms, ids.len(), ms(started.elapsed())))
        })
        .await
        .unwrap()
}

// spec: UI-145
#[tokio::test]
#[ignore = "Messrezept B4-LIVEDETAIL, nur auf Anforderung (--ignored)"]
async fn live_link_view_open_time_split() {
    let pages = env_u64("OS_LIVEDETAIL_PAGES", 5000);
    let rate = env_u64("OS_LIVEDETAIL_RATE", 150);
    let threads = env_u64("OS_LIVEDETAIL_THREADS", 4);
    let site = os_testsite::TestSite::start_spec(
        &format!("synthetic:n={pages},links=30,seed=7"),
        os_testsite::SiteOptions::default(),
    )
    .await
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let mut config = cfg();
    config.speed.threads = threads as u32;
    config.speed.max_urls_per_sec = (rate > 0).then_some(rate as f64);
    config.save_default(&app.config().paths.settings).unwrap();
    call(
        &app,
        "crawl.start",
        serde_json::to_value(request(site.url("/"))).unwrap(),
    )
    .await;
    for percent in [10u64, 30, 60, 90] {
        let target = pages * percent / 100;
        tokio::time::timeout(Duration::from_secs(900), async {
            while crawled(&app).await < target {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("crawl did not reach the measurement point");
        let mid = (pages / 2) as i64;
        let mut views = Vec::new();
        for (name, kind, url) in [
            ("inlinks-start", "inlinks", 1i64),
            ("outlinks-start", "outlinks", 1),
            ("inlinks-mid", "inlinks", mid),
            ("outlinks-mid", "outlinks", mid),
        ] {
            let (mut open, mut ser, mut rows, mut ids_ms, mut page) =
                (vec![], vec![], vec![], vec![], vec![]);
            let (mut bytes, mut ids, mut total) = (0usize, 0usize, Value::Null);
            for _ in 0..5 {
                let spec = json!({"kind":kind,"urlIds":[url],"filter":{"id":"All"},"search":null,"sort":[]});
                let started = Instant::now();
                let handle = call(&app, "view.open", json!({"spec":spec,"columns":[]})).await;
                open.push(ms(started.elapsed()));
                let started = Instant::now();
                bytes = serde_json::to_vec(&handle).unwrap().len();
                ser.push(ms(started.elapsed()));
                total = handle["total"].clone();
                let started = Instant::now();
                call(&app, "view.rows", json!({"viewId":handle["viewId"],"version":handle["version"],"offset":0,"limit":200,"columns":[]})).await;
                rows.push(ms(started.elapsed()));
                call(&app, "view.close", json!({"viewId":handle["viewId"]})).await;
                let (i, n, p) = store_split(&app, url, kind == "inlinks").await;
                ids_ms.push(i);
                ids = n;
                page.push(p);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            views.push(json!({"view":name,"total":total,"ids":ids,"bytes":bytes,
                "open_ms":stats(&mut open),"ser_ms":stats(&mut ser),"rows_ms":stats(&mut rows),
                "ids_ms":stats(&mut ids_ms),"page_ms":stats(&mut page)}));
        }
        let links = app
            .read_pool()
            .run(key(&app), |f, _| {
                Ok(f.conn()
                    .query_row("SELECT COUNT(*) FROM links", [], |r| r.get::<_, u64>(0))?)
            })
            .await
            .unwrap();
        eprintln!(
            "B4_LIVEDETAIL_CORE {}",
            json!({"percent":percent,"crawled":crawled(&app).await,"links":links,"views":views})
        );
    }
    call(&app, "crawl.stop", json!(null)).await;
}
