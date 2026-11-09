#!/usr/bin/env python3
"""Generische Planprüfung für M3, M4 und M5 (Paket M3-Q01; Nachfolger von M2-check.py für diese Meilensteine).

Aufruf aus der Repo-Wurzel:  python3 -I docs/plan/plan-check.py [--milestone M3|M4|M5 ...]
Ohne --milestone werden M3, M4 und M5 geprüft. Exit 0 = alle Prüfungen bestanden, 1 = Befunde (Liste auf stdout).
`--selftest` prüft das Skript gegen die Fixture crates/os-parity/tests/data/coverage_mx/plan (erwartete Befunde
in erwartet.txt); `--root <dir>` liest Plan und Spec aus einem anderen Baum (docs/plan, docs/spec).

Prüft je Meilenstein Mn die Datei docs/plan/Mn-packages.json:
  P1  Paket-IDs eindeutig mit Präfix Mn-, Pflichtfelder vorhanden, Wellen, kind und runner bekannt.
  P2  depends_on: im eigenen Meilenstein nur auf frühere Wellen; über Meilensteine hinweg nur auf vorhandene Pakete
      früherer Meilensteine (M1 … Mn-1), nie auf spätere.
  P3  spec_ids/extra_spec_ids gegen die Spec-Dateien: jede ID steht genau einmal als Überschrift in docs/spec/NN-*.md;
      jede spec_id genau einmal im Plan und in keiner anderen Paketliste M1–M5; die Menge entspricht der
      README-Zuordnung (ERWARTET) plus den begründeten Zusätzen (ZUSATZ); IDs aus Kapiteln ohne Meilenstein
      (OHNE_MEILENSTEIN, z. B. 11-teams.md) stehen in keinem Paket und gelten nicht als fehlend.
  P4  owns: keine Überschneidung innerhalb einer Welle (Stichprobe über Glob-Präfixe wie M2-check.py P4,
      „ohne …“-Ausschlüsse werden beachtet; gewollte Paare in ERLAUBT).
  P12 Nutzer-Abnahmen: "human": true genau dann, wenn runner und model "human" sind; keine spec_ids; das letzte Gate
      hängt (transitiv) an jeder Nutzer-Abnahme außer dem Release-Schritt, der Release-Schritt am letzten Gate.

P2-Einträge werden nicht pauschal ausgeschlossen: ERWARTET nennt sie, wenn die README sie einem Meilenstein zuordnet
(K3-15: UI-094, UI-097, UI-098, UI-100). Das Skript ist Teil der Planpflege: Wer Mn-packages.json ändert, führt es vor
dem Commit aus. Es liest nur und verändert keine Datei.
"""
import argparse
import importlib.util
import os
import itertools
import json
import re
import sys
from pathlib import Path

sys.dont_write_bytecode = True  # kein __pycache__ neben M2-check.py

ROOT = Path(__file__).resolve().parents[2]
SELBSTTEST = ROOT / "crates" / "os-parity" / "tests" / "data" / "coverage_mx" / "plan"

MEILENSTEINE = ["M3", "M4", "M5"]
PFLICHTFELDER = ("id", "kind", "title", "wave", "spec_ids", "owns", "depends_on", "size", "prompt_notes", "model", "runner")
KINDS = {"package", "spike", "gate", "measurement", "release"}
RUNNER = {"server", "ci", "local-mac", "human"}
WELLE_RE = re.compile(r"^(\d+)([a-z]?)$")
ID_RE = re.compile(r"^#{2,4} +([A-Z][A-Z0-9]*-\d{3})\b", re.M)
PRIO_RE = re.compile(r"\*\*Priorität:\*\*\s*(P\d)")

# Kapitel ohne Meilenstein: ihre IDs stehen in keinem Paket und fehlen nirgends.
OHNE_MEILENSTEIN = {
    "11-teams.md": "Entwurf vom 09.11.2026, keinem Meilenstein zugeordnet (Roadmap R10, nach Release 1.0)",
}


def ids_im_bereich(spec, datei, praefix, von, bis, prios=None):
    """IDs `praefix-von … praefix-bis` aus `datei`, optional nur mit den Prioritäten `prios`."""
    out = set()
    for sid, (f, prio) in spec.items():
        if f != datei or not sid.startswith(praefix + "-"):
            continue
        nr = int(sid.split("-")[1])
        if von <= nr <= bis and (prios is None or prio in prios):
            out.add(sid)
    return out


def erwartet(n, spec):
    """README-Meilensteinzuordnung (docs/spec/README.md, Tabelle „Meilensteinzuordnung“) als ID-Menge.

    Die Zeilen sind Prosa; die Regeln stehen deshalb hier ausdrücklich, mit Bezug auf die Zeile.
    """
    if n == "M3":
        # M3-Zeile: P0/P1-Einträge aus OUT-250 … 266 (P2 OUT-253, -254, -264 nicht; OUT-257 neu seit 09.11.2026);
        # Visualisierung (Nutzerentscheid 30.10.2026, K3-15): UI-102 (P1) und die vorgezogenen P2-Einträge.
        return ids_im_bereich(spec, "06-output-ops.md", "OUT", 250, 266, {"P0", "P1"}) | {
            "UI-094", "UI-097", "UI-098", "UI-100", "UI-102"}
    if n == "M4":
        # M4-Zeile: 08 vollständig (AI-001 … AI-127); INTEG-160 … 189; ONPAGE-159 … 164; UI-072, UI-101;
        # OUT-065, OUT-069; AGENT-030. Teilkriterien [M4] anderer Einträge stehen in extra_spec_ids.
        return (ids_im_bereich(spec, "08-ai.md", "AI", 1, 127)
                | ids_im_bereich(spec, "04-custom-integrations.md", "INTEG", 160, 189)
                | ids_im_bereich(spec, "02-tabs-onpage.md", "ONPAGE", 159, 164)
                | {"UI-072", "UI-101", "OUT-065", "OUT-069", "AGENT-030"})
    if n == "M5":
        # M5-Zeile: 09 vollständig (D4S-001 … D4S-076).
        return ids_im_bereich(spec, "09-dataforseo.md", "D4S", 1, 76)
    raise ValueError(n)


# spec_ids außerhalb der README-Zuordnung, mit Grund (Planentscheidungen).
ZUSATZ = {
    "M3": {
        "CRAWL-142": "Verweis auf OUT-250, von M2 nach M3 verschoben (K2-13; M3.md 1.1)",
        "OUT-246": "Absturzberichte, OS-Festlegung Nutzer 03.11.2026 (Paket M3-C01)",
        "OUT-247": "Absturzberichte, OS-Festlegung Nutzer 03.11.2026 (Paket M3-C01)",
    },
    "M4": {
        "AI-200": "bedingtes Paket M4-X01 (K4-02)",
        "AGENT-060": "bedingtes Paket M4-Z01 (K4-03, 10 F „zusammen mit M4“)",
        "AGENT-061": "bedingtes Paket M4-Z01 (K4-03)",
    },
    "M5": {
        "D4S-200": "bedingtes Paket M5-X02 (K5-01)",
    },
}

# Gewollte owns-Überschneidungen innerhalb einer Welle (Paar → Grund).
ERLAUBT = {
    "M3": {},
    "M4": {},
    "M5": {},
}


def lade_m2_check():
    """Glob-Heuristik aus M2-check.py (P4), damit beide Prüfungen dieselben Besitzmuster gleich auslegen."""
    spec = importlib.util.spec_from_file_location("m2_check", ROOT / "docs" / "plan" / "M2-check.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def ausgeschlossen(m2check, path, pat, excl):
    """Wie M2-check.py `excluded`, aber „ohne …“ auch relativ zu jedem übergeordneten Ordner des Musters
    (`ai/{settings,cost}/** (ohne settings/accounts/**)` schließt `ai/settings/accounts/**` aus)."""
    if m2check.excluded(path, pat, excl):
        return True
    teile = m2check.base_dir(pat).split("/")
    for e in excl:
        for e2 in m2check.expand_braces(e):
            for i in range(1, len(teile)):
                kandidat = "/".join(teile[:i]) + "/" + e2
                if m2check.to_regex(kandidat).match(path) or path.startswith(kandidat.rstrip("*").rstrip("/") + "/"):
                    return True
    return False


def ueberschneidung(m2check, a_entries, b_entries):
    """Stichprobe wie M2-check.py `overlap` mit `ausgeschlossen`."""
    for ea in a_entries:
        pa, xa = m2check.parse_owns(ea)
        for eb in b_entries:
            pb, xb = m2check.parse_owns(eb)
            for ga, gb in itertools.product(pa, pb):
                ra, rb = m2check.to_regex(ga), m2check.to_regex(gb)
                for s, r, own_pat, own_x, other_pat, other_x in (
                    (m2check.sample(ga), rb, ga, xa, gb, xb),
                    (m2check.sample(gb), ra, gb, xb, ga, xa),
                ):
                    if (r.match(s) and not ausgeschlossen(m2check, s, other_pat, other_x)
                            and not ausgeschlossen(m2check, s, own_pat, own_x)):
                        return f"{ea}  ×  {eb}"
    return None


def spec_ueberschriften(spec_dir):
    """ID → [(Datei, Priorität)] aus docs/spec/NN-*.md (README und Reviews zählen nicht)."""
    out = {}
    for f in sorted(spec_dir.glob("*.md")):
        if not re.match(r"^\d\d-", f.name):
            continue
        text = f.read_text(encoding="utf-8")
        teile = re.split(r"^(#{2,4} +[A-Z][A-Z0-9]*-\d{3}\b.*)$", text, flags=re.M)
        for i in range(1, len(teile), 2):
            sid = ID_RE.match(teile[i]).group(1)
            m = PRIO_RE.search(teile[i + 1])
            out.setdefault(sid, []).append((f.name, m.group(1) if m else None))
    return out


def welle(w):
    m = WELLE_RE.match(str(w))
    return (int(m.group(1)), m.group(2)) if m else None


def lade(plan_dir, n):
    path = plan_dir / f"{n}-packages.json"
    if not path.exists():
        return None
    doc = json.loads(path.read_text(encoding="utf-8"))
    return doc["packages"] if isinstance(doc, dict) else doc


def pruefe(n, alle, spec_roh, m2check):
    pk = alle[n]
    errs = []
    nr = int(n[1:])
    byid = {}
    for p in pk:
        for k in PFLICHTFELDER:
            if k not in p:
                errs.append(f"P1 {p.get('id')}: Feld {k} fehlt")
        pid = p.get("id", "?")
        if not pid.startswith(n + "-"):
            errs.append(f"P1 {pid}: ID ohne Präfix {n}-")
        if pid in byid:
            errs.append(f"P1 doppelte ID {pid}")
        byid[pid] = p
        if welle(p.get("wave")) is None:
            errs.append(f"P1 {pid}: unbekannte Welle {p.get('wave')!r}")
        if p.get("kind") not in KINDS:
            errs.append(f"P1 {pid}: unbekannte Art {p.get('kind')!r}")
        runner = str(p.get("runner", ""))
        if not runner or any(r not in RUNNER for r in runner.split("+")):
            errs.append(f"P1 {pid}: unbekannter runner {runner!r}")
        for k in ("spec_ids", "extra_spec_ids", "owns", "depends_on"):
            if k in p and not isinstance(p[k], list):
                errs.append(f"P1 {pid}: Feld {k} ist keine Liste")

    # P2: Abhängigkeiten nur in frühere Wellen bzw. frühere Meilensteine.
    for p in pk:
        for d in p.get("depends_on", []):
            m = re.match(r"^M(\d)-", d)
            if not m:
                errs.append(f"P2 {p['id']}: Abhängigkeit {d} ohne Meilenstein-Präfix")
                continue
            k = int(m.group(1))
            if k == nr:
                if d not in byid:
                    errs.append(f"P2 {p['id']}: unbekannte Abhängigkeit {d}")
                elif welle(byid[d]["wave"]) and welle(p["wave"]) and welle(byid[d]["wave"]) >= welle(p["wave"]):
                    errs.append(f"P2 {p['id']} (W{p['wave']}) hängt an {d} (W{byid[d]['wave']})")
            elif k > nr:
                errs.append(f"P2 {p['id']} hängt an {d} aus einem späteren Meilenstein")
            else:
                fremd = alle.get(f"M{k}")
                if fremd is None:
                    errs.append(f"P2 {p['id']}: Paketliste M{k} fehlt für {d}")
                elif not any(q.get("id") == d for q in fremd):
                    errs.append(f"P2 {p['id']}: unbekannte Abhängigkeit {d} (nicht in M{k}-packages.json)")

    # P3: spec_ids gegen die Spec-Dateien und die README-Zuordnung.
    spec = {sid: v[0] for sid, v in spec_roh.items()}
    ohne = {sid for sid, (f, _) in spec.items() if f in OHNE_MEILENSTEIN}
    seen = {}
    for p in pk:
        for feld in ("spec_ids", "extra_spec_ids"):
            for s in p.get(feld, []):
                if s not in spec_roh:
                    errs.append(f"P3 {p['id']}: {feld} {s} steht in keiner Spec-Datei")
                elif len(spec_roh[s]) > 1:
                    errs.append(f"P3 {s} steht mehrfach in den Spec-Dateien: {', '.join(f for f, _ in spec_roh[s])}")
                if s in ohne:
                    errs.append(f"P3 {p['id']}: {s} aus {spec[s][0]} ({OHNE_MEILENSTEIN[spec[s][0]]}) im Plan")
        for s in p.get("spec_ids", []):
            if s in seen:
                errs.append(f"P3 {s} doppelt: {seen[s]} und {p['id']}")
            seen[s] = p["id"]
    for k, fremd in alle.items():
        if k == n or fremd is None:
            continue
        for q in fremd:
            for s in q.get("spec_ids", []):
                if s in seen:
                    errs.append(f"P3 {s} steht in {seen[s]} und in {q.get('id')} ({k})")
    soll = erwartet(n, spec)
    zusatz = ZUSATZ.get(n, {})
    for s in sorted(soll - set(seen)):
        datei, prio = spec.get(s, ("keine Spec-Datei", "-"))
        errs.append(f"P3 {s} ({datei}, {prio}) gehört laut README zu {n}, steht aber in keinem Paket")
    for s in sorted(set(seen) - soll - set(zusatz)):
        errs.append(f"P3 {s} ({seen[s]}) außerhalb der README-Zuordnung von {n} und ohne Grund in ZUSATZ")
    for s in sorted(set(zusatz) - set(seen)):
        errs.append(f"P3 ZUSATZ nennt {s}, das in keinem Paket von {n} steht")

    # P4: owns je Welle ohne Überschneidung.
    erlaubt = ERLAUBT.get(n, {})
    wellen = sorted({str(p["wave"]) for p in pk if welle(p.get("wave"))}, key=welle)
    for w in wellen:
        wp = [p for p in pk if str(p["wave"]) == w and p.get("kind", "package") != "release" and not p.get("human")]
        for a, b in itertools.combinations(wp, 2):
            if frozenset({a["id"], b["id"]}) in erlaubt:
                continue
            hit = ueberschneidung(m2check, a.get("owns", []), b.get("owns", []))
            if hit:
                errs.append(f"P4 Welle {w}: {a['id']} / {b['id']}: {hit}")

    # P12: Nutzer-Abnahmen.
    def alle_deps(pid, acc=None):
        acc = set() if acc is None else acc
        for d in byid.get(pid, {"depends_on": []}).get("depends_on", []):
            if d not in acc:
                acc.add(d)
                alle_deps(d, acc)
        return acc

    gates = sorted((p for p in pk if p.get("kind") == "gate" and welle(p.get("wave"))), key=lambda p: welle(p["wave"]))
    letztes = gates[-1]["id"] if gates else None
    if letztes is None:
        errs.append(f"P12 {n}: kein Gate-Paket (kind gate)")
    vor_gate = alle_deps(letztes) if letztes else set()
    for p in pk:
        human = bool(p.get("human"))
        menschlich = p.get("runner") == "human" or p.get("model") == "human"
        if human and not (p.get("runner") == "human" and p.get("model") == "human"):
            errs.append(f"P12 {p['id']}: Nutzer-Abnahme ohne runner/model human")
        if menschlich and not human:
            errs.append(f"P12 {p['id']}: runner/model human ohne \"human\": true")
        if not human:
            continue
        if p.get("spec_ids"):
            errs.append(f"P12 {p['id']}: Nutzer-Abnahme mit spec_ids")
        if p.get("kind") == "release":
            if letztes and letztes not in alle_deps(p["id"]):
                errs.append(f"P12 Release-Schritt {p['id']} hängt nicht am Gate {letztes}")
        elif letztes and p["id"] not in vor_gate:
            errs.append(f"P12 Gate {letztes} hängt nicht an der Nutzer-Abnahme {p['id']}")

    extra = {s for p in pk for s in p.get("extra_spec_ids", [])}
    print(f"{n}: Pakete {len(pk)}, spec_ids {len(seen)} (README {len(soll)}, Zusatz {len(zusatz)}), "
          f"extra_spec_ids {len(extra)}, Wellen {len(wellen)}, IDs ohne Meilenstein {len(ohne)}")
    return errs


def befunde(root, ziele):
    plan_dir, spec_dir = root / "docs" / "plan", root / "docs" / "spec"
    alle = {f"M{k}": lade(plan_dir, f"M{k}") for k in range(1, 6)}
    spec_roh = spec_ueberschriften(spec_dir)
    m2check = lade_m2_check()
    errs = []
    for n in ziele:
        if alle[n] is None:
            errs.append(f"P1 docs/plan/{n}-packages.json fehlt")
            continue
        errs.extend(pruefe(n, alle, spec_roh, m2check))
    return errs


def selbsttest():
    """Fixture mit je einem Befund pro Prüfung; die Liste muss exakt erwartet.txt entsprechen."""
    soll = [z for z in (SELBSTTEST / "erwartet.txt").read_text(encoding="utf-8").splitlines() if z.strip()]
    ist = befunde(SELBSTTEST, ["M3"])
    if ist == soll:
        print(f"Selbsttest OK: {len(ist)} erwartete Befunde")
        return 0
    print("Selbsttest FEHLER")
    for z in soll:
        if z not in ist:
            print(f"  fehlt:       {z}")
    for z in ist:
        if z not in soll:
            print(f"  unerwartet:  {z}")
    if sorted(ist) == sorted(soll):
        print("  (gleiche Befunde, andere Reihenfolge)")
    return 1


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--milestone", action="append", choices=MEILENSTEINE,
                    help="nur diesen Meilenstein prüfen (wiederholbar; Standard M3, M4, M5)")
    ap.add_argument("--root", type=Path, default=ROOT, help="Wurzel mit docs/plan und docs/spec (Standard: Repo)")
    ap.add_argument("--selftest", action="store_true", help="gegen die Fixture des Skripts prüfen")
    args = ap.parse_args(argv)
    if args.selftest:
        return selbsttest()
    errs = befunde(Path(os.path.abspath(args.root)), args.milestone or MEILENSTEINE)
    if errs:
        print("\n".join(errs))
        return 1
    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
