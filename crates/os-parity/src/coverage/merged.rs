//! Gemergte M2-Pakete aus der Merge-Historie der Hauptlinie (B4-M2CLOSE, Folgepunkt aus B3-COV).
//!
//! Eingabe ist `git log <hauptlinie> --format='%H%x09%s'` (Commit, Tabulator, Titel). Erkannt werden die
//! Titelformen, mit denen M2-Pakete auf die Hauptlinie kamen:
//!
//! 1. `m2: Merge [m2/][M2-]<Zweig> …` (C1, B3-COV),
//! 2. `m2: Integration Runde <N> – merge m2/<Zweig>` bzw. `m2: Integration Runde <N> – <Zweig> (…)` (Runden 6/7),
//! 3. `merge: m2/<Zweig> (…) in Integrationsrunde <N>` (Runde 8).
//!
//! `<Zweig>` ist eine Folge von `[A-Za-z0-9]+`-Teilen mit Bindestrich; es zählt der längste Bindestrich-Präfix,
//! der eine Paket-ID ist (`fix-live` → `M2-FIX-LIVE`, `ci-e2e-limit` → `M2-CI-E2E`). Andere Zweige (`int-16`,
//! `main`, `N00-SPEC-TAGS` …) werden nur aufgelistet. Pakete, die ohne eigenen Merge-Titel über einen
//! Integrationszweig kamen (N06/Q02 über `base-4r`, N15F mit N20, FIXE-PORT als Cherry-Pick in int-23), nennen in
//! `M2-packages.json` ihren Beleg im Feld `merged_via` (Commit und Titel); sie gelten als gemergt, wenn genau dieser
//! Commit mit genau diesem Titel in der Historie steht.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Beleg eines Pakets ohne eigenen Merge-Titel (`merged_via` in `M2-packages.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct MergedVia {
    /// Commit-Hash auf der Hauptlinie (mindestens 7 Zeichen, Präfix genügt).
    pub commit: String,
    /// Titel dieses Commits, zur Gegenprobe und für Menschen.
    pub subject: String,
}

/// Ergebnis der Erkennung.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Detection {
    /// Paket-ID → Beleg (Merge-Titel bzw. `merged_via <commit>: <titel>`).
    pub merged: BTreeMap<String, String>,
    /// Zweig ohne Planpaket → erster Merge-Titel.
    pub other: BTreeMap<String, String>,
    /// `merged_via`-Belege, die nicht in der Historie stehen (Paket-ID, Grund).
    pub missing_via: Vec<(String, String)>,
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        s.get(prefix.len()..)
    } else {
        None
    }
}

/// Zweigname am Anfang von `rest` (nach optionalem `m2/` und `M2-`).
fn branch(rest: &str) -> Option<&str> {
    let rest = strip_prefix_ci(rest, "m2/").unwrap_or(rest);
    let rest = strip_prefix_ci(rest, "M2-").unwrap_or(rest);
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .unwrap_or(rest.len());
    let token = rest[..end].trim_end_matches('-');
    (!token.is_empty() && !token.starts_with('-')).then_some(token)
}

/// Zweig eines Merge-Titels in einer der drei Formen, sonst `None`.
pub fn merged_branch(subject: &str) -> Option<&str> {
    if let Some(rest) = strip_prefix_ci(subject, "m2: Merge ") {
        return branch(rest);
    }
    if let Some(rest) = strip_prefix_ci(subject, "m2: Integration Runde ") {
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        let rest = rest
            .strip_prefix(" – ")
            .or_else(|| rest.strip_prefix(" - "))?;
        let rest = strip_prefix_ci(rest, "merge ").unwrap_or(rest);
        return branch(rest);
    }
    if let Some(rest) = strip_prefix_ci(subject, "merge: m2/") {
        if subject.contains(" in Integrationsrunde ") {
            return branch(rest);
        }
    }
    None
}

/// Erkennt gemergte Pakete. `packages` sind (Paket-ID, `merged_via`) in Planreihenfolge.
pub fn detect<'a>(
    log: &str,
    packages: impl IntoIterator<Item = (&'a str, Option<&'a MergedVia>)>,
) -> Detection {
    let packages: Vec<_> = packages.into_iter().collect();
    let mut ids: BTreeMap<String, &str> = BTreeMap::new();
    for (id, _) in &packages {
        ids.insert(id.to_ascii_uppercase(), id);
        ids.insert(id.trim_start_matches("M2-").to_ascii_uppercase(), id);
    }
    let lines: Vec<(&str, &str)> = log
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(c, s)| (c.trim(), s.trim()))
        .collect();
    let mut out = Detection::default();
    // Es zählt der erste Treffer in Logreihenfolge (`git log`: neueste zuerst, wie in B3-COV).
    for (_, subject) in &lines {
        let Some(token) = merged_branch(subject) else {
            continue;
        };
        let parts: Vec<_> = token.split('-').collect();
        let id = (1..=parts.len())
            .rev()
            .find_map(|n| ids.get(&parts[..n].join("-").to_ascii_uppercase()));
        match id {
            Some(id) => {
                out.merged
                    .entry((*id).to_string())
                    .or_insert_with(|| (*subject).to_string());
            }
            None => {
                out.other
                    .entry(token.to_string())
                    .or_insert_with(|| (*subject).to_string());
            }
        }
    }
    for (id, via) in &packages {
        let Some(via) = via else { continue };
        if out.merged.contains_key(*id) {
            continue;
        }
        let commit = via.commit.trim();
        let found = commit.len() >= 7
            && lines.iter().any(|(c, s)| {
                c.len() >= commit.len()
                    && c[..commit.len()].eq_ignore_ascii_case(commit)
                    && *s == via.subject.trim()
            });
        if found {
            out.merged.insert(
                (*id).to_string(),
                format!("merged_via {commit}: {}", via.subject.trim()),
            );
        } else {
            out.missing_via.push((
                (*id).to_string(),
                format!(
                    "merged_via {commit} „{}“ steht nicht in der Merge-Historie",
                    via.subject.trim()
                ),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // B4-M2CLOSE: keine Spec-ID (Infrastruktur M2-Q01). Titel wörtlich aus der Hauptlinie (main 816e51d2).
    #[test]
    fn branch_of_each_merge_title_form() {
        for (subject, expected) in [
            ("m2: Merge M2-U27 (Fable approved 90)", Some("U27")),
            (
                "m2: Merge m2/fix-stop (CRAWL-126 Stoppen aus Pausiert)",
                Some("fix-stop"),
            ),
            ("m2: merge int-21 into n15f", Some("int-21")),
            ("m2: Merge main in Integrationsrunde 12", Some("main")),
            ("m2: Integration Runde 6 – merge m2/u04", Some("u04")),
            (
                "m2: Integration Runde 6 – merge m2/ci-slim (K2-67)",
                Some("ci-slim"),
            ),
            (
                "m2: Integration Runde 7 – U21 (Crawl-Verwaltung UI)",
                Some("U21"),
            ),
            (
                "m2: Integration Runde 7 – N01 (C9-Auflösung Welle 1)",
                Some("N01"),
            ),
            (
                "merge: m2/n02a (M2-N02a) in Integrationsrunde 8",
                Some("n02a"),
            ),
            ("merge: origin/main in m2/n02a", None),
            (
                "merge: M2-N33 calamine 0.38.0 gegen RUSTSEC-2026-0436",
                None,
            ),
            ("m3: Merge r01 (Fable-Korrekturen, Beta-Vorzug)", None),
            ("docs: m2: Merge im Text", None),
        ] {
            assert_eq!(merged_branch(subject), expected, "{subject}");
        }
    }

    // B4-M2CLOSE: keine Spec-ID (Infrastruktur M2-Q01).
    #[test]
    fn longest_package_prefix_and_merged_via_with_subject_check() {
        let via_ok = MergedVia {
            commit: "5c70de74".into(),
            subject: "m2: Merge base-4r (Fable N06/Q02 freigegeben)".into(),
        };
        let via_wrong_subject = MergedVia {
            commit: "62164954".into(),
            subject: "m2: Merge N20 (anderer Titel)".into(),
        };
        let log = "\
aaaaaaaa00000000\tm2: Merge m2/fix-live-2 (Nachtrag)
bbbbbbbb00000000\tm2: Integration Runde 6 – merge m2/u04
cccccccc00000000\tm2: Merge m2/int-16
5c70de74ffffffff\tm2: Merge base-4r (Fable N06/Q02 freigegeben)
6216495400000000\tm2: Merge N20 (Fable N15F approved, N20 approved mit Auflagen)
dddddddd00000000\tm2: Merge M2-U04-alt (älterer Merge desselben Pakets)
";
        let d = detect(
            log,
            [
                ("M2-FIX-LIVE", None),
                ("M2-FIX", None),
                ("M2-U04", None),
                ("M2-N06", Some(&via_ok)),
                ("M2-N15F", Some(&via_wrong_subject)),
                ("M2-N20", None),
            ],
        );
        assert_eq!(
            d.merged.keys().collect::<Vec<_>>(),
            ["M2-FIX-LIVE", "M2-N06", "M2-N20", "M2-U04"]
        );
        assert_eq!(d.merged["M2-U04"], "m2: Integration Runde 6 – merge m2/u04");
        assert_eq!(
            d.merged["M2-N06"],
            "merged_via 5c70de74: m2: Merge base-4r (Fable N06/Q02 freigegeben)"
        );
        assert_eq!(d.other.keys().collect::<Vec<_>>(), ["base-4r", "int-16"]);
        assert_eq!(d.missing_via.len(), 1);
        assert_eq!(d.missing_via[0].0, "M2-N15F");
    }
}
