//! Kommandozeile `openspider mcp` (und der schlanke Starter `openspider-mcp` für die Prozess-Tests).
//!
//! Ablauf von [`run`]: Schalter prüfen (Widerspruch `--read-only`/`--allow` → Exit 2) → `mcp.toml` lesen
//! → Freigaben auflösen → Server auf stdin/stdout starten. `--print-config <client>` gibt stattdessen die
//! Client-Konfiguration aus und beendet sich. Exit-Codes: 0 normales Ende, 1 Laufzeitfehler, 2 Aufruffehler.

use std::io::Write;
use std::path::PathBuf;

use os_store::paths::{app_paths, AppPaths};

use crate::error::McpError;
use crate::permissions::{class_name, Permissions};
use crate::print_config::{self, Client};
use crate::server::McpServer;
use crate::settings::{self, McpSettings};
use crate::tool::{Tool, ToolContext};
use crate::{tools, transport};

/// Argumente von `openspider mcp`.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct McpArgs {
    /// Ordner mit *.ospider-Dateien; keine Registry nötig (Standard: Crawl-Ordner der Anwendung).
    #[arg(long, value_name = "ORDNER")]
    pub crawl_dir: Option<PathBuf>,

    /// Internal disposable SQL sandbox process.
    #[arg(long, hide = true)]
    pub sql_worker: bool,

    /// Streamable HTTP on IPv4/IPv6 loopback instead of STDIO (token in the secret store).
    #[arg(long, hide = true)]
    pub http: bool,

    /// Loopback HTTP port (0 selects a free port).
    #[arg(long, requires = "http", hide = true)]
    pub port: Option<u16>,

    /// Freigegebene Werkzeugklassen (read, crawl, write, cost), kommagetrennt.
    #[arg(long, value_delimiter = ',', value_name = "KLASSEN")]
    pub allow: Vec<String>,

    /// Nur lesende Werkzeuge (überstimmt die Einstellungsdatei).
    #[arg(long)]
    pub read_only: bool,

    /// Gibt die Konfiguration für einen MCP-Client aus und beendet sich.
    #[arg(
        long,
        value_name = "CLIENT",
        value_parser = ["claude-desktop", "claude-code", "codex", "cursor"]
    )]
    pub print_config: Option<String>,
}

/// Startet den MCP-Server (bzw. gibt mit `--print-config` die Konfiguration aus); liefert den Exit-Code.
pub fn run(args: McpArgs) -> i32 {
    match execute(args) {
        Ok(()) => 0,
        Err(e) => {
            // Das Logging ist hier ggf. noch nicht eingerichtet; die Meldung muss auf jeden Fall nach stderr.
            let mut err = std::io::stderr().lock();
            let _ = writeln!(err, "openspider mcp: {e}");
            e.exit_code()
        }
    }
}

fn execute(args: McpArgs) -> Result<(), McpError> {
    if args.sql_worker {
        return os_store::readonly_sql::worker_main().map_err(McpError::Transport);
    }
    check_switches(&args)?;
    if let Some(name) = &args.print_config {
        return print_config_command(name, &args);
    }
    let paths = app_paths();
    // Plan M4 E1 (E-2 b): KI-Werkzeuge nur mit eingeschaltetem Schalter „KI (Vorschau)“ der App-Einstellungen.
    let preview = os_model::settings::AppSettings::load(&paths.settings)
        .map(|s| os_ai::PreviewSwitch::from_settings(&s))
        .unwrap_or(os_ai::PreviewSwitch::OFF);
    let server = build_server(&args, &paths, tools::offered(preview))?;
    transport::init_logging();
    log_start(&server, &args, &paths);
    run_blocking(
        server,
        args.http,
        args.port
            .or(settings::load(&settings::path_in(&paths))?.http.port),
    )
}

/// Baut den Server: liest `mcp.toml` aus `paths.settings`, löst die Freigaben auf (`--read-only` überstimmt
/// die Datei, `--allow` erweitert sie) und legt die Aufrufumgebung an.
///
/// # Errors
/// [`McpError::Usage`] bei widersprüchlichen oder unbekannten Schaltern, [`McpError::Settings`]/[`McpError::Io`]
/// bei unlesbarer `mcp.toml`.
pub fn build_server(
    args: &McpArgs,
    paths: &AppPaths,
    tools: Vec<Box<dyn Tool>>,
) -> Result<McpServer, McpError> {
    check_switches(args)?;
    let settings = settings::load(&settings::path_in(paths))?;
    let permissions = Permissions::resolve(&args.allow, args.read_only, &settings)?;
    let crawl_dir = args
        .crawl_dir
        .clone()
        .unwrap_or_else(|| paths.crawls.clone());
    let mut ctx = ToolContext::new(crawl_dir);
    ctx.settings_dir = paths.settings.clone();
    ctx.settings = settings;
    Ok(McpServer::new(tools, permissions, ctx))
}

/// Meldet Widersprüche und unbekannte Klassen in den Schaltern, bevor irgendeine Datei gelesen wird.
fn check_switches(args: &McpArgs) -> Result<(), McpError> {
    Permissions::resolve(&args.allow, args.read_only, &McpSettings::default()).map(|_| ())
}

fn log_start(server: &McpServer, args: &McpArgs, paths: &AppPaths) {
    let classes: Vec<&str> = server
        .permissions()
        .classes()
        .iter()
        .map(|c| class_name(*c))
        .collect();
    let crawl_dir = args.crawl_dir.as_ref().unwrap_or(&paths.crawls);
    if !crawl_dir.is_dir() {
        tracing::warn!(dir = %crawl_dir.display(), "Crawl-Ordner existiert nicht");
    }
    tracing::info!(
        classes = ?classes,
        tools = server.exposed_tool_names().len(),
        crawl_dir = %crawl_dir.display(),
        transport = if args.http { "HTTP" } else { "STDIO" },
        "openspider mcp gestartet"
    );
}

/// Führt den Server auf einem eigenen Thread mit eigener Laufzeit aus (so auch aus einer bestehenden
/// Laufzeit heraus aufrufbar).
fn run_blocking(server: McpServer, http: bool, port: Option<u16>) -> Result<(), McpError> {
    let worker = std::thread::Builder::new()
        .name("openspider-mcp".into())
        .spawn(move || -> Result<(), McpError> {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|e| McpError::Transport(format!("Laufzeit nicht startbar: {e}")))?;
            rt.block_on(async move {
                if http {
                    let token =
                        crate::http::TokenStore::open(os_fetch::secrets::secret_store_from_env())?;
                    let listener =
                        crate::http::HttpServer::bind(server, port.unwrap_or(0), token).await?;
                    tracing::info!(port = listener.port(), "MCP HTTP ready on loopback");
                    tokio::signal::ctrl_c()
                        .await
                        .map_err(|e| McpError::Transport(e.to_string()))?;
                    listener.stop().await;
                    Ok(())
                } else {
                    transport::serve_stdio(server).await
                }
            })
        })
        .map_err(|e| McpError::Transport(format!("Thread nicht startbar: {e}")))?;
    worker
        .join()
        .map_err(|_| McpError::Transport("Server-Thread abgestürzt".into()))?
}

/// Argumente, mit denen der Client den Server startet: `mcp` (außer beim Starter `openspider-mcp`) und die
/// angegebenen Schalter (`--crawl-dir`, `--read-only`, `--allow`).
pub fn launch_args(args: &McpArgs, with_subcommand: bool) -> Vec<String> {
    let mut out = Vec::new();
    if with_subcommand {
        out.push("mcp".to_string());
    }
    if let Some(dir) = &args.crawl_dir {
        out.push("--crawl-dir".into());
        out.push(print_config::path_string(dir));
    }
    if args.read_only {
        out.push("--read-only".into());
    }
    let allow: Vec<&str> = args
        .allow
        .iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .collect();
    if !allow.is_empty() {
        out.push("--allow".into());
        out.push(allow.join(","));
    }
    out
}

fn print_config_command(name: &str, args: &McpArgs) -> Result<(), McpError> {
    let client = Client::parse(name)
        .ok_or_else(|| McpError::Usage(format!("--print-config: unbekannter Client \"{name}\"")))?;
    let exe = print_config::current_exe_absolute()?;
    let is_starter = exe.file_stem().is_some_and(|stem| stem == "openspider-mcp");
    let text = print_config::render(
        client,
        &print_config::path_string(&exe),
        &launch_args(args, !is_starter),
    )?;
    let mut out = std::io::stdout().lock();
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| McpError::io("stdout", e))?;
    let _ = writeln!(std::io::stderr(), "{}", client.placement_hint());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Wrapper {
        #[command(flatten)]
        args: McpArgs,
    }

    fn parse(argv: &[&str]) -> McpArgs {
        Wrapper::try_parse_from(std::iter::once("t").chain(argv.iter().copied()))
            .unwrap()
            .args
    }

    // spec: AGENT-001 [M2]
    #[test]
    fn internal_http_flags_are_hidden_and_client_launches_use_stdio() {
        use clap::CommandFactory;
        let help = Wrapper::command().render_long_help().to_string();
        assert!(!help.contains("--http"));
        assert!(!help.contains("--port"));
        let args = parse(&["--http", "--port", "4567", "--allow", "read"]);
        assert_eq!(launch_args(&args, true), ["mcp", "--allow", "read"]);
    }

    // spec: AGENT-001
    #[test]
    fn allow_takes_comma_separated_and_repeated_values() {
        let a = parse(&["--allow", "crawl,write", "--allow", "cost"]);
        assert_eq!(a.allow, ["crawl", "write", "cost"]);
        assert!(!a.read_only);
        assert!(parse(&["--read-only"]).read_only);
    }

    // spec: AGENT-001
    #[test]
    fn print_config_accepts_only_the_four_clients() {
        for c in ["claude-desktop", "claude-code", "codex", "cursor"] {
            assert_eq!(
                parse(&["--print-config", c]).print_config.as_deref(),
                Some(c)
            );
        }
        assert!(Wrapper::try_parse_from(["t", "--print-config", "vscode"]).is_err());
    }

    // spec: AGENT-001
    #[test]
    fn launch_args_carry_the_given_switches() {
        assert_eq!(launch_args(&McpArgs::default(), true), ["mcp"]);
        assert!(launch_args(&McpArgs::default(), false).is_empty());
        let a = parse(&[
            "--crawl-dir",
            "/tmp/c",
            "--allow",
            "crawl,write",
            "--read-only",
        ]);
        assert_eq!(
            launch_args(&a, true),
            [
                "mcp",
                "--crawl-dir",
                "/tmp/c",
                "--read-only",
                "--allow",
                "crawl,write"
            ]
        );
    }

    // spec: AGENT-001
    #[test]
    fn run_reports_the_contradiction_with_exit_code_two() {
        let code = run(McpArgs {
            allow: vec!["crawl".into()],
            read_only: true,
            ..McpArgs::default()
        });
        assert_eq!(code, 2);
    }
}
