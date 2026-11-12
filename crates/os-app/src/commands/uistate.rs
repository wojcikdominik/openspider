//! uistate.load/save (UI-125).
//!
//! Der UI-Zustand liegt als `ui-state.json` im Einstellungsordner; der Kern speichert den Inhalt unverändert,
//! atomar und mit Sicherung `.bak`.

use futures_util::future::BoxFuture;

use crate::dto::{CmdError, ErrorCode, UiStateJson};
use crate::registry::{CmdResult, CommandMeta, Registry};
use crate::runtime::error::{parse_args, to_json};
use crate::runtime::{App, CallCtx};

fn join_error(e: tokio::task::JoinError) -> CmdError {
    CmdError::new(ErrorCode::Internal, "err-internal").with_detail(e.to_string())
}

fn load<'a>(app: &'a App, _ctx: CallCtx, _args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let app = app.clone();
        // CRAWL-127: duplicates of one crawl file (beta.4, symbolic links) are merged on load.
        let state = tokio::task::spawn_blocking(move || {
            let mut state = app.uistate().load();
            crate::session::recent::merge_in_state(&mut state.state);
            state
        })
        .await
        .map_err(join_error)?;
        to_json(&state)
    })
}

fn save<'a>(app: &'a App, _ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let mut state: UiStateJson = parse_args(args)?;
        let app = app.clone();
        // CRAWL-127: the frontend compares paths as text; the core merges spellings of one file before storing.
        let saved = tokio::task::spawn_blocking(move || {
            crate::session::recent::merge_in_state(&mut state.state);
            app.uistate().save(&state)
        })
        .await
        .map_err(join_error)??;
        to_json(&saved)
    })
}

/// Trägt die Commands dieser Domäne in die Registry ein.
pub fn register(r: &mut Registry) {
    r.register(
        CommandMeta {
            name: "uistate.load",
            args: "void",
            ret: "UiStateJson",
            long_running: false,
        },
        load,
    );
    r.register(
        CommandMeta {
            name: "uistate.save",
            args: "UiStateJson",
            ret: "UiStateJson",
            long_running: false,
        },
        save,
    );
}
