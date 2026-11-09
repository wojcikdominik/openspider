# Update-Fixtures (M3-U01)

Ausschließlich Testmaterial: `test.key` / `test.key.pub` wurde lokal mit dem Tauri-Signer erzeugt,
mit leerem Testpasswort. Diese Schlüssel niemals als Repository-Secret oder Produkt-Schlüssel verwenden.
`artifact` ist mit diesem Testschlüssel signiert (`artifact.sig`, Tauri-base64-minisign,
gebunden an 1.0.1). Ein geändertes Byte muss die Prüfung scheitern lassen.

`manifest.json` enthält Tauri-v2-Metadaten mit signierten Artefaktverweisen, nicht eine
Signatur des JSON-Dokuments. Die Metadaten kommen nur über HTTPS; die Artefaktbytes müssen
vor Verwendung separat mit `update::verify_release` gegen Schlüssel und angekündigte Version geprüft werden (Staging/Installation: M3-U02).

`scenario.toml` bietet stabile, ältere, Beta-, HTTP-Download- und Redirect-Varianten sowie
hängende/ungültige Antworten. `os_testsite::mocks::update::https_fixture` startet diese Site
mit einer ephemeren Test-CA und vollständigem Request-Log. `FixtureProxy` protokolliert CONNECT
und tunnelt ausschließlich zu einem vorgegebenen Loopback-Ziel.
`settings.toml` zeigt die app-weite Update-Einstellung samt Zeitstempel.

Reproduktion der Signatur (nur vorhandenes Testmaterial):

```sh
node node_modules/@tauri-apps/cli/tauri.js signer sign \
  -f tests/fixtures/update/test.key -p '' --app-version 1.0.1 tests/fixtures/update/artifact
```

Anschließend muss die Signatur auch in `manifest.json` und `scenario.toml` aktualisiert werden.

`release-manifest.json` (B4-MSIUPD) hat die Form, die `scripts/release/manifest.mjs` für ein Release schreibt: genau
die Schlüssel `darwin-aarch64`, `darwin-x86_64`, `windows-x86_64-nsis` und `windows-x86_64-msi`, je mit eigener URL
nach dem Namensschema der Release-Dateien (Signatur wie in `manifest.json`). Daran prüfen die Tests, dass eine
MSI-Installation nur den MSI-Eintrag und eine NSIS-Installation nur den NSIS-Eintrag wählt.
