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
//!
//! M3 … M5 (M3-Q01, `detect_milestone`): Titel `m<n>: Merge [m<n>/][M<n>-]<Zweig>[ inkl. <Zweig>, …]`,
//! `merge: [m<n>/]M<n>-<Paket> …` und die Reihen B3, B4 … (`merge: B4-LAGFIX …`), die nicht in der Paketliste
//! stehen, aber als bekannt gelten (Liste `series`). Fremde Meilensteine (`m3: Merge m2/n34`) und Zweige ohne
//! Planpaket landen in `other`.

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

/// Ergebnis der Erkennung für M3–M5 (M3-Q01).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MilestoneDetection {
    /// Paket-ID → Beleg (Merge-Titel bzw. `merged_via <commit>: <titel>`).
    pub merged: BTreeMap<String, String>,
    /// Pakete der Reihen B3, B4 … (nicht in der Paketliste, aber bekannt) → erster Merge-Titel.
    pub series: BTreeMap<String, String>,
    /// Zweig ohne Planpaket → erster Merge-Titel.
    pub other: BTreeMap<String, String>,
    /// `merged_via`-Belege, die nicht in der Historie stehen (Paket-ID, Grund).
    pub missing_via: Vec<(String, String)>,
}

/// Ist `token` die ID eines Pakets der Reihen B3, B4 … (`B<Zahl>-<Name>`, z. B. `B4-LAGFIX`, `B3-N00`)?
pub fn is_series_id(token: &str) -> bool {
    let Some(rest) = token.strip_prefix('B') else {
        return false;
    };
    let Some((num, name)) = rest.split_once('-') else {
        return false;
    };
    !num.is_empty()
        && num.chars().all(|c| c.is_ascii_digit())
        && !name.is_empty()
        && name.split('-').all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
}

/// Bindestrich-Token am Anfang von `rest` und der Rest danach.
fn token(rest: &str) -> Option<(&str, &str)> {
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .unwrap_or(rest.len());
    let tok = rest[..end].trim_end_matches('-');
    (!tok.is_empty() && !tok.starts_with('-')).then(|| (tok, &rest[tok.len()..]))
}

/// Ein Merge-Titel auf der Hauptlinie für Meilenstein `n` (3–5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MilestoneMerge<'a> {
    /// Zweige bzw. Paket-IDs dieses Meilensteins (erster Eintrag, dann `inkl. …`).
    Branches(Vec<&'a str>),
    /// Paket einer Reihe B3, B4 … (`merge: B4-LAGFIX …`).
    Series(&'a str),
    /// Zweig eines anderen Meilensteins (`m3: Merge m2/n34`, `merge: M2-N33 …`).
    Foreign(String),
}

/// Erkennt die Titelformen von M3 … M5 auf der Hauptlinie:
///
/// 1. `m<n>: Merge [m<n>/][M<n>-]<Zweig>[ inkl. <Zweig>[, <Zweig>][ und <Zweig>]] …` (D1, z. B.
///    `m3: Merge m3/k01 (…)`, `m3: Merge w02 inkl. w01 (…)`); `m<k>/<Zweig>` eines anderen Meilensteins ist fremd,
///    `<Name>/<Zweig>` ohne Meilenstein (`origin/main … in m3/int-f`) ist ein Merge in einen Zweig und zählt nicht;
/// 2. `merge: [m<n>/]M<n>-<Paket> …` (z. B. `merge: M3-V01 Übersicht …`);
/// 3. `merge: B<Zahl>-<Name> …` für die Reihen B3, B4 … (z. B. `merge: B4-LAGFIX …`, `merge: B3-COV – …`).
///
/// Freitext-Titel (`merge: Crawl-Datei-Beschädigung …`) und Merges in Zweige (`merge: origin/main in m3/diff`)
/// liefern `None`.
pub fn milestone_merge(subject: &str, n: u8) -> Option<MilestoneMerge<'_>> {
    let ms = format!("m{n}");
    if let Some(rest) = strip_prefix_ci(subject, &format!("{ms}: Merge ")) {
        let (first, mut after) = token(rest)?;
        if let Some(path) = after.strip_prefix('/') {
            // `m<k>/<Zweig>`: eigener oder fremder Meilenstein; sonst Merge in einen Zweig.
            let k = first
                .strip_prefix(['m', 'M'])
                .filter(|d| d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()))?;
            let (branch, rest_after) = token(path)?;
            if k != n.to_string() {
                return Some(MilestoneMerge::Foreign(format!("{first}/{branch}")));
            }
            after = rest_after;
            let branch = strip_prefix_ci(branch, &format!("M{n}-")).unwrap_or(branch);
            return Some(MilestoneMerge::Branches(with_included(branch, after)));
        }
        let first = strip_prefix_ci(first, &format!("M{n}-")).unwrap_or(first);
        return Some(MilestoneMerge::Branches(with_included(first, after)));
    }
    let rest = strip_prefix_ci(subject, "merge: ")?;
    let rest = strip_prefix_ci(rest, &format!("{ms}/")).unwrap_or(rest);
    let (first, after) = token(rest)?;
    if after.starts_with('/') {
        return None;
    }
    if is_series_id(first) {
        return Some(MilestoneMerge::Series(first));
    }
    let (prefix, id) = first.split_once('-')?;
    let k = prefix.strip_prefix('M')?;
    if k.len() != 1 || !k.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if k == n.to_string() {
        Some(MilestoneMerge::Branches(vec![id]))
    } else {
        Some(MilestoneMerge::Foreign(first.to_string()))
    }
}

/// Erster Zweig und die mit `inkl.` genannten weiteren Zweige (`w02 inkl. w01`).
fn with_included<'a>(first: &'a str, after: &'a str) -> Vec<&'a str> {
    let mut out = vec![first];
    let Some(mut rest) = after.strip_prefix(" inkl. ") else {
        return out;
    };
    while let Some((tok, tail)) = token(rest) {
        out.push(tok);
        match [", ", " und "]
            .iter()
            .find_map(|sep| tail.strip_prefix(sep))
        {
            Some(next) => rest = next,
            None => break,
        }
    }
    out
}

/// Erkennt gemergte Pakete eines Meilensteins `n` (3–5) aus der Merge-Historie. `packages` sind
/// (Paket-ID, `merged_via`) in Planreihenfolge.
pub fn detect_milestone<'a>(
    log: &str,
    n: u8,
    packages: impl IntoIterator<Item = (&'a str, Option<&'a MergedVia>)>,
) -> MilestoneDetection {
    let packages: Vec<_> = packages.into_iter().collect();
    let prefix = format!("M{n}-");
    let mut ids: BTreeMap<String, &str> = BTreeMap::new();
    for (id, _) in &packages {
        ids.insert(id.to_ascii_uppercase(), id);
        ids.insert(
            id.trim_start_matches(prefix.as_str()).to_ascii_uppercase(),
            id,
        );
    }
    let lines: Vec<(&str, &str)> = log
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(c, s)| (c.trim(), s.trim()))
        .collect();
    let mut out = MilestoneDetection::default();
    // Wie M2: der erste Treffer in Logreihenfolge (neueste zuerst) ist der Beleg.
    for (_, subject) in &lines {
        match milestone_merge(subject, n) {
            None => {}
            Some(MilestoneMerge::Series(id)) => {
                out.series
                    .entry(id.to_string())
                    .or_insert_with(|| (*subject).to_string());
            }
            Some(MilestoneMerge::Foreign(branch)) => {
                out.other
                    .entry(branch)
                    .or_insert_with(|| (*subject).to_string());
            }
            Some(MilestoneMerge::Branches(branches)) => {
                for token in branches {
                    let parts: Vec<_> = token.split('-').collect();
                    let id = (1..=parts.len())
                        .rev()
                        .find_map(|k| ids.get(&parts[..k].join("-").to_ascii_uppercase()));
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
            }
        }
    }
    // `merged_via` wie in M2 (`detect`): genau dieser Commit mit genau diesem Titel.
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

    // M3-Q01: keine Spec-ID (Infrastruktur). Titel wörtlich aus der Hauptlinie (origin/main bb0b27e1).
    #[test]
    fn milestone_titles_of_main() {
        use MilestoneMerge::{Branches, Foreign, Series};
        for (subject, expected) in [
            (
                "m3: Merge m3/k01 (Chromium-Bereitstellung nach K3-01, Fable approved mit Auflagen)",
                Some(Branches(vec!["k01"])),
            ),
            ("m3: Merge r01 (Fable-Korrekturen, Beta-Vorzug)", Some(Branches(vec!["r01"]))),
            (
                "m3: Merge w02 inkl. w01 (Fable approved mit Auflagen 82, Auflagen 1–11 erfüllt)",
                Some(Branches(vec!["w02", "w01"])),
            ),
            (
                "m3: Merge r03 Nachtrag (Aufräumen mit Wiederholung)",
                Some(Branches(vec!["r03"])),
            ),
            (
                "m3: Merge m2/n34 (Live-Test reagiert wie die UI auf Stale, Fable approved)",
                Some(Foreign("m2/n34".into())),
            ),
            ("m3: Merge origin/main (M2 bis int-26, Spec-Festlegung 07) in m3/int-f", None),
            (
                "merge: M3-V01 Übersicht nach Teilmengen (Fable approved 90)",
                Some(Branches(vec!["V01"])),
            ),
            (
                "merge: B4-LAGFIX Zähler und Tabellenfuß folgen dem Live-Tick (F7)",
                Some(Series("B4-LAGFIX")),
            ),
            ("merge: B3-COV – Spec-coverage-M2 meldet Testfehler immer rot", Some(Series("B3-COV"))),
            ("merge: B3-N00 Restpunkte M2", Some(Series("B3-N00"))),
            (
                "merge: M2-N33 calamine 0.38.0 gegen RUSTSEC-2026-0436",
                Some(Foreign("M2-N33".into())),
            ),
            ("merge: origin/main in m3/diff (beta.3-Plan Docker-Image)", None),
            ("merge: Crawl-Datei-Beschädigung durch Auflisten behoben", None),
            ("merge: CI Spec-Abdeckung ohne Debuginfo", None),
            ("m3: Basis – Integrationsstand Runde 15 plus M3-Plan", None),
            ("m2: Merge M2-U27 (Fable approved 90)", None),
            ("docs: m3: Merge im Text", None),
        ] {
            assert_eq!(milestone_merge(subject, 3), expected, "{subject}");
        }
        assert_eq!(
            milestone_merge("m4: Merge m4/f01 (Fable approved)", 4),
            Some(Branches(vec!["f01"]))
        );
        assert_eq!(
            milestone_merge("m4: Merge m4/f01 (Fable approved)", 3),
            None
        );
        assert!(is_series_id("B4-M2CLOSE") && is_series_id("B10-X"));
        assert!(!is_series_id("B4-") && !is_series_id("Basis") && !is_series_id("B4-lagfix"));
    }

    // M3-Q01: keine Spec-ID (Infrastruktur).
    #[test]
    fn milestone_detection_with_included_branches_and_merged_via() {
        let via = MergedVia {
            commit: "c412c1d0".into(),
            subject: "m3: Basis – Integrationsstand Runde 15 plus M3-Plan".into(),
        };
        let log = "\
1111111100000000\tmerge: B4-LAGFIX Zähler
2222222200000000\tm3: Merge w02 inkl. w01 und w03 (Fable approved)
3333333300000000\tm3: Merge m3/fix-e (CRAWL-126)
c412c1d0aaaaaaaa\tm3: Basis – Integrationsstand Runde 15 plus M3-Plan
4444444400000000\tmerge: M3-V02 Visualisierungen
5555555500000000\tmerge: B4-LAGFIX älterer Titel
";
        let d = detect_milestone(
            log,
            3,
            [
                ("M3-W01", None),
                ("M3-W02", None),
                ("M3-F01", Some(&via)),
                ("M3-V02", None),
                ("M3-FIX", None),
            ],
        );
        assert_eq!(
            d.merged.keys().collect::<Vec<_>>(),
            ["M3-F01", "M3-FIX", "M3-V02", "M3-W01", "M3-W02"]
        );
        assert_eq!(
            d.merged["M3-W01"],
            "m3: Merge w02 inkl. w01 und w03 (Fable approved)"
        );
        assert_eq!(d.series["B4-LAGFIX"], "merge: B4-LAGFIX Zähler");
        assert_eq!(d.other.keys().collect::<Vec<_>>(), ["w03"]);
        assert!(d.missing_via.is_empty());
    }
}
