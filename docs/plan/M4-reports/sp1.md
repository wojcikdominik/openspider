# M4-SP1 – Lizenz- und Bibliotheks-Spike (E-9 a, K4-09)

Stand: 21.11.2026, Zweig `ai/sp1`, Basis `main` (`fb5e4dbb`). Ausgangspunkt: Auswahl der Vorarbeit `refs/srv/m4/f01` (M4.md 0b), nur gelesen, nicht gemergt.

## Vorgehen

- Wegwerf-Kopie des Workspace-Stands (`git archive HEAD`) im Scratch-Ordner der Sitzung; im Repo ist nur dieser Bericht geändert.
- Je Kandidat wurde die Abhängigkeit in der Kopie in ein Crate-Manifest eingetragen (`os-analyze` bzw. `os-export`; für Lockfile und Lizenzgraph ist der Ort gleichgültig, weil alle Crates dieselben `deny.toml`-Ziele haben). Danach liefen `cargo metadata` (minimale Lockfile-Auflösung, kein allgemeines `cargo update`) und `cargo deny check licenses bans sources advisories` (cargo-deny 0.20.2, Advisory-DB vom 21.11.2026). Ausgewertet: neue Lockfile-Einträge, geänderte Versionen vorhandener Crates, `tokio`/`rust-ini`, Lizenzfeld jedes neuen Crates, neue deny-Fehler gegenüber der Basis.
- Kein `cargo build/test/clippy` auf diesem Rechner. Für die Eignung (Kosinus, Seed, Determinismus) wurden die Quellen der Kandidaten gelesen (crates.io-Archive, nichts ausgeführt). Pflegezustand nach crates.io-Veröffentlichungsdaten.

## Basisbefund (unabhängig von SP1)

`cargo deny check advisories` ist auf `main` heute **rot**: 12 Fehler, keiner davon aus M4 – `chrono 0.4.45` unmaintained (RUSTSEC-2026-0543), `lettre 0.11.23` (RUSTSEC-2026-0560 … 0565), `zip 8.6.0` (RUSTSEC-2026-0545 … 0548), `zip 9.0.4` (RUSTSEC-2026-0548). `licenses`, `bans`, `sources` sind grün. Unten zählt deshalb „deny grün“ = **keine neuen Fehler gegenüber der Basis** und licenses/bans/sources ohne Fehler. Die Basis-Advisories gehören in ein Nacharbeitspaket (K-82), nicht nach F01.

## Ergebnis je Bibliothek

`tokio 1.53.3` und `rust-ini 0.21.1` bleiben in **allen** geprüften Varianten unverändert. Keine Variante ändert die Version eines vorhandenen Crates, außer `jsonschema 0.62.0` (siehe dort).

| Bibliothek (Zweck) | Version | Lizenz direkt / neu transitiv | deny | neue Crates | Pflege | Eignung INTEG-181/186/189 | Empfehlung |
|---|---|---|---|---|---|---|---|
| jsonschema (Schema-Prüfung, os-ai) | `=0.20.0`, `default-features = false`, `draft201909`, `draft202012` | MIT / MIT, MIT/Apache-2.0, MIT OR Apache-2.0 | grün | 11 | Version vom 18.09.2024 (alt); Projekt aktiv (0.62.0 vom 21.11.2026) | offline, ohne HTTP-/Dateiauflösung | **nehmen 0.20.0**, solange K4-17 offen ist. Ab **0.21.0** zieht jede Version `borrow-or-share 0.2.4` (**MIT-0**, einzige Lizenz) über `fluent-uri`. Gibt der Orchestrator MIT-0 frei: `=0.62.0` (20 neue Crates, deny grün, hebt aber `regex` 1.13.1 → 1.13.2 und bringt `regex-automata` 0.2/0.3 als Doppelversionen). Alternative `boon 0.6.1` (MIT OR Apache-2.0) hat **dieselbe** MIT-0-Abhängigkeit. |
| docx-rs (DOCX-Export, os-export, AI-107) | `=0.4.22`, `default-features = false` | MIT / – | grün | 1 | aktiv (0.4.22 vom 21.07.2026) | – | **nehmen 0.4.22** |
| pulldown-cmark (Markdown-Export) | `=0.13.6`, `default-features = false`, `html` | MIT / MIT, MIT OR Apache-2.0 | grün | 3 | aktiv (0.13.6 vom 30.10.2026) | – | **nehmen 0.13.6**. Der F01-Pin `=0.13.4` macht `deny` **rot**: RUSTSEC-2026-0414 und RUSTSEC-2026-0433 (beide `ENABLE_WIKILINKS`); 0.13.6 ist frei. |
| ANN/HNSW (> 50.000 URLs, INTEG-181) | `hnsw =0.11.0` + `space =0.17.0` (`alloc`), beide `default-features = false` | MIT / MIT, MIT OR Apache-2.0 (`ahash 0.7.8`, `hashbrown 0.11.2`) | grün | 5 | **ungepflegt**: letzte Version 19.07.2021 | Seed über `Hnsw::new_prng` (rand_core 0.6); Metrik muss eine vorzeichenlose Ganzzahl liefern → Kosinusabstand `1 − cos` (≥ 0) als `f32::to_bits` abbilden | **nehmen 0.11.0** (kleinster Fußabdruck, deny grün). Alternativen: `instant-distance 0.6.1` (MIT OR Apache-2.0, 7 neue Crates, letzte Version 06/2023), `hnsw_rs 0.3.5` (MIT/Apache-2.0, gepflegt, aber 28 neue Crates inkl. `env_logger`, `mmap-rs`, `nix`, `windows 0.48`). Eigene HNSW in `os-analyze::semantic`: ca. 3–4 Personentage inkl. Recall-Test gegen exakte Rechnung. |
| k-means (Clusterfärbung, INTEG-186) | `linfa =0.8.1` + `linfa-clustering =0.8.1`, `default-features = false` (+ `ndarray 0.16`) | MIT OR Apache-2.0 / MIT OR Apache-2.0, MIT/Apache-2.0, Apache-2.0, MIT | grün | 22 | aktiv (0.8.1 vom 23.12.2025) | Seed über `KMeans::params_with_rng`; L2 auf L2-normalisierten Vektoren ist kosinus-monoton. **Nicht** `KMeansInit::KMeansPara` verwenden (Thread-RNGs mit atomarem Seed-Zähler → nicht reproduzierbar); `KMeansPlusPlus`/`Random` sind seed-stabil, Zuordnungsschritt parallel nur elementweise | **nehmen 0.8.1** (E-9 a erfüllt). Hinweis: Eine eigene seed-stabile k-means(++) in `os-analyze::semantic` kostet ca. 1 Personentag und spart 22 Crates (u. a. `sprs 0.11.2`, `space 0.19.0` neben 0.17, `rand_distr 0.4` neben 0.5); Entscheidung beim Orchestrator. |
| t-SNE (INTEG-186/189) | `bhtsne =0.7.14`, `default-features = false` | MIT / MIT, MIT OR Apache-2.0 | grün | 12 | aktiv (0.7.14 vom 16.11.2026; F01 hatte 0.7.9, 11 Crates, ebenfalls grün) | Distanz frei als Closure (Kosinus möglich); Startlayout über `initial_embedding` (aus eigenem Seed) – ohne diesen Aufruf nutzt die Bibliothek `rand::rng()` (nicht reproduzierbar). Gradienten-Normierung summiert mit rayon parallel (`par_iter().sum()`), Summationsreihenfolge kann zwischen Läufen schwanken | **nehmen 0.7.14 mit Auflage**: `initial_embedding` immer setzen; byte-identische CSV (INTEG-186) im ersten E02-Test prüfen. Falls nicht identisch: Lauf in einem rayon-Pool mit 1 Thread (Leistung gegen 30 s/10.000 Punkte messen) oder eigene Implementierung (ca. 3 Personentage). Perplexity-Grenze ⌊(n−1)/3⌋ passt zur Prüfung der Bibliothek. |
| UMAP (INTEG-186/189, Standardverfahren) | `umap-rs 0.4.5` geprüft | BSD-3-Clause / Apache-2.0 (`alga`, `approx 0.3`), MIT, MIT OR Apache-2.0 | allein grün (20 Crates) | 20 | wenig genutzt, letzte Version 01.12.2025; `alga 0.9.3` seit 2019 ohne Version | **ungeeignet**: SGD-Phase nutzt Hogwild-Parallelität mit `rand::rng()` je Thread, kein Seed-Parameter → keine reproduzierbaren Koordinaten (INTEG-186, INTEG-189). Zusätzlich nicht gemeinsam mit `linfa 0.8.1` auflösbar (`sprs =0.11.2` gegen `^0.11.3`) | **eigene Implementierung** in `os-analyze::semantic` (wie Vorarbeit): kNN-Graph (exakt bzw. HNSW, Kosinus), Fuzzy-Simplicial-Set, seed-gesteuerte Initialisierung, SGD mit Negativ-Sampling in fester Reihenfolge; ca. 5–7 Personentage inkl. Determinismus- und Leistungstest. |
| HDBSCAN (INTEG-186) | `hdbscan =0.12.0`, `default-features = false`, `features = ["serial", "parallel"]` | MIT OR Apache-2.0 / MIT OR Apache-2.0 (`kdtree 0.7.0`) | grün | 2 (mit `parallel` 6, davon 4 rayon-Crates, die die Auswahl ohnehin bringt) | aktiv (0.12.0 vom 24.11.2025) | deterministisch, keine Zufallszahlen; Kosinus nicht direkt, Euklid auf L2-normalisierten Vektoren ist monoton dazu (Hierarchie gleich, Stabilitätswerte λ verschieden). Rauschen = `-1` wie in INTEG-186. kd-Baum degeneriert in 768 Dimensionen zu Brute Force | **nehmen 0.12.0**. Achtung: Der F01-Eintrag `default-features = false` **ohne** Feature schaltet sowohl `cluster()` (Feature `serial`) als auch `cluster_par()` (Feature `parallel`) ab – Feature muss gesetzt werden. Alternative: `petal-clustering 0.14.0` (Apache-2.0, 12 neue Crates, bringt `ndarray 0.17` als zweite Version). |

## Gesamtauswahl

| Variante | neue Crates | deny | tokio / rust-ini |
|---|---|---|---|
| F01-Vorarbeit unverändert (inkl. `pulldown-cmark =0.13.4`, `bhtsne =0.7.9`) | 49 | **rot** (2 neue Advisories pulldown-cmark 0.13.4) | 1.53.3 / 0.21.1 |
| Empfehlung (Cargo.toml unten) | 50 | grün (keine neuen Fehler) | 1.53.3 / 0.21.1 |
| Empfehlung ohne linfa (eigene k-means) | 34 | grün | 1.53.3 / 0.21.1 |

`bans`: nur Warnungen (Mehrfachversionen 64 → 72), keine Fehler; `multiple-versions = "warn"`.

## Vorgabe für M4-F01 (`[workspace.dependencies]`)

```toml
# M4-SP1: Lizenzgraph mit cargo-deny geprüft (docs/plan/M4-reports/sp1.md)
jsonschema = { version = "=0.20.0", default-features = false, features = ["draft201909", "draft202012"] }
docx-rs = { version = "=0.4.22", default-features = false }
pulldown-cmark = { version = "=0.13.6", default-features = false, features = ["html"] }
hnsw = { version = "=0.11.0", default-features = false }
space = { version = "=0.17.0", default-features = false, features = ["alloc"] }
linfa = { version = "=0.8.1", default-features = false }
linfa-clustering = { version = "=0.8.1", default-features = false }
bhtsne = { version = "=0.7.14", default-features = false }
hdbscan = { version = "=0.12.0", default-features = false, features = ["serial", "parallel"] }
ndarray = { version = "0.16", default-features = false, features = ["std"] }
rand = "0.8"
rayon = "1"
```

Abweichungen zur Vorarbeit: `pulldown-cmark` 0.13.4 → 0.13.6 (Advisories), `bhtsne` 0.7.9 → 0.7.14 (aktuell, grün), `hdbscan` mit Features. UMAP bleibt eigene Implementierung (kein Crate). Ein Pin `tokio = "=1.53.3"` im Manifest ist für SP1 nicht nötig: keine Auswahl bewegt `tokio` oder `rust-ini` im Lockfile. Für `rand` (0.8) gilt: `hnsw` braucht einen `rand_core 0.6`-PRNG; `bhtsne` nutzt intern `rand 0.9`, bekommt aber das Startlayout von außen.

## K4-17: Fundstellen (Entscheidung beim Orchestrator)

`deny.toml` erlaubt über die CLAUDE.md-Liste hinaus `Apache-2.0 WITH LLVM-exception`, `MIT-0`, `0BSD`, `BSL-1.0`, `Unicode-3.0`. Im heutigen Lockfile von `main` werden davon als **einzige** Lizenz schon genutzt: `BSL-1.0` (`clipboard-win 5.4.1`, `error-code 3.4.0`), `Unicode-3.0` (ICU-Crates, `litemap`, `tinystr` u. a.), `0BSD` (`quoted_printable 0.5.2`), `Apache-2.0 WITH LLVM-exception` (`ar_archive_writer 0.5.3`, `target-lexicon 0.12.16`). `MIT-0` kommt bisher nur in Oder-Ausdrücken vor (`dunce 1.0.5`, `aws-lc-sys 0.46.0`). `borrow-or-share 0.2.4` (über `jsonschema ≥ 0.21` und `boon`) wäre das erste Crate allein unter MIT-0. Die Empfehlung oben kommt ohne MIT-0-only aus.

## Spec-Unklarheiten

- INTEG-186 sagt nicht, ob k-means/HDBSCAN auf den Originalvektoren (z. B. 768 Dimensionen) oder auf der 2D-Projektion clustern. Das bestimmt die Laufzeit (HDBSCAN mit kd-Baum in hoher Dimension ≈ Brute Force, O(n²·d)) und ob Kosinus nötig ist. Bitte in der Spec festlegen (betrifft E02).
- INTEG-189 verlangt Kosinus „jeweils“ für UMAP und t-SNE; für das Clustering fehlt eine Angabe zum Distanzmaß.

## Nicht geprüft

- Übersetzbarkeit mit Rust 1.99.1 und MSRV 1.90 (kein Bau erlaubt); `hnsw 0.11.0` ist fünf Jahre alt. Die F01-Vorarbeit hat die Auswahl (mit bhtsne 0.7.9) auf dem früheren Server übersetzt.
- Determinismus und Leistung zur Laufzeit (bhtsne-Summation, linfa-Zuordnung, hdbscan `cluster_par`, HNSW-Recall ≥ 99 %): nur aus dem Quelltext abgeleitet; gehört in E01/E02 und Q02 auf dem Mac mini.
- `cargo deny` für Linux-Ziele: `deny.toml` prüft nur `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`.

PAKET FERTIG
