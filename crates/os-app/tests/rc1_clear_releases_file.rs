//! RC1-EBUSY: "Clear" (UI-003) leaves no handle on the crawl file. The crawl stays in the crawl management, where
//! deleting it removes the file (OUT-228); on Windows any open handle makes that deletion fail.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Grace for reads that were already running when the view was cleared; a leaked handle stays forever.
const GRACE: Duration = Duration::from_secs(2);

fn companions(file: &Path) -> Vec<PathBuf> {
    ["", "-wal", "-shm", "-journal"]
        .iter()
        .map(|suffix| {
            let mut name = file.as_os_str().to_owned();
            name.push(suffix);
            PathBuf::from(name)
        })
        .collect()
}

/// Open descriptors of this process on `file` and its companion files.
#[cfg(target_os = "linux")]
fn open_fds(file: &Path) -> Vec<String> {
    let names = companions(file);
    std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .flatten()
        .filter_map(|entry| {
            let target = std::fs::read_link(entry.path()).ok()?;
            names.contains(&target).then(|| {
                format!(
                    "fd {} -> {}",
                    entry.file_name().to_string_lossy(),
                    target.display()
                )
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
async fn assert_no_open_fds(file: &Path, after: &str) {
    let deadline = Instant::now() + GRACE;
    let mut open = open_fds(file);
    while !open.is_empty() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
        open = open_fds(file);
    }
    assert!(
        open.is_empty(),
        "{after}: handles on the crawl file remain: {open:?}"
    );
}

/// Deleting the file and its companions succeeds (on Windows only once no handle is open).
async fn assert_deletable(file: &Path) {
    for path in companions(file) {
        let deadline = Instant::now() + GRACE;
        loop {
            match std::fs::remove_file(&path) {
                Ok(()) => break,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) if Instant::now() >= deadline => {
                    panic!("{} cannot be deleted after clearing: {e}", path.display())
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
    }
    assert!(!file.exists());
}

// spec: UI-003
// spec: OUT-228
// spec: CRAWL-127
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn clearing_an_opened_crawl_releases_every_handle_on_its_file() {
    let site = os_testsite::TestSite::start_dir(fixture(), 0)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    cfg().save_default(&app.config().paths.settings).unwrap();
    call(
        &app,
        "crawl.start",
        serde_json::to_value(request(site.url("/"))).unwrap(),
    )
    .await;
    completed(&app).await;
    let file = app.sessions().current(&window()).unwrap().path;
    call(&app, "crawl.clear", json!(null)).await;
    #[cfg(target_os = "linux")]
    assert_no_open_fds(&file, "clear after the crawl").await;

    // Open again (File > Open, "Recently opened"), let the live feed and a tab view read it, clear again.
    open(&app, &file).await;
    for _ in 0..5 {
        os_app::session::poll(&app).await.unwrap();
    }
    let view = call(
        &app,
        "view.open",
        json!({"spec":{"kind":"tab","tab":"Internal","filter":"All","search":null,"sort":[]},"columns":["Address"]}),
    )
    .await;
    assert!(view["viewId"].is_string(), "{view}");
    os_app::session::poll(&app).await.unwrap();
    call(&app, "crawl.clear", json!(null)).await;
    #[cfg(target_os = "linux")]
    assert_no_open_fds(&file, "clear after reopening").await;
    assert_deletable(&file).await;
}
