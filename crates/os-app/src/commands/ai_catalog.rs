//! STUB(M4-F02): KI-Command-Katalog (Plan M4 3.3) – Namen, Argument-/Rückgabetypen und `long_running` stehen fest,
//! die Fachpakete (Besitzer je Command, M4.md 4.2) ersetzen den Handler und behalten den Vertrag bei.
//!
//! Die Bereiche aus 3.3 (`ai.accounts.*`, `ai.cost.estimate` …) erscheinen als `ai.<bereich><Aktion>`, weil der
//! Command-Vertrag genau einen Punkt verlangt (`<domäne>.<command>`, `manifest_contract`).
//!
//! Alle Stubs antworten mit `Unsupported`/`err-not-implemented` samt Besitzer. Kein Stub sendet einen KI-Request;
//! ohne Schalter „KI (Vorschau)“ (Plan M4 E1, E-2 b) ruft das Frontend sie nicht auf.
use futures_util::future::BoxFuture;

use crate::dto::CmdError;
use crate::registry::{CmdResult, CommandMeta, Handler, Registry};
use crate::{App, CallCtx};

/// Ein Command des Katalogs: Name, Argumente, Ergebnis, `long_running`, Besitzer (Paket-ID aus M4.md 4.2).
pub struct AiCommand {
    pub name: &'static str,
    pub args: &'static str,
    pub ret: &'static str,
    pub long_running: bool,
    pub owner: &'static str,
}

const fn cmd(
    name: &'static str,
    args: &'static str,
    ret: &'static str,
    long_running: bool,
    owner: &'static str,
) -> AiCommand {
    AiCommand {
        name,
        args,
        ret,
        long_running,
        owner,
    }
}

/// Der Katalog in Registrierungsreihenfolge (Reihenfolge der Bereiche wie in M4.md 3.3).
pub const CATALOG: &[AiCommand] = &[
    // ai.accounts.* – Anbieterkonten und Schlüssel (AI-009, INTEG-001)
    cmd("ai.accountsList", "void", "JsonValue", false, "M4-P01"),
    cmd("ai.accountsTest", "JsonValue", "JsonValue", true, "M4-P01"),
    // ai.settings.* – KI-Profil: Modellwahl je Aufgabe, Richtlinien (AI-011, AI-018, AI-019, AI-024)
    cmd("ai.settingsGet", "void", "JsonValue", false, "M4-U01"),
    cmd("ai.settingsSet", "JsonValue", "JsonValue", false, "M4-U01"),
    // ai.cost.estimate|budget|log – Kostenvoranschlag, Budget, Protokoll (AI-012 – AI-014, UI-013 [M4])
    cmd("ai.costEstimate", "AiRunArgs", "JsonValue", false, "M4-K05"),
    cmd("ai.costBudget", "void", "AiCostSnapshot", false, "M4-U01"),
    cmd("ai.costLog", "void", "JsonValue", false, "M4-K05"),
    // ai.consent.* – Datenflussübersicht und Einwilligung (AI-018)
    cmd("ai.consentStatus", "void", "JsonValue", false, "M4-U01"),
    cmd("ai.consentGrant", "JsonValue", "JsonValue", false, "M4-U01"),
    // ai.decisions.* / ai.questions.* / ai.tester.run – Decision-Engine (AI-030 ff., AI-042)
    cmd("ai.decisionsRun", "AiRunArgs", "JsonValue", true, "M4-D01"),
    cmd("ai.questionsList", "void", "JsonValue", false, "M4-D01"),
    cmd("ai.testerRun", "JsonValue", "JsonValue", true, "M4-D04"),
    // ai.suggest.* – Optimierungsvorschläge (AI-070 ff.)
    cmd("ai.suggestRun", "AiRunArgs", "JsonValue", true, "M4-V01"),
    // ai.redirects.* – Redirect-Mapping (AI-080 ff.)
    cmd("ai.redirectsRun", "AiRunArgs", "JsonValue", true, "M4-R01"),
    // ai.links.* – interne Linkvorschläge (AI-090 ff.)
    cmd("ai.linksRun", "AiRunArgs", "JsonValue", true, "M4-L01"),
    // ai.ask.* – Frag den Crawl (AI-100 ff.)
    cmd("ai.askRun", "JsonValue", "JsonValue", true, "M4-B01"),
    // ai.audit.* – Audit-Zusammenfassung und Maßnahmenplan (AI-106, AI-107)
    cmd("ai.auditRun", "JsonValue", "JsonValue", true, "M4-M01"),
    // ai.prompts.* – Paritäts-Prompts (INTEG-160 ff.)
    cmd("ai.promptsList", "void", "JsonValue", false, "M4-P02"),
    // ai.semantic.* – Embedding-Analysen (INTEG-180 ff.)
    cmd("ai.semanticRun", "AiRunArgs", "JsonValue", true, "M4-E01"),
    // ai.search.* – AI-Search-Sichtbarkeit (AI-120 ff.)
    cmd("ai.searchRun", "AiRunArgs", "JsonValue", true, "M4-S01"),
];

fn owner_of(name: &str) -> &'static str {
    CATALOG
        .iter()
        .find(|c| c.name == name)
        .map_or("M4-F02", |c| c.owner)
}

macro_rules! stub_handler {
    ($fn:ident, $name:literal) => {
        fn $fn<'a>(_: &'a App, _: CallCtx, _: serde_json::Value) -> BoxFuture<'a, CmdResult> {
            Box::pin(async move { Err(CmdError::unsupported($name, owner_of($name))) })
        }
    };
}

stub_handler!(accounts_list, "ai.accountsList");
stub_handler!(accounts_test, "ai.accountsTest");
stub_handler!(settings_get, "ai.settingsGet");
stub_handler!(settings_set, "ai.settingsSet");
stub_handler!(cost_estimate, "ai.costEstimate");
stub_handler!(cost_budget, "ai.costBudget");
stub_handler!(cost_log, "ai.costLog");
stub_handler!(consent_status, "ai.consentStatus");
stub_handler!(consent_grant, "ai.consentGrant");
stub_handler!(decisions_run, "ai.decisionsRun");
stub_handler!(questions_list, "ai.questionsList");
stub_handler!(tester_run, "ai.testerRun");
stub_handler!(redirects_run, "ai.redirectsRun");
stub_handler!(links_run, "ai.linksRun");
stub_handler!(ask_run, "ai.askRun");
stub_handler!(audit_run, "ai.auditRun");
stub_handler!(prompts_list, "ai.promptsList");
stub_handler!(semantic_run, "ai.semanticRun");
stub_handler!(search_run, "ai.searchRun");

fn handler(name: &str) -> Handler {
    match name {
        "ai.accountsList" => accounts_list,
        "ai.accountsTest" => accounts_test,
        "ai.settingsGet" => settings_get,
        "ai.settingsSet" => settings_set,
        "ai.costEstimate" => cost_estimate,
        "ai.costBudget" => cost_budget,
        "ai.costLog" => cost_log,
        "ai.consentStatus" => consent_status,
        "ai.consentGrant" => consent_grant,
        "ai.decisionsRun" => decisions_run,
        "ai.questionsList" => questions_list,
        "ai.testerRun" => tester_run,
        // M4-V01: Besitzer ersetzt den Handler (Vertrag unverändert).
        "ai.suggestRun" => super::ai_suggest::suggest_run,
        "ai.redirectsRun" => redirects_run,
        "ai.linksRun" => links_run,
        "ai.askRun" => ask_run,
        "ai.auditRun" => audit_run,
        "ai.promptsList" => prompts_list,
        "ai.semanticRun" => semantic_run,
        "ai.searchRun" => search_run,
        other => unreachable!("kein Handler für {other}"),
    }
}

/// Trägt den KI-Katalog ein.
pub fn register(r: &mut Registry) {
    for c in CATALOG {
        r.register(
            CommandMeta {
                name: c.name,
                args: c.args,
                ret: c.ret,
                long_running: c.long_running,
            },
            handler(c.name),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    // spec: UI-013 [M4]
    // Plan M4 3.3: alle Bereiche des KI-Katalogs sind registriert, je Command ein Besitzer aus M4.
    #[test]
    fn every_area_of_plan_3_3_has_a_command_with_an_m4_owner() {
        let mut r = Registry::new();
        register(&mut r);
        assert_eq!(r.len(), CATALOG.len());
        let names: BTreeSet<&str> = r.manifest().iter().map(|m| m.name).collect();
        for prefix in [
            "ai.accounts",
            "ai.settings",
            "ai.costEstimate",
            "ai.costBudget",
            "ai.costLog",
            "ai.consent",
            "ai.decisions",
            "ai.questions",
            "ai.testerRun",
            "ai.suggest",
            "ai.redirects",
            "ai.links",
            "ai.ask",
            "ai.audit",
            "ai.prompts",
            "ai.semantic",
            "ai.search",
        ] {
            assert!(names.iter().any(|n| n.starts_with(prefix)), "{prefix}");
        }
        for c in CATALOG {
            assert!(c.owner.starts_with("M4-"), "{}", c.name);
            assert_eq!(c.name.matches('.').count(), 1, "{}", c.name);
        }
        let budget = r.get("ai.costBudget").unwrap().0;
        assert_eq!((budget.args, budget.ret), ("void", "AiCostSnapshot"));
    }
}
