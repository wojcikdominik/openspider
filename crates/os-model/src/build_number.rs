//! Build-Nummer (UI-127, OUT-181, Nutzerentscheidung 09.11.2026): Anzahl der Commits bis zum gebauten Stand
//! (`git rev-list --count HEAD`), angezeigt als „1.0.0-beta.3 (Build 2578)“. Die Version selbst bleibt SemVer; der
//! Updater vergleicht nur sie.
//!
//! Quelle beim Bau: die Umgebungsvariable [`ENV`] (Release-Workflow, für alle Plattformen dieselbe Nummer), sonst bei
//! Release-Builds Git, sofern der Checkout vollständig ist und zu diesem Workspace gehört. Flacher Checkout, fremdes
//! Repository oder kein Git (Quell-Tarball): keine Build-Nummer statt einer falschen Zahl. Debug-Builds fragen Git
//! nicht, damit nicht jeder Commit den ganzen Workspace neu baut.
//!
//! Diese Datei wird auch von den Build-Skripten von `os-model` und `os-cli` per `#[path]` eingebunden; sie macht
//! keine Ein-/Ausgabe.

/// Umgebungsvariable mit der Build-Nummer des Release-Workflows.
pub const ENV: &str = "OPENSPIDER_BUILD_NUMBER";

/// Eingebettete Build-Nummer dieses Builds; `None` ohne Git, im flachen Checkout und in Debug-Builds ohne [`ENV`].
pub const BUILD_NUMBER: Option<&str> = option_env!("OS_BUILD_NUMBER");

/// Prüft eine Build-Nummer: positive ganze Zahl ohne führende Null.
pub fn parse(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('0') || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok().filter(|n| *n > 0)
}

/// Was die Build-Skripte über Git herausfinden; `None`-Felder, wenn Git fehlt oder der Aufruf scheitert.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GitFacts {
    /// Ausgabe von `git rev-list --count HEAD`.
    pub count: Option<String>,
    /// Ausgabe von `git rev-parse --is-shallow-repository` (`true`/`false`).
    pub shallow: Option<String>,
    /// Das Git-Wurzelverzeichnis ist das Workspace-Verzeichnis (nicht ein umgebendes fremdes Repository).
    pub own_repository: bool,
}

/// Build-Nummer aus Git: nur bei vollständigem Checkout des eigenen Repositorys.
pub fn from_git(facts: &GitFacts) -> Option<u64> {
    if !facts.own_repository || facts.shallow.as_deref().map(str::trim) != Some("false") {
        return None;
    }
    parse(facts.count.as_deref()?)
}

/// Entscheidung des Build-Skripts. Eine gesetzte, aber ungültige [`ENV`] ist ein Fehler (der Release-Lauf wird rot);
/// eine leere gilt als nicht gesetzt.
pub fn resolve(
    env: Option<&str>,
    release_profile: bool,
    git: impl FnOnce() -> GitFacts,
) -> Result<Option<u64>, String> {
    match env.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => parse(value).map(Some).ok_or_else(|| {
            format!("{ENV}={value:?} ist keine gültige Build-Nummer (positive ganze Zahl)")
        }),
        None if release_profile => Ok(from_git(&git())),
        None => Ok(None),
    }
}

/// Anzeigeform „<Version> (Build <Nummer>)“, ohne Nummer nur die Version.
pub fn display(version: &str, build: Option<&str>) -> String {
    match build {
        Some(build) => format!("{version} (Build {build})"),
        None => version.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(count: &str, shallow: &str, own: bool) -> GitFacts {
        GitFacts {
            count: Some(count.into()),
            shallow: Some(shallow.into()),
            own_repository: own,
        }
    }

    // spec: UI-127
    // spec: OUT-181
    #[test]
    fn environment_wins_and_must_be_a_positive_number() {
        fn unused() -> GitFacts {
            panic!("Git wird mit gesetzter Variable nicht gefragt")
        }
        assert_eq!(resolve(Some("2578"), true, unused), Ok(Some(2578)));
        assert_eq!(resolve(Some(" 2578\n"), false, unused), Ok(Some(2578)));
        for bad in ["abc", "-1", "0", "0123", "25 78", "2578a"] {
            assert!(resolve(Some(bad), true, unused).is_err(), "{bad}");
        }
        // An empty variable (unset output of a job) counts as not set.
        assert_eq!(resolve(Some(""), false, unused), Ok(None));
    }

    // spec: UI-127
    // spec: OUT-181
    #[test]
    fn git_only_for_release_builds_of_a_complete_own_checkout() {
        assert_eq!(
            resolve(None, true, || git("2578\n", "false\n", true)),
            Ok(Some(2578))
        );
        // Debug builds do not ask Git (no workspace rebuild per commit).
        fn debug_git() -> GitFacts {
            panic!("Git im Debug-Build gefragt")
        }
        assert_eq!(resolve(None, false, debug_git), Ok(None));
        // Without Git (source tarball): no number instead of a wrong one.
        assert_eq!(resolve(None, true, GitFacts::default), Ok(None));
        // Shallow checkout counts wrong; a surrounding foreign repository is not ours.
        assert_eq!(resolve(None, true, || git("1", "true", true)), Ok(None));
        assert_eq!(
            resolve(None, true, || git("9999", "false", false)),
            Ok(None)
        );
        assert_eq!(resolve(None, true, || git("", "false", true)), Ok(None));
        assert_eq!(
            resolve(None, true, || GitFacts {
                count: Some("12".into()),
                shallow: None,
                own_repository: true
            }),
            Ok(None)
        );
    }

    // spec: UI-127
    #[test]
    fn display_form() {
        assert_eq!(
            display("1.0.0-beta.3", Some("2578")),
            "1.0.0-beta.3 (Build 2578)"
        );
        assert_eq!(display("1.0.0-beta.3", None), "1.0.0-beta.3");
    }
}
