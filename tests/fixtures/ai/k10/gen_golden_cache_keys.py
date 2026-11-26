"""Erzeugt tests/fixtures/ai/k06-acceptance/golden-cache-keys.json neu (M4-K10, AI-015 Nachtrag 24.11.2026, Q00).

Unabhaengig von der Rust-Implementierung: kanonisches JSON mit json.dumps(sort_keys=True, separators=(',', ':'),
ensure_ascii=False), SHA-256 ueber UTF-8. Gegenueber dem Stand von M4-K06 kommt je Eintrag das Feld `reasoning`
(effektive Reasoning-Stufe) hinzu: auf Wegen ohne Stufe (Decisions API, Embeddings) `null` und nicht in der
kanonischen Darstellung, sonst als Feld `reasoning` im Schluessel. Eingaben und Namen bleiben unveraendert.

Aufruf aus dem Wurzelverzeichnis des Repositorys: python3 -I tests/fixtures/ai/k10/gen_golden_cache_keys.py
"""
import hashlib
import json
import pathlib
import sys

PATH = pathlib.Path("tests/fixtures/ai/k06-acceptance/golden-cache-keys.json")
KEY_FORMAT_VERSION = 1
# Stufe je Eintrag; nur Eintraege mit Ausfuehrungsweg `structured-output` bzw. Vorschlagsaufgaben tragen eine Stufe.
REASONING = {"k02": "off", "k08": "low"}


def canonical(obj):
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def main():
    doc = json.loads(PATH.read_text(encoding="utf-8"))
    for entry in doc["keys"]:
        inp = entry["input"]
        level = REASONING.get(entry["name"])
        if level is not None and (inp["path"] == "decisions-api" or inp["task"] == "embedding"):
            sys.exit(f"{entry['name']}: Weg ohne Stufe")
        obj = dict(inp)
        obj["v"] = KEY_FORMAT_VERSION
        if level is not None:
            obj["reasoning"] = level
        text = canonical(obj)
        entry["reasoning"] = level
        entry["canonical"] = text
        entry["key"] = hashlib.sha256(text.encode("utf-8")).hexdigest()
    doc["_comment"] = (
        "Unabhaengige Abnahme M4-K06 (AI-015): 10 Golden-Schluessel; neu erzeugt in M4-K10 (Nachtrag Q00: Feld "
        "reasoning) mit tests/fixtures/ai/k10/gen_golden_cache_keys.py. Erwartete Werte mit Python berechnet "
        "(json.dumps sort_keys, separators=(',',':'), ensure_ascii=False; SHA-256 ueber UTF-8). Feldnamen = "
        "oeffentliche Felder von CacheKeyInput plus v = KEY_FORMAT_VERSION; reasoning (effektive Stufe) nur bei "
        "Wegen mit Stufe, sonst null und nicht im kanonischen JSON."
    )
    PATH.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
