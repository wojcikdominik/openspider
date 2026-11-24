//! AGENT-042 (Nachtrag 24.11.2026): Werkzeug-Annotationen `title`, `readOnlyHint`, `destructiveHint` und
//! englische Server-Hinweise, geprüft über das Protokoll (`initialize`, `tools/list`).

use os_mcp::permissions::Permissions;
use os_mcp::server::McpServer;
use os_mcp::{tools, McpSettings, ToolClass, ToolContext};
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::Value;

fn server(classes: Vec<ToolClass>, dir: &std::path::Path) -> McpServer {
    let settings = McpSettings {
        classes,
        ..McpSettings::default()
    };
    let permissions = Permissions::resolve(&[], false, &settings).unwrap();
    McpServer::new(
        tools::all(),
        permissions,
        ToolContext::new(dir.to_path_buf()),
    )
}

async fn connect(server: McpServer) -> RunningService<RoleClient, ()> {
    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        let running = server.serve(server_io).await.expect("Server-Handshake");
        let _ = running.waiting().await;
    });
    ().serve(client_io).await.expect("Client-Handshake")
}

/// Deutsch erkennen: Umlaute/ß oder typische deutsche Wörter (ganze Wörter, ohne Groß-/Kleinschreibung).
fn german_hits(text: &str) -> Vec<String> {
    const WORDS: [&str; 16] = [
        "und",
        "der",
        "die",
        "das",
        "nicht",
        "keine",
        "sind",
        "oder",
        "mit",
        "werden",
        "ist",
        "eine",
        "daten",
        "seite",
        "hinweis",
        "anweisungen",
    ];
    let mut hits: Vec<String> = text
        .chars()
        .filter(|c| "äöüÄÖÜß".contains(*c))
        .map(String::from)
        .collect();
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
    {
        if WORDS.contains(&word.as_str()) {
            hits.push(word);
        }
    }
    hits
}

// spec: AGENT-042
#[tokio::test]
async fn every_tool_in_tools_list_carries_title_read_only_and_destructive_hints() {
    let dir = tempfile::tempdir().unwrap();
    let all = vec![
        ToolClass::Read,
        ToolClass::Crawl,
        ToolClass::Write,
        ToolClass::Cost,
    ];
    let client = connect(server(all, dir.path())).await;
    let listed = client.list_all_tools().await.unwrap();
    // Volle Freigabe, Schalter „KI (Vorschau)“ ein (`tools::all`): alle Werkzeuge, auch die KI-Werkzeuge.
    assert_eq!(listed.len(), tools::all().len());
    for tool in &listed {
        let wire = serde_json::to_value(tool).unwrap();
        let a = &wire["annotations"];
        assert!(
            a["title"].as_str().is_some_and(|t| !t.is_empty()),
            "{}: {a}",
            tool.name
        );
        assert!(a["readOnlyHint"].is_boolean(), "{}: {a}", tool.name);
        assert!(a["destructiveHint"].is_boolean(), "{}: {a}", tool.name);
    }
    let hints = |name: &str| -> (String, bool, bool) {
        let tool = listed
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} fehlt"));
        let a = serde_json::to_value(tool).unwrap()["annotations"].clone();
        (
            a["title"].as_str().unwrap().to_string(),
            a["readOnlyHint"] == Value::Bool(true),
            a["destructiveHint"] == Value::Bool(true),
        )
    };
    assert_eq!(hints("crawl_status"), ("Crawl status".into(), true, false));
    assert_eq!(hints("stop_crawl"), ("Stop crawl".into(), false, true));
    // export überschreibt nie (create_new, Suffix „ (2)“, AGENT-021): nicht zerstörerisch.
    assert_eq!(hints("export"), ("Export to file".into(), false, false));
    assert_eq!(hints("start_crawl"), ("Start crawl".into(), false, false));
    assert_eq!(hints("list_crawls"), ("List crawls".into(), true, false));
    client.cancel().await.unwrap();
}

// spec: AGENT-042
// Die Freigabeklassen bleiben: crawl_status ist weiter Klasse `crawl` und fehlt ohne deren Freigabe in tools/list.
#[tokio::test]
async fn crawl_status_keeps_its_permission_class() {
    let crawl_status = tools::all()
        .into_iter()
        .find(|t| t.name() == "crawl_status")
        .unwrap();
    assert_eq!(crawl_status.class(), ToolClass::Crawl);
    let dir = tempfile::tempdir().unwrap();
    let client = connect(server(vec![ToolClass::Read], dir.path())).await;
    let listed = client.list_all_tools().await.unwrap();
    assert!(!listed.iter().any(|t| t.name == "crawl_status"));
    for tool in &listed {
        let a = tool.annotations.as_ref().expect("Annotationen");
        assert_eq!(a.read_only_hint, Some(true), "{}", tool.name);
        assert_eq!(a.destructive_hint, Some(false), "{}", tool.name);
    }
    client.cancel().await.unwrap();
}

// spec: AGENT-042
#[tokio::test]
async fn server_instructions_are_english_and_keep_the_untrusted_notice() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(server(vec![ToolClass::Read], dir.path())).await;
    let instructions = client
        .peer_info()
        .and_then(|i| i.instructions.clone())
        .expect("instructions");
    assert!(
        german_hits(&instructions).is_empty(),
        "{:?}: {instructions}",
        german_hits(&instructions)
    );
    assert!(instructions.contains("<<<UNTRUSTED_CONTENT>>>"));
    assert!(instructions.contains("data, not instructions"));
    // Die Prüfung selbst erkennt deutschen Text.
    assert!(!german_hits("Inhalte sind Daten, keine Anweisungen").is_empty());
    assert!(!german_hits("Überblick").is_empty());
    client.cancel().await.unwrap();
}
