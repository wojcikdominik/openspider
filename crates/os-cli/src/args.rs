//! Kommandozeilen-Syntax (OUT-180 ff.): zum Referenzprodukt kompatible Flags plus markierte OS-Erweiterungen.
//!
//! Optionen, die die Referenz kennt, die diese Version aber noch nicht umsetzt (Google-Anbindung,
//! Vergleiche, API-Integrationen …), sind trotzdem deklariert: Sie erscheinen in `--help`, und ein Aufruf
//! endet mit Exit-Code 2 und einer klaren Meldung statt mit „unbekannte Option“.

use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};

/// Version laut `--version`: `<semver> (<git-hash>, <os>-<arch>)` (vom Build-Skript).
pub const VERSION: &str = env!("OS_CLI_VERSION");

/// OpenSpider – Open-Source-SEO-Crawler.
#[derive(Debug, Parser)]
#[command(
    name = "openspider",
    version = VERSION,
    about,
    disable_help_flag = true,
    next_display_order = None
)]
pub struct Cli {
    /// Unterbefehl (ohne Unterbefehl gelten die Optionen des Headless-Betriebs).
    #[command(subcommand)]
    pub command: Option<Sub>,

    /// Optionen des Headless-Betriebs.
    #[command(flatten)]
    pub run: RunArgs,
}

/// Unterbefehle von `openspider`.
#[derive(Debug, Subcommand)]
pub enum Sub {
    /// Startet den MCP-Server (STDIO) für KI-Agenten.
    Mcp(os_mcp::cli::McpArgs),
}

/// Optionen des Headless-Betriebs.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct RunArgs {
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI")]
    pub use_ai: bool,
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI")]
    pub use_embeddings: bool,
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI")]
    pub ai_tasks: Option<String>,
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI")]
    pub ai_questions: Option<String>,
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI")]
    pub ai_consent: bool,
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI")]
    pub ai_sample: Option<u16>,
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI")]
    pub api_budget: Option<String>,
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI")]
    pub ai_budget: Option<String>,
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI")]
    pub export_query: Option<String>,
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI", value_parser = ["auto", "on", "off"])]
    pub ai_batch: Option<String>,
    /// STUB(M4-C01): KI-Option, Ausführung ab M4-C01.
    #[arg(long, help_heading = "KI")]
    pub ai_reuse_cache: Option<PathBuf>,

    /// Prüft manuell auf Updates (STUB M3-U01).
    #[arg(long, conflicts_with_all = ["run_task", "uninstall_cleanup", "crawl", "crawl_list", "load_crawl"])]
    pub check_update: bool,
    /// Installer-Hook: entfernt OS-Aufgabenregistrierungen, erhält Benutzerdaten.
    #[arg(long, hide = true, conflicts_with_all = ["run_task", "crawl", "crawl_list", "load_crawl"])]
    pub uninstall_cleanup: bool,

    /// Protokollformat (OUT-240).
    #[arg(long, value_parser = ["text", "json"])]
    pub log_format: Option<String>,
    /// Protokollstufe (OUT-241).
    #[arg(long, value_parser = ["error", "warn", "info", "debug", "trace"])]
    pub log_level: Option<String>,
    /// Diagnoseordner (OUT-244).
    #[arg(long, value_name = "ORDNER")]
    pub diagnostics: Option<PathBuf>,

    /// Zeigt die Hilfe; `--help export-tabs|bulk-export|save-report|export-custom-summary|set` listet die
    /// gültigen Namen der Kategorie (mit `--json` maschinenlesbar).
    #[arg(
        short = 'h',
        long,
        value_name = "THEMA",
        num_args = 0..=1,
        help_heading = "Allgemein"
    )]
    pub help: Option<Option<String>>,

    // ---- Aufgaben -----------------------------------------------------------------------------
    /// Führt eine geplante Aufgabe aus (Einstieg des OS-Planers, OUT-209): lädt `schedules/<ID>.ostask`,
    /// crawlt, exportiert und schreibt Laufhistorie und Lauf-Log.
    #[arg(long, value_name = "ID", help_heading = "Aufgaben")]
    pub run_task: Option<String>,

    /// [OS-Erweiterung] Trägt alle OpenSpider-Aufgaben beim Betriebssystem-Planer aus (Deinstallation,
    /// OUT-166); die Aufgabendateien bleiben erhalten.
    #[arg(long, help_heading = "Aufgaben")]
    pub unregister_tasks: bool,

    /// Installiert die stabile Chromium-Komponente für Headless-Rendering (OUT-207).
    #[arg(long, help_heading = "Konfiguration")]
    pub install_browser: bool,

    // ---- Quelle -------------------------------------------------------------------------------
    /// Crawlt eine Website ab der URL (Spider-Modus); ohne Schema wird `http://` ergänzt.
    #[arg(long, value_name = "URL", help_heading = "Quelle")]
    pub crawl: Option<String>,

    /// Crawlt eine URL-Liste: Datei (.txt, .csv, .xlsx), XML-Sitemap (URL oder Datei, auch Index); mehrfach
    /// angebbar.
    #[arg(long, value_name = "QUELLE", help_heading = "Quelle")]
    pub crawl_list: Vec<String>,

    /// Öffnet einen gespeicherten Crawl (Pfad zur `.ospider`-Datei oder Crawl-ID) für Exporte ohne
    /// erneutes Crawlen.
    #[arg(long, value_name = "DATEI|ID", help_heading = "Quelle")]
    pub load_crawl: Option<String>,

    /// Listet alle Crawls der Crawl-Verwaltung (mit `--json` als JSON-Array, `--project-name` filtert).
    #[arg(long, help_heading = "Quelle")]
    pub list_crawls: bool,

    /// [noch nicht unterstützt] Crawlt eine Google-Tabelle: `<Konto> <Tabellen-URL>`.
    #[arg(long, num_args = 0.., value_name = "KONTO URL", help_heading = "Quelle")]
    pub crawl_google_sheet: Option<Vec<String>>,

    /// Verbundenes Google-Konto für Drive und Sheets.
    #[arg(long, value_name = "KONTO", help_heading = "Quelle")]
    pub google_drive_account: Option<String>,

    // ---- Konfiguration ------------------------------------------------------------------------
    /// Gespeicherte Crawl-Konfiguration (`.osconfig`).
    #[arg(long, value_name = "DATEI", help_heading = "Konfiguration")]
    pub config: Option<PathBuf>,

    /// Verschlüsseltes Authentifizierungsprofil (.osauth); Passphrase über OPENSPIDER_AUTH_PASSPHRASE.
    #[arg(long, value_name = "DATEI", help_heading = "Konfiguration")]
    pub auth_config: Option<PathBuf>,

    /// Wirkungslos: das Binary läuft immer ohne Oberfläche (Kompatibilität).
    #[arg(long, help_heading = "Konfiguration")]
    pub headless: bool,

    /// Projektname in der Crawl-Verwaltung (Standard: `Default`).
    #[arg(long, value_name = "NAME", help_heading = "Konfiguration")]
    pub project_name: Option<String>,

    /// Name des Crawls in der Crawl-Verwaltung (Standard: Host der Start-URL bzw. Name der Listendatei).
    #[arg(long, value_name = "NAME", help_heading = "Konfiguration")]
    pub task_name: Option<String>,

    // ---- Vergleich ----------------------------------------------------------------------------
    /// Vergleicht zwei gespeicherte Crawls (Current nach Crawl-Datum).
    #[arg(long, num_args = 2, value_name = "ID1 ID2", help_heading = "Vergleich")]
    pub crawl_comparison: Option<Vec<String>>,

    /// Vergleicht mit dem vorherigen erfolgreichen Crawl des Projekts.
    #[arg(long, num_args = 0..=1, value_name = "true|false", help_heading = "Vergleich")]
    pub project_crawl_comparison: Option<Option<String>>,

    /// Alias für `--project-crawl-comparison true`.
    #[arg(long, help_heading = "Vergleich")]
    pub compare_with_previous: bool,

    // ---- APIs ---------------------------------------------------------------------------------
    /// Google Analytics 4.
    #[arg(
        long,
        num_args = 4,
        value_name = "KONTO ACCOUNT PROPERTY STROM",
        help_heading = "APIs"
    )]
    pub use_google_analytics_4: Option<Vec<String>>,

    /// Google Search Console.
    #[arg(
        long,
        num_args = 2,
        value_name = "KONTO WEBSITE",
        help_heading = "APIs"
    )]
    pub use_google_search_console: Option<Vec<String>>,

    /// Universal Analytics wird nicht mehr unterstützt.
    #[arg(long, num_args = 0.., help_heading = "APIs")]
    pub use_google_analytics: Option<Vec<String>>,

    /// PageSpeed Insights.
    #[arg(long, help_heading = "APIs")]
    pub use_pagespeed: bool,

    /// Ahrefs.
    #[arg(long, help_heading = "APIs")]
    pub use_ahrefs: bool,

    /// Majestic.
    #[arg(long, help_heading = "APIs")]
    pub use_majestic: bool,

    /// Moz.
    #[arg(long, help_heading = "APIs")]
    pub use_mozscape: bool,

    /// Angeforderte Integrationen werden verpflichtend.
    #[arg(long, help_heading = "APIs")]
    pub require_apis: bool,

    // ---- Exporte ------------------------------------------------------------------------------
    /// Beschränkt Exporte auf ein gespeichertes Segment; `all` exportiert zusätzlich jedes Segment einzeln.
    #[arg(long, value_name = "NAME|all", help_heading = "Exporte")]
    pub segment: Option<String>,

    /// Tab-Exporte `"Tab:Filter,…"` (z. B. `"Internal:All,Response Codes:Client Error (4xx)"`); Kommas im
    /// Namen mit `\,` maskieren. Im Vergleich wählt ein dritter Teil den Unterfilter: `"Internal:HTML:Missing"`
    /// (Added, New, Removed, Missing; OUT-151).
    #[arg(long, value_name = "TAB:FILTER,…", help_heading = "Exporte")]
    pub export_tabs: Vec<String>,

    /// Bulk-Exporte `"Menü:Eintrag,…"` (z. B. `"Links:All Inlinks"`).
    #[arg(long, value_name = "MENÜ:EINTRAG,…", help_heading = "Exporte")]
    pub bulk_export: Vec<String>,

    /// Reports `"Report,…"` (z. B. `"Crawl Overview,Redirects:Redirect Chains"`).
    #[arg(long, value_name = "REPORT,…", help_heading = "Exporte")]
    pub save_report: Vec<String>,

    /// Herkunftspfad der gewählten URLs als eine Datei in Auswahlreihenfolge exportieren.
    #[arg(long, value_name = "URL[,URL…]", help_heading = "Exporte")]
    pub export_crawl_path: Vec<String>,

    /// Format der Exporte: csv (Standard), xls, xlsx oder gsheet (verbundenes Google-Konto).
    #[arg(long, value_name = "FORMAT", help_heading = "Exporte")]
    pub export_format: Option<String>,

    /// Legt den Crawl zusätzlich als `crawl.ospider` im Ausgabeordner ab.
    #[arg(long, help_heading = "Exporte")]
    pub save_crawl: bool,

    /// Ausgabeordner (Standard: aktuelles Arbeitsverzeichnis); wird bei Bedarf angelegt.
    #[arg(long, value_name = "ORDNER", help_heading = "Exporte")]
    pub output_folder: Option<PathBuf>,

    /// Ersetzt gleichnamige Dateien im Ausgabeordner (sonst muss der Ordner leer sein).
    #[arg(long, help_heading = "Exporte")]
    pub overwrite: bool,

    /// Legt im Ausgabeordner einen Unterordner `YYYY-MM-DD_HH-MM-SS` an.
    #[arg(long, help_heading = "Exporte")]
    pub timestamped_output: bool,

    /// Listet alle Namen für `--export-tabs` und `--bulk-export` (mit `--json` maschinenlesbar).
    #[arg(long, help_heading = "Exporte")]
    pub list_exports: bool,

    /// Listet alle Namen für `--save-report` (mit `--json` maschinenlesbar).
    #[arg(long, help_heading = "Exporte")]
    pub list_reports: bool,

    /// Maschinenlesbare Ausgabe für `--list-crawls`, `--list-exports`, `--list-reports` und `--help <thema>`.
    #[arg(long, help_heading = "Exporte")]
    pub json: bool,

    /// XML-Sitemap mit den Einstellungen des Crawl-Profils erzeugen.
    #[arg(long, help_heading = "Exporte")]
    pub create_sitemap: bool,

    /// Bilder-Sitemap mit den Einstellungen des Crawl-Profils erzeugen.
    #[arg(long, help_heading = "Exporte")]
    pub create_images_sitemap: bool,

    /// [noch nicht unterstützt] Abschlussmail senden.
    #[arg(long, value_name = "ADRESSE,…", help_heading = "Exporte")]
    pub email_on_complete: Option<String>,

    /// [noch nicht unterstützt] Looker-Studio-Export.
    #[arg(long, value_name = "METRIK,…", help_heading = "Exporte")]
    pub export_custom_summary: Option<String>,

    /// Multi-Export-Vorlage (.ospreset): Name im Profilordner oder Dateipfad.
    #[arg(long, value_name = "NAME|DATEI", help_heading = "Exporte")]
    pub export_preset: Option<String>,

    /// Tabellen als Blätter einer gemeinsamen Arbeitsmappe exportieren.
    #[arg(long, help_heading = "Exporte")]
    pub single_workbook: bool,

    /// Exporte ohne Datenzeilen überspringen.
    #[arg(long, help_heading = "Exporte")]
    pub skip_empty_exports: bool,

    /// Spaltenköpfe der englischen UI oder kanonische englische Namen.
    #[arg(long, value_parser = ["ui", "canonical"], help_heading = "Exporte")]
    pub column_headers: Option<String>,

    /// Fortschrittsformat auf stderr (Alias: --json-progress).
    #[arg(long, value_parser = ["text", "json"], help_heading = "Allgemein")]
    pub progress: Option<String>,

    /// Nur Fehler auf stderr ausgeben.
    #[arg(long, help_heading = "Allgemein")]
    pub quiet: bool,

    /// API-Basis-URLs aus der Umgebung auch im Release freigeben (INTEG-019).
    #[arg(long, help_heading = "APIs")]
    pub allow_api_override: bool,

    // ---- OS-Erweiterungen ---------------------------------------------------------------------
    /// [OS-Erweiterung] Host-Auflösung `host=ip[:port]` bzw. `*.domain=ip[:port]` statt DNS; mehrfach
    /// angebbar.
    #[arg(long, value_name = "HOST=IP[:PORT]", help_heading = "OS-Erweiterungen")]
    pub resolve: Vec<String>,

    /// [OS-Erweiterung] Überschreibt einen Konfigurationswert (`schlüssel=wert`), wirkt nach `--config`;
    /// mehrfach angebbar. Die setzbaren Schlüssel: `--help set`.
    #[arg(long, value_name = "SCHLÜSSEL=WERT", help_heading = "OS-Erweiterungen")]
    pub set: Vec<String>,

    /// [OS-Erweiterung] Kurzform für `--set limits.max_urls=<n>`.
    #[arg(long, value_name = "N", help_heading = "OS-Erweiterungen")]
    pub max_urls: Option<String>,

    /// [OS-Erweiterung] Setzt mit `--load-crawl` einen pausierten oder unterbrochenen Crawl fort.
    #[arg(long, help_heading = "OS-Erweiterungen")]
    pub resume: bool,

    /// [OS-Erweiterung] Kurzform für `--crawl-list <Sitemap-URL>`; mehrfach angebbar.
    #[arg(long, value_name = "URL", help_heading = "OS-Erweiterungen")]
    pub crawl_sitemap: Vec<String>,

    /// [OS-Erweiterung] Verwirft den Crawl nach dem Ende (außer mit `--save-crawl`).
    #[arg(long, help_heading = "OS-Erweiterungen")]
    pub no_store: bool,

    /// [OS-Erweiterung] Schreibt höchstens einmal je Sekunde eine JSON-Fortschrittszeile nach stderr.
    #[arg(long, help_heading = "OS-Erweiterungen")]
    pub json_progress: bool,

    /// [OS-Erweiterung] Start-URL nicht erreichbar → Exit-Code 3 statt Warnung.
    #[arg(long, help_heading = "OS-Erweiterungen")]
    pub fail_on_start_error: bool,

    /// [OS-Erweiterung] Warnungen führen zu Exit-Code 4.
    #[arg(long, help_heading = "OS-Erweiterungen")]
    pub strict: bool,
}

/// Namen aller langen Optionen (für Vorschläge bei Tippfehlern).
pub fn long_option_names() -> Vec<String> {
    let mut names: Vec<String> = Cli::command()
        .get_arguments()
        .filter_map(|a| a.get_long().map(|l| format!("--{l}")))
        .collect();
    names.push("--version".to_string());
    names.sort();
    names.dedup();
    names
}

impl RunArgs {
    /// Die noch nicht unterstützten Optionen, die der Aufruf enthält (Namen mit `--`).
    pub fn unsupported(&self) -> Vec<&'static str> {
        let checks: [(bool, &'static str); 14] = [
            (self.use_ai, "--use-ai"),
            (self.use_embeddings, "--use-embeddings"),
            (self.ai_tasks.is_some(), "--ai-tasks"),
            (self.ai_questions.is_some(), "--ai-questions"),
            (self.ai_consent, "--ai-consent"),
            (self.ai_sample.is_some(), "--ai-sample"),
            (self.api_budget.is_some(), "--api-budget"),
            (self.ai_budget.is_some(), "--ai-budget"),
            (self.export_query.is_some(), "--export-query"),
            (self.ai_batch.is_some(), "--ai-batch"),
            (self.ai_reuse_cache.is_some(), "--ai-reuse-cache"),
            (self.crawl_google_sheet.is_some(), "--crawl-google-sheet"),
            (self.email_on_complete.is_some(), "--email-on-complete"),
            (
                self.export_custom_summary.is_some(),
                "--export-custom-summary",
            ),
        ];
        checks
            .into_iter()
            .filter(|(present, _)| *present)
            .map(|(_, name)| name)
            .collect()
    }
}

/// Windows-Fallstrick: `--output-folder "C:\out\"` – der Backslash vor dem schließenden Anführungszeichen
/// maskiert es, und der Pfad schluckt die Folgeargumente (`C:\out" --crawl x`). Erkennt das, schneidet den
/// Pfad vor dem Anführungszeichen ab und zerlegt den Rest wieder in Argumente (OUT-190).
///
/// Liefert die korrigierten Argumente und die Namen der betroffenen Optionen (für den Hinweis).
pub fn fix_trailing_backslash_quote(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut fixed = Vec::with_capacity(args.len());
    let mut touched = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let is_path_option = matches!(
            arg.as_str(),
            "--output-folder" | "--config" | "--auth-config" | "--load-crawl"
        );
        fixed.push(arg.clone());
        if !is_path_option {
            continue;
        }
        let Some(value) = iter.next() else {
            break;
        };
        match value.split_once('"') {
            Some((path, rest)) => {
                touched.push(arg);
                fixed.push(path.to_string());
                fixed.extend(split_rest(rest));
            }
            None => fixed.push(value),
        }
    }
    (fixed, touched)
}

/// Zerlegt den verschluckten Rest einer Kommandozeile an Leerraum; Anführungszeichen umschließen Wörter.
fn split_rest(rest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    for c in rest.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    out.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // spec: OUT-190
    #[test]
    fn trailing_backslash_before_a_quote_is_repaired() {
        // Die Shell liefert `"C:\out\" --crawl x --overwrite` als EIN Argument `C:\out" --crawl x --overwrite`.
        let args = s(&["--output-folder", "C:\\out\" --crawl x --overwrite"]);
        let (fixed, touched) = fix_trailing_backslash_quote(args);
        assert_eq!(
            fixed,
            s(&["--output-folder", "C:\\out", "--crawl", "x", "--overwrite"])
        );
        assert_eq!(touched, s(&["--output-folder"]));
    }

    // spec: OUT-190
    #[test]
    fn well_formed_paths_stay_untouched() {
        let args = s(&[
            "--output-folder",
            "C:\\Users\\Jürgen\\SEO Reports",
            "--crawl",
            "x",
        ]);
        let (fixed, touched) = fix_trailing_backslash_quote(args.clone());
        assert_eq!(fixed, args);
        assert!(touched.is_empty());
    }

    // spec: OUT-190
    #[test]
    fn quoted_words_in_the_swallowed_rest_stay_together() {
        let args = s(&[
            "--output-folder",
            "D:\\a\" --export-tabs \"Internal:All, Page Titles:All\"",
        ]);
        let (fixed, _) = fix_trailing_backslash_quote(args);
        assert_eq!(
            fixed,
            s(&[
                "--output-folder",
                "D:\\a",
                "--export-tabs",
                "Internal:All, Page Titles:All"
            ])
        );
    }

    #[test]
    fn long_option_names_contain_the_documented_flags() {
        let names = long_option_names();
        for flag in [
            "--crawl",
            "--crawl-list",
            "--load-crawl",
            "--list-crawls",
            "--config",
            "--headless",
            "--save-crawl",
            "--output-folder",
            "--overwrite",
            "--timestamped-output",
            "--project-name",
            "--task-name",
            "--export-tabs",
            "--bulk-export",
            "--save-report",
            "--export-format",
            "--resolve",
            "--set",
            "--max-urls",
            "--resume",
            "--crawl-sitemap",
            "--no-store",
            "--json-progress",
            "--fail-on-start-error",
            "--strict",
            "--run-task",
            "--unregister-tasks",
        ] {
            assert!(names.contains(&flag.to_string()), "{flag} fehlt");
        }
    }
}

#[path = "ai.rs"]
pub mod ai;
