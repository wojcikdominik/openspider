//! Einstieg: Optionen lesen, Trockenlauf (Standard) oder Live-Lauf.

use probe::cli::{parse_args, Mode, LIVE_ENV, USAGE};
use probe::plan::Endpoints;
use probe::runner::{print_plan, Probe, RunConfig};
use probe::secret::KeyringSource;
use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = std::env::var(LIVE_ENV).ok();
    let options = match parse_args(&args, env.as_deref()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Fehler: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if options.help {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let mut stdout = std::io::stdout();
    match options.mode {
        Mode::DryRun { .. } => match print_plan(&options, &Endpoints::default(), &mut stdout) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Ausgabe fehlgeschlagen: {e}");
                ExitCode::FAILURE
            }
        },
        Mode::Rebuild => match probe::runner::rebuild_results(&probe::default_results_dir()) {
            Ok(n) => {
                println!("{n} Anbieterdateien bereinigt, summary.json neu zusammengesetzt");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Ergebnisse nicht bearbeitbar: {e}");
                ExitCode::FAILURE
            }
        },
        Mode::Live => {
            let config = RunConfig::new(options, probe::default_results_dir());
            let probe = match Probe::new(&config, &mut stdout) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            };
            match probe.run(&KeyringSource).await {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("Ergebnisse nicht schreibbar: {e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
