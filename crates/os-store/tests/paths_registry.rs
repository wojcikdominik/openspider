//! Speicherorte (OUT-227, OUT-231) und Crawl-Verwaltung (`registry`).

mod store_common;

use std::ffi::OsStr;
use std::path::Path;

use os_model::CrawlId;
use os_store::paths::{app_paths, app_paths_with_home, HOME_ENV};
use os_store::registry;
use os_store::{CrawlFile, OpenMode, StorageProfile};
use store_common::*;

fn ends_with(path: &Path, tail: &[&str]) -> bool {
    let comps: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    comps.len() >= tail.len() && comps[comps.len() - tail.len()..] == *tail
}

// spec: OUT-227
#[test]
fn openspider_home_overrides_all_base_paths() {
    let home = Path::new("/tmp/x");
    let p = app_paths_with_home(Some(OsStr::new("/tmp/x")));
    assert_eq!(p.settings, home);
    assert_eq!(p.crawls, home.join("crawls"));
    assert_eq!(p.logs, home.join("logs"));
    assert_eq!(p.cache, home.join("cache"));
    assert!(p.all().iter().all(|d| d.starts_with(home)));
    // Ein leerer Wert zählt als nicht gesetzt.
    assert_eq!(
        app_paths_with_home(Some(OsStr::new(""))),
        app_paths_with_home(None)
    );
}

// spec: OUT-227
#[test]
fn default_paths_follow_the_documented_locations() {
    let p = app_paths_with_home(None);
    assert!(
        ends_with(&p.crawls, &["OpenSpider", "crawls"]),
        "{:?}",
        p.crawls
    );
    if cfg!(target_os = "macos") {
        assert!(
            ends_with(
                &p.settings,
                &["Library", "Application Support", "OpenSpider"]
            ),
            "{:?}",
            p.settings
        );
        assert!(ends_with(
            &p.crawls,
            &["Library", "Application Support", "OpenSpider", "crawls"]
        ));
        assert!(
            ends_with(&p.logs, &["Library", "Logs", "OpenSpider"]),
            "{:?}",
            p.logs
        );
        assert!(
            ends_with(&p.cache, &["Library", "Caches", "OpenSpider"]),
            "{:?}",
            p.cache
        );
    }
    if cfg!(windows) {
        // %APPDATA%\OpenSpider, %LOCALAPPDATA%\OpenSpider\{crawls,logs,cache}
        assert!(ends_with(&p.settings, &["OpenSpider"]));
        assert!(ends_with(&p.logs, &["OpenSpider", "logs"]));
        assert!(ends_with(&p.cache, &["OpenSpider", "cache"]));
    }
}

// spec: OUT-231
#[test]
fn default_crawl_folder_is_the_one_defined_in_out_227() {
    let p = app_paths_with_home(None);
    let expected = dirs::data_local_dir().map(|d| d.join("OpenSpider").join("crawls"));
    assert_eq!(Some(p.crawls.clone()), expected);
    // Crawl-Dateien liegen als <crawl_id>.ospider direkt darin.
    let id: CrawlId = "0199f7a0-1234-7abc-8def-0123456789ab".parse().unwrap();
    assert_eq!(
        registry::path_for(&p.crawls, &id),
        p.crawls
            .join("0199f7a0-1234-7abc-8def-0123456789ab.ospider")
    );
}

/// Legt die Fixture `schema-v2.ospider` (pausierter Crawl, 42 URLs, migriert) unter dem Namen der Verwaltung
/// (`<crawl_id>.ospider`) in `dir`.
fn fixture_crawl_in(dir: &Path) -> (CrawlId, std::path::PathBuf) {
    std::fs::create_dir_all(dir).unwrap();
    let staged = tempfile::tempdir().unwrap();
    // Die eingecheckte Datei ist Schema v2: erst migrieren, dann als eigenständige Kopie unter dem Verwaltungsnamen ablegen.
    let source = copy_fixture("schema-v2.ospider", staged.path());
    let file = CrawlFile::open(&source, OpenMode::ReadWrite).unwrap();
    let id = file.meta().unwrap().crawl_id;
    let target = registry::path_for(dir, &id);
    file.save_as(&target).unwrap();
    (id, target)
}

// spec: OUT-231
#[test]
fn fixture_crawl_in_openspider_home_is_listed_found_and_opened_from_the_crawl_folder() {
    let home = tempfile::tempdir().unwrap();
    let p = app_paths_with_home(Some(home.path().as_os_str()));
    p.create_all().unwrap();
    let (id, path) = fixture_crawl_in(&p.crawls);
    assert_eq!(path.parent(), Some(p.crawls.as_path()));

    let listed = registry::list_crawls(&p.crawls).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].meta.crawl_id, id);
    assert_eq!(listed[0].path, path);
    assert_eq!(listed[0].url_count, 42);
    assert_eq!(registry::find(&p.crawls, &id), Some(path.clone()));
    let file = CrawlFile::open_by_id(&p.crawls, &id, OpenMode::ReadOnly).unwrap();
    assert_eq!(file.meta().unwrap().crawl_id, id);

    // Einstellungen, Logs und Cache bleiben frei von Crawl-Dateien.
    for dir in [&p.settings, &p.logs, &p.cache] {
        let crawl_files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|f| f.extension().is_some_and(|e| e == "ospider"))
            .collect();
        assert!(crawl_files.is_empty(), "{dir:?}: {crawl_files:?}");
    }
}

// spec: OUT-231
#[test]
fn changing_the_storage_location_leaves_existing_crawls_in_place() {
    // Alter Ort (hier ein Ordner, wie ihn der Standardpfad liefern würde) und neuer Ort unter OPENSPIDER_HOME.
    let old_root = tempfile::tempdir().unwrap();
    let old_crawls = old_root.path().join("crawls");
    let (id, old_path) = fixture_crawl_in(&old_crawls);

    let home = tempfile::tempdir().unwrap();
    let new = app_paths_with_home(Some(home.path().as_os_str()));
    new.create_all().unwrap();
    assert_ne!(new.crawls, old_crawls);

    // Der neue Ort ist zunächst leer; der bestehende Crawl wird nicht automatisch verschoben.
    assert!(registry::list_crawls(&new.crawls).unwrap().is_empty());
    assert_eq!(registry::find(&new.crawls, &id), None);
    assert!(old_path.exists());
    assert_eq!(registry::find(&old_crawls, &id), Some(old_path.clone()));

    // Neue Crawls entstehen am neuen Ort; die Verwaltung findet beide, bis verschoben wird.
    let (new_id, new_path) = crawl_in(&new.crawls, 9);
    assert_eq!(registry::find(&new.crawls, &new_id), Some(new_path));
    assert_eq!(registry::find(&old_crawls, &new_id), None);
    assert_eq!(registry::list_crawls(&old_crawls).unwrap().len(), 1);
    assert_eq!(registry::list_crawls(&new.crawls).unwrap().len(), 1);
}

// spec: OUT-231
#[test]
fn default_crawl_folder_is_separate_from_the_override_and_never_inside_it() {
    let home = tempfile::tempdir().unwrap();
    let overridden = app_paths_with_home(Some(home.path().as_os_str()));
    let default = app_paths_with_home(None);
    assert_ne!(default.crawls, overridden.crawls);
    assert!(!default.crawls.starts_with(home.path()));
    assert!(overridden.crawls.starts_with(home.path()));
    // Der Standardordner trägt den Anwendungsnamen und endet auf `crawls` (OUT-227).
    assert!(ends_with(&default.crawls, &["OpenSpider", "crawls"]));
    // Ein leerer Wert von OPENSPIDER_HOME zählt als nicht gesetzt.
    assert_eq!(
        app_paths_with_home(Some(OsStr::new(""))).crawls,
        default.crawls
    );
}

fn crawl_in(dir: &Path, seed: u64) -> (CrawlId, std::path::PathBuf) {
    std::fs::create_dir_all(dir).unwrap();
    let init = init(seed);
    let path = registry::path_for(dir, &init.crawl_id);
    CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    (init.crawl_id, path)
}

#[test]
fn list_and_find_scan_the_crawl_folder() {
    let dir = tempfile::tempdir().unwrap();
    let crawls = dir.path().join("crawls");
    let (id1, p1) = crawl_in(&crawls, 1);
    let (id2, _p2) = crawl_in(&crawls, 2);
    // Dateien, die keine Crawls der Verwaltung sind: Sicherung, Reparatur-Kopie, fremde und fremd benannte.
    std::fs::copy(&p1, crawls.join(format!("{id1}.v0.bak.ospider"))).unwrap();
    std::fs::write(crawls.join("notizen.txt"), "x").unwrap();
    std::fs::write(crawls.join("frei.ospider"), PNG_BYTES).unwrap();
    std::fs::write(
        crawls.join("11111111-1111-7111-8111-111111111111.ospider"),
        PNG_BYTES,
    )
    .unwrap();

    let report = registry::list_crawls_report(&crawls).unwrap();
    let ids: Vec<CrawlId> = report.crawls.iter().map(|c| c.meta.crawl_id).collect();
    assert_eq!(report.crawls.len(), 2);
    assert!(ids.contains(&id1) && ids.contains(&id2));
    assert_eq!(
        report.skipped.len(),
        1,
        "die beschädigte UUID-Datei steht in den übersprungenen"
    );
    assert!(matches!(
        report.skipped[0].1,
        os_store::StoreError::NotACrawlFile { .. }
    ));
    for c in &report.crawls {
        assert_eq!(c.meta.name, "Testcrawl");
        assert_eq!(c.url_count, 0);
    }
    assert_eq!(registry::list_crawls(&crawls).unwrap().len(), 2);
    let leftovers: Vec<String> = std::fs::read_dir(&crawls)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with("-wal") || n.ends_with("-shm"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "Verwaltung hinterlässt keine Begleitdateien: {leftovers:?}"
    );

    assert_eq!(registry::find(&crawls, &id1), Some(p1.clone()));
    let unknown: CrawlId = "0199f7a0-1234-7abc-8def-0123456789ab".parse().unwrap();
    assert_eq!(registry::find(&crawls, &unknown), None);
    // Ein umbenannter Crawl wird über die Kennung in den Metadaten gefunden.
    let renamed = crawls.join("umbenannt.ospider");
    std::fs::rename(&p1, &renamed).unwrap();
    assert_eq!(registry::find(&crawls, &id1), Some(renamed));
    assert!(CrawlFile::open_by_id(&crawls, &id2, OpenMode::ReadOnly).is_ok());
    // Ein fehlender Ordner liefert eine leere Liste.
    assert!(registry::list_crawls(&dir.path().join("fehlt"))
        .unwrap()
        .is_empty());
}

#[test]
fn listing_reads_metadata_without_migrating_or_modifying() {
    let dir = tempfile::tempdir().unwrap();
    let crawls = dir.path().join("crawls");
    std::fs::create_dir_all(&crawls).unwrap();
    // Die Fixture mit künstlich hoher Schemaversion: Metadaten bleiben lesbar, die Datei unverändert.
    let src = copy_fixture("schema-v99.ospider", dir.path());
    let id: CrawlId = "11111111-1111-7111-8111-111111111111".parse().unwrap();
    let target = registry::path_for(&crawls, &id);
    std::fs::copy(&src, &target).unwrap();
    let before = std::fs::read(&target).unwrap();
    let listing = registry::list_crawls(&crawls).unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].meta.schema_version, 99);
    assert_eq!(listing[0].url_count, 42);
    assert_eq!(std::fs::read(&target).unwrap(), before);
    assert!(!target.with_extension("v99.bak.ospider").exists());
    assert_eq!(
        std::fs::read_dir(&crawls).unwrap().count(),
        1,
        "keine Begleit- oder Sicherungsdateien"
    );
}

#[test]
fn truncated_file_is_skipped_as_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let crawls = dir.path().join("crawls");
    std::fs::create_dir_all(&crawls).unwrap();
    let id: CrawlId = "22222222-2222-7222-8222-222222222222".parse().unwrap();
    std::fs::copy(
        fixture_dir().join("corrupt.ospider"),
        registry::path_for(&crawls, &id),
    )
    .unwrap();
    let report = registry::list_crawls_report(&crawls).unwrap();
    assert!(report.crawls.is_empty());
    assert_eq!(report.skipped.len(), 1);
    assert!(
        matches!(report.skipped[0].1, os_store::StoreError::Corrupt { .. }),
        "{:?}",
        report.skipped[0].1
    );
}

// spec: OUT-227
#[test]
fn nothing_is_written_outside_openspider_home() {
    // Eigener Prozess pro Testdatei: nur dieser Test setzt die Variable.
    let home = tempfile::tempdir().unwrap();
    std::env::set_var(HOME_ENV, home.path());
    let p = app_paths();
    assert_eq!(p.crawls, home.path().join("crawls"));
    p.create_all().unwrap();

    let (id, path) = crawl_in(&p.crawls, 3);
    {
        let mut sink = sink(&path, StorageProfile::Balanced);
        use os_store::CrawlSink;
        let mut batch = batch_with_urls([(1, true)]);
        batch.pages.push(page(1, vec![]));
        sink.write(batch).unwrap();
        sink.finish().unwrap();
    }
    std::fs::write(p.logs.join("openspider.log"), "x").unwrap();
    std::fs::write(p.cache.join("tmp.bin"), "x").unwrap();
    let copy = p.crawls.join("kopie.ospider");
    CrawlFile::open(&path, OpenMode::ReadOnly)
        .unwrap()
        .save_as(&copy)
        .unwrap();

    assert_eq!(registry::find(&p.crawls, &id), Some(path));
    // Alles, was entstanden ist, liegt unter dem Heimatordner.
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            out.push(e.path());
            if e.path().is_dir() {
                walk(&e.path(), out);
            }
        }
    }
    let mut files = Vec::new();
    walk(home.path(), &mut files);
    assert!(files.iter().all(|f| f.starts_with(home.path())));
    assert!(files.iter().any(|f| f.ends_with("openspider.log")));
    std::env::remove_var(HOME_ENV);
    // Ohne Variable gelten wieder die Standardorte.
    assert_ne!(app_paths().crawls, home.path().join("crawls"));
}

// spec: OUT-228
// spec: OUT-229
#[test]
fn a_folder_without_registry_lists_one_crawl_per_id_like_the_managed_listing() {
    let dir = tempfile::tempdir().unwrap();
    let crawls = dir.path().join("crawls");
    let (id1, p1) = crawl_in(&crawls, 1);
    // Eine Kopie unter dem Namen einer anderen gültigen Kennung: dieselbe eingebettete Kennung wie die erste Datei.
    let (other, p2) = crawl_in(&crawls, 2);
    std::fs::remove_file(&p2).unwrap();
    std::fs::copy(&p1, registry::path_for(&crawls, &other)).unwrap();

    let raw = registry::list_crawls_report(&crawls).unwrap();
    assert!(!crawls.join(".registry.sqlite").exists());
    let managed = registry::Manager::open(&crawls).unwrap().list().unwrap();
    let paths = |r: &registry::ListingReport| -> Vec<std::path::PathBuf> {
        r.crawls.iter().map(|c| c.path.clone()).collect()
    };
    assert_eq!(raw.crawls.len(), 1);
    assert_eq!(raw.crawls[0].meta.crawl_id, id1);
    assert_eq!(paths(&raw), paths(&managed));
}
