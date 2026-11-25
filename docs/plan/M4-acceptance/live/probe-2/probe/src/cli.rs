//! Optionen und Modusentscheidung. Echte Aufrufe nur mit `OPENSPIDER_LIVE_API_TESTS=1` **und** `--live`.

use crate::plan::{Provider, TOTAL_CAP_USD};

/// Umgebungsvariable, die Live-Aufrufe freigibt (08 „Gemeinsame Testinfrastruktur“).
pub const LIVE_ENV: &str = "OPENSPIDER_LIVE_API_TESTS";

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    /// Nur Plan ausgeben; kein Netzwerk, kein Schlüsselbund. `note` erklärt, warum `--live` nicht gilt.
    DryRun {
        note: Option<String>,
    },
    Live,
    /// Ohne Netz und Schlüsselbund: vorhandene Ergebnisse bereinigen und `summary.json` neu zusammensetzen.
    Rebuild,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub mode: Mode,
    pub only: Option<Provider>,
    /// Gesamtgrenze in USD (höchstens [`TOTAL_CAP_USD`]).
    pub max_total_usd: f64,
    /// Nur kostenlose Abfragen (Modelllisten).
    pub models_only: bool,
    pub help: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mode: Mode::DryRun { note: None },
            only: None,
            max_total_usd: TOTAL_CAP_USD,
            models_only: false,
            help: false,
        }
    }
}

impl Options {
    pub fn providers(&self) -> Vec<Provider> {
        match self.only {
            Some(p) => vec![p],
            None => Provider::ALL.to_vec(),
        }
    }
}

pub const USAGE: &str = "\
openspider-live-probe-2 – zweite Live-Probe M4-Q00b (V-A6 Reasoning-Parameter, V-A8 Speicherung)

  --dry-run                 Plan ausgeben, ohne Netzwerk und ohne Schlüsselbund (Standard)
  --live                    echte Aufrufe; nur zusammen mit OPENSPIDER_LIVE_API_TESTS=1
  --only <anbieter>         anthropic | openai | openrouter
  --models-only             nur kostenlose Abfragen (Modelllisten)
  --rebuild-results         ohne Netz: results/*.json bereinigen, summary.json aus allen Anbieterdateien neu bilden
  --max-total-usd <betrag>  Gesamtgrenze senken (höchstens 1, Standard 1)
  --help                    diese Hilfe
";

/// Liest die Optionen. `live_env` ist der Wert von [`LIVE_ENV`] (falls gesetzt).
pub fn parse_args(args: &[String], live_env: Option<&str>) -> Result<Options, String> {
    let mut o = Options::default();
    let mut live = false;
    let mut rebuild = false;
    let mut it = args.iter();
    let value = |it: &mut std::slice::Iter<'_, String>, name: &str| {
        it.next()
            .cloned()
            .ok_or_else(|| format!("{name} braucht einen Wert"))
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--dry-run" => live = false,
            "--live" => live = true,
            "--models-only" => o.models_only = true,
            "--rebuild-results" => rebuild = true,
            "--help" | "-h" => o.help = true,
            "--only" => {
                let v = value(&mut it, "--only")?;
                o.only =
                    Some(Provider::parse(&v).ok_or_else(|| format!("unbekannter Anbieter: {v}"))?);
            }
            "--max-total-usd" => {
                let v = value(&mut it, "--max-total-usd")?;
                let n: f64 = v.parse().map_err(|_| format!("kein Betrag: {v}"))?;
                if n.is_nan() || n <= 0.0 || n > TOTAL_CAP_USD {
                    return Err(format!(
                        "--max-total-usd muss größer 0 und höchstens {TOTAL_CAP_USD} sein (nur nach unten änderbar)"
                    ));
                }
                o.max_total_usd = n;
            }
            other => return Err(format!("unbekannte Option: {other}")),
        }
    }
    o.mode = match (live, live_env == Some("1")) {
        _ if rebuild => Mode::Rebuild,
        (true, true) => Mode::Live,
        (true, false) => Mode::DryRun {
            note: Some(format!(
                "--live ignoriert: {LIVE_ENV}=1 ist nicht gesetzt; Trockenlauf"
            )),
        },
        (false, _) => Mode::DryRun { note: None },
    };
    Ok(o)
}
