//! Unabhängige Abnahmetests M4-F01: Register-Stubs der KI-Werkzeuge im MCP-Server.
//!
//! Erwartungen ausschließlich aus `docs/spec/10-agent-interface.md` (AGENT-003, AGENT-030).
// acceptance: independent

use os_mcp::tools::all;
use os_mcp::ToolClass;

// spec: AGENT-030
// acceptance: independent
// `run_decisions`, `suggest_metadata`, `redirect_mapping`, `internal_link_suggestions` sind Werkzeuge der
// Klasse `cost` (AGENT-003: kostenpflichtige APIs inkl. KI).
#[test]
fn ai_tools_are_registered_in_class_cost() {
    let tools = all();
    for name in [
        "run_decisions",
        "suggest_metadata",
        "redirect_mapping",
        "internal_link_suggestions",
    ] {
        let tool = tools
            .iter()
            .find(|t| t.name() == name)
            .unwrap_or_else(|| panic!("Werkzeug {name} fehlt"));
        assert_eq!(tool.class(), ToolClass::Cost, "{name}");
        assert_eq!(
            tools.iter().filter(|t| t.name() == name).count(),
            1,
            "{name} doppelt"
        );
    }
}

// spec: AGENT-030
// acceptance: independent
// Das Eingabeschema kennt `confirm` (zweiter Aufruf mit `confirm: true` nach der Kostenschätzung, AGENT-003).
#[test]
fn ai_tools_accept_a_confirm_flag() {
    for tool in all().iter().filter(|t| {
        matches!(
            t.name(),
            "run_decisions" | "suggest_metadata" | "redirect_mapping" | "internal_link_suggestions"
        )
    }) {
        let schema = tool.input_schema();
        assert_eq!(
            schema["properties"]["confirm"]["type"],
            "boolean",
            "{}: {schema}",
            tool.name()
        );
    }
}
