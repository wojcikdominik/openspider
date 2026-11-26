//! M4-K11 (Nacharbeit 26.11.2026): `--load-crawl <ID> --resume` auf einem Crawl, den ein anderer Prozess als aktiver
//! Besitzer führt (KI-Lauf, simulierter Heartbeat) – „läuft noch (Prozess <pid>)“ und Exit-Code 1 wie bei einem dort
//! laufenden Crawl, auch bei gespeichertem `Completed` und `Paused` (08 AI-020 [M4], Wirkung b; 01 CRAWL-125, Nachtrag
//! K4-22). Die CLI prüft mit der Systemuhr; der Heartbeat liegt deshalb in der Zukunft und gilt nach CRAWL-125 als
//! aktuell (keine Wanduhr im Test).
mod common;

use std::path::Path;

use common::Env;
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};

const FUTURE: &str = "2099-01-01T00:00:00.000Z";

fn foreign_pid() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/crawl-files/active-owner/cases.json");
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    cases["foreign_pid"].as_str().unwrap().to_string()
}

/// Crawl-Datei `<id>.ospider` im Crawl-Ordner der Umgebung; `Completed` mit Restwarteschlange (Limit), damit sie ohne
/// Besitzer fortsetzbar wäre.
fn crawl(env: &Env, seed: u64, state: &str, pid: &str) -> String {
    let created = Timestamp::from_db("2026-11-26T12:00:00.000Z").unwrap();
    let id = SeededIds::new(seed).new_crawl_id(created);
    let dir = env.crawls_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{id}.ospider"));
    let init = CrawlInit {
        crawl_id: id,
        mode: CrawlMode::Spider,
        start_url: Some("https://site.test/".into()),
        list_source: None,
        project: String::new(),
        name: "active-owner".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    let file = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    file.set_meta("state", state).unwrap();
    file.set_meta("started_at", &created.to_db()).unwrap();
    file.conn()
        .execute(
            "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length) \
             VALUES (1,'https://site.test/a','https://site.test/a','https://site.test',1,'queued','link',0,19)",
            [],
        )
        .unwrap();
    file.conn()
        .execute(
            "INSERT INTO queue(url_id,seq,depth,priority) VALUES (1,1,1,0)",
            [],
        )
        .unwrap();
    if state == "Completed" {
        file.set_meta("end_reason", "limit_reached").unwrap();
    }
    file.set_meta("owner_kind", "mcp-stdio").unwrap();
    file.set_meta("owner_pid", pid).unwrap();
    file.set_meta("heartbeat_at", FUTURE).unwrap();
    id.to_string()
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// spec: OUT-204
#[test]
fn resume_of_a_crawl_with_a_foreign_active_owner_ends_with_running_elsewhere_and_exit_code_1() {
    let env = Env::new();
    let pid = foreign_pid();
    for (seed, state) in [(1, "Completed"), (2, "Paused")] {
        let id = crawl(&env, seed, state, &pid);
        let run = env.run(&["--load-crawl", id.as_str(), "--resume"]);
        assert_eq!(run.code, 1, "{state}: {}", run.dump());
        assert!(
            run.stderr.contains("läuft noch") && run.stderr.contains(&format!("Prozess {pid}")),
            "{state}: {}",
            run.dump()
        );
        assert!(run.last_line().ends_with("(Exit-Code 1)"), "{}", run.dump());
        let path = env.crawls_dir().join(format!("{id}.ospider"));
        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        assert_eq!(file.get_meta("state").unwrap().as_deref(), Some(state));
        assert_eq!(file.get_meta("owner_pid").unwrap(), Some(pid.clone()));
    }
}

/// Ohne aktiven Besitzer (`owner_pid` leer wie in 1.0) bleibt die Meldung für einen abgeschlossenen Crawl ohne
/// Restwarteschlange die aus 1.0.
// spec: AI-020 [M4]
// spec: CRAWL-128
#[test]
fn without_an_owner_the_completed_message_of_one_point_zero_remains() {
    let env = Env::new();
    let id = crawl(&env, 3, "Completed", "");
    let path = env.crawls_dir().join(format!("{id}.ospider"));
    {
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        file.conn().execute("DELETE FROM queue", []).unwrap();
    }
    let run = env.run(&["--load-crawl", id.as_str(), "--resume"]);
    assert_eq!(run.code, 1, "{}", run.dump());
    assert!(!run.stderr.contains("läuft noch"), "{}", run.dump());
    assert!(run.stderr.contains("Completed"), "{}", run.dump());
}
