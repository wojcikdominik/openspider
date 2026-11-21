# Windows-Installer und Signierung

M3-R02 baut Windows x64 auf `windows-latest`. Der Workflow
[`release-windows.yml`](../../.github/workflows/release-windows.yml) erzeugt ausschließlich
Actions-Artefakte, keine veröffentlichten Releases. Chromium ist in der frühen Beta nicht enthalten.
Bundle-ID: `org.openspider.desktop`. CLI und Desktop-App binden die VC-Runtime
statisch ein; die CLI erhält beim expliziten Windows-Target-Build `+crt-static`.

Für Crawl-Dateien normalisiert der Store Windows-Pfade und verwendet die mitgelieferte
SQLite-VFS `win32-longpath`. Das gilt auch für Speichern unter, Reparatur, Metadaten,
Registry, Checkpoint-Verbindungen und schreibgeschützte Abfragen. Die Windows-Tests
prüfen Crawl-Dateien und CSV-Ausgaben unter Pfaden mit mehr als 260 Zeichen.

## Installation und Deinstallation

NSIS installiert ohne Administratorrechte für den aktuellen Nutzer, einschließlich CLI
`openspider.exe`, Dateizuordnungen, Startmenü und Benutzer-PATH:

```powershell
.\OpenSpider-setup.exe /S /D=C:\Tools\OpenSpider
# /D ist der letzte Parameter, auch bei Leerzeichen ohne Anführungszeichen.
& 'C:\Tools\OpenSpider\uninstall.exe' /S
& 'C:\Tools\OpenSpider\uninstall.exe' /S /REMOVEUSERDATA=1
```

MSI installiert pro Rechner. Installation/Deinstallation benötigen eine erhöhte Konsole:

```powershell
msiexec /i OpenSpider.msi /qn /norestart INSTALLDIR="C:\Program Files\OpenSpider" ADDTOPATH=1
msiexec /x OpenSpider.msi /qn /norestart
msiexec /x OpenSpider.msi /qn /norestart REMOVEUSERDATA=1
```

MSI setzt den Rechner-PATH bei der Erstinstallation nur mit `ADDTOPATH=1`;
ein Upgrade übernimmt die vorhandene PATH-Auswahl aus der Registry. Die CLI wird immer mitinstalliert.
Neue Terminals übernehmen den geänderten PATH. MSI verwaltet und entfernt seinen
PATH-Bestandteil selbst. NSIS verwaltet ausschließlich den eigenen Benutzer-PATH-Bestandteil.

Vor dem Entfernen ruft der Installer `openspider --uninstall-cleanup` auf. Die CLI
liest die Aufgabenplanung direkt und entfernt ausschließlich Aufgaben unter `\OpenSpider\`,
auch wenn lokale Planer-XML-Dateien fehlen. Fehler brechen die Bereinigung ab und werden an
den Installer weitergereicht. NSIS meldet im stillen Modus den Exit-Code als Warnung
und setzt die Deinstallation fort; interaktiv bricht NSIS mit einer Fehlermeldung ab.
Bei einem Versionswechsel bleiben die Aufgaben erhalten;
die registrierte CLI am gleichbleibenden Installationspfad wird ersetzt.

Standardmäßig bleiben sämtliche Benutzerdaten erhalten. Interaktiv fragt NSIS nach
„Benutzerdaten entfernen“, MSI bietet das Kontrollkästchen im Wartungsdialog.
`REMOVEUSERDATA=1` (NSIS: `/REMOVEUSERDATA=1`) entfernt ausschließlich die
Standardordner `%APPDATA%\OpenSpider` und `%LOCALAPPDATA%\OpenSpider` des
ausführenden Nutzers. Andere Profile, extern gespeicherte Crawls und ein über
`OPENSPIDER_HOME` gewählter Ordner werden nicht gelöscht. Solche Ordner entfernt der
Nutzer selbst. Das ist auch bei einer zentral durch SYSTEM ausgeführten MSI-Deinstallation
zu beachten: fremde Nutzerprofile werden nicht durchlaufen.

Beide Installer prüfen WebView2 und laden bei Bedarf den Microsoft-Bootstrapper
(still). Ein Dispatch mit `offline_webview=true` bettet stattdessen den
Offline-Runtime-Installer ein; diese Variante gilt für beide Installer. Eine vorhandene
Runtime wird nicht mit der App deinstalliert.

## MSI-Version (K3-07)

Der Windows-Installer vergleicht nur drei numerische Stellen. Deshalb wird die
SemVer-Version `major.minor.patch[-beta.N|-rc.N]` so abgebildet:

| Version | MSI ProductVersion |
| --- | --- |
| `1.0.0-beta.1` | `1.0.1` |
| `1.0.0-beta.499` | `1.0.499` |
| `1.0.0-rc.1` | `1.0.500` |
| `1.0.0-rc.499` | `1.0.998` |
| `1.0.0` | `1.0.999` |
| `1.0.1-beta.1` | `1.0.1001` |

Formel für die dritte Stelle: `patch * 1000 + stage`, Beta `stage=N`,
RC `stage=499+N`, stabil `stage=999`. Major/Minor müssen ≤255, Patch ≤64,
N zwischen 1 und 499 sein; andere Vorabnamen werden abgelehnt. Der feste Upgrade-Code
bleibt bei allen Versionen gleich. App, Update-Manifest und NSIS behalten die SemVer.
Die Abbildung ist vor einer späteren Erweiterung des Versionsraums erneut zu planen.

## Azure Artifact Signing mit OIDC

Der Nutzer richtet eine Azure-App mit GitHub-OIDC-Verbund ein.
Issuer: `https://token.actions.githubusercontent.com`, Audience: `api://AzureADTokenExchange`.
Empfohlen und im Windows-Job mit `environment: release` eingestellt ist das
GitHub-Environment **release** mit genau diesem Azure-Subject:
`repo:wojcikdominik@2600469/openspider@1416630178:environment:release`.
Das Environment vor dem ersten Dispatch im Repository anlegen und seine zulässigen
Deployment-Branches/Tags konfigurieren: für die Korrekturprüfung `m3/r02`, danach
`main`, `release/*` und `v*`. Diese GitHub-Muster sind keine Azure-Subjects.
Eventuelle Environment-Freigaberegeln gelten auch für `workflow_dispatch` und R04.
Ohne Signierwerte bleibt der Build unsigniert; für OIDC mit vollständigen Werten
muss das Environment-Subject eingerichtet sein.

Alternativ sind bei Jobs **ohne** `environment` separate Federated Credentials je
exaktem Ref nötig:

| Auslöser | Subject |
| --- | --- |
| Hauptbranch | `repo:wojcikdominik@2600469/openspider@1416630178:ref:refs/heads/main` |
| Paketbranch | `repo:wojcikdominik@2600469/openspider@1416630178:ref:refs/heads/m3/r02` |
| Versionstags `v*` | Je konkretem Tag, z. B. `repo:wojcikdominik@2600469/openspider@1416630178:ref:refs/tags/v1.0.0-beta.1` |
| Release-Branches `release/*` | Je konkretem Branch, z. B. `repo:wojcikdominik@2600469/openspider@1416630178:ref:refs/heads/release/1.0` |
| Anderes Environment `<name>` | `repo:wojcikdominik@2600469/openspider@1416630178:environment:<name>`; Job und Credential müssen denselben Namen verwenden. |

Azure unterstützt in normalen Federated-Credential-Subjects **keine Wildcards**:
`repo:wojcikdominik@2600469/openspider@1416630178:ref:refs/tags/v*` und
`repo:wojcikdominik@2600469/openspider@1416630178:ref:refs/heads/release/*` sind keine gültige
Musterfreigabe. Deshalb das Environment `release` verwenden, statt für jeden Tag
neue Credentials anzulegen. Die separate Azure-Vorschau „Flexible federated identity
credentials“ mit Claim-Ausdrücken wird hier nicht vorausgesetzt.
Mit einem Environment enthält das OIDC-Subject den Environment-Namen statt des Refs;
ein reines Branch-Credential reicht für den vorhandenen Windows-Job nicht aus.
Siehe [Microsoft: Federated Credentials](https://learn.microsoft.com/en-us/entra/workload-id/workload-identity-federation-create-trust)
und [GitHub: OIDC-Subjects](https://docs.github.com/en/actions/reference/security/oidc#example-subject-claims).
Die Azure-Identität benötigt die Rolle **Artifact Signing Certificate Profile Signer**
am vorgesehenen Signierprofil. Es gibt keinen Client-Secret.

Repository-Variablen oder Secrets (Variablen haben Vorrang):

- `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_SUBSCRIPTION_ID`
- `ARTIFACT_SIGNING_ENDPOINT`, `ARTIFACT_SIGNING_ACCOUNT`, `ARTIFACT_SIGNING_PROFILE`

Endpoint und Profil müssen derselben Azure-Region angehören. Der Workflow benötigt
`id-token: write`; bei `workflow_call` muss auch der Aufrufer diese Berechtigung gewähren.

`azure/login@v2` meldet die Identität über OIDC an. Das Build installiert die offiziellen
Microsoft-NuGet-Werkzeuge `Microsoft.ArtifactSigning.Client` **1.0.146** und
`Microsoft.Windows.SDK.BuildTools` **10.0.28000.2957** sowie .NET 8.
Beide Versionen sind in `windows/initialize-signing.ps1` mit `-Version` gepinnt.
Der aktuelle Client-Paketname wurde über die
[NuGet-API](https://api.nuget.org/v3-flatcontainer/microsoft.artifactsigning.client/index.json)
geprüft; `Microsoft.Trusted.Signing.Client` ist der frühere Paketname. Tauri ruft
`scripts/release/sign-windows.ps1` als `signCommand` auf, einschließlich NSIS-Uninstaller
und beider fertiger Installer. Die CLI wird bereits vor dem Bundeln signiert.
SignTool verwendet die x64-Dlib, SHA-256 und den RFC-3161-Zeitstempel von
`http://timestamp.acs.microsoft.com`. Metadaten liegen temporär außerhalb des
Repos und werden anschließend gelöscht. Die Credential-Kette erlaubt ausschließlich
die durch OIDC angemeldete Azure-CLI; Schlüssel und Metadaten werden nicht ausgegeben.

Fehlt ein Azure-Wert, wird **unsigniert** gebaut und getestet; der Lauf bleibt bei
erfolgreichen Prüfungen grün und zeigt den Hinweis in Summary und Log.
Sind alle Werte vorhanden, ist ein Signier- oder Verifikationsfehler ein echter Fehler.
`verify-windows.ps1 -Path <Datei-oder-Ordner>` fordert für jede gefundene
`.exe`, `.dll` und `.msi` `Get-AuthenticodeSignature.Status = Valid` und einen Zeitstempel.
Nur der ausdrücklich unsignierte Lauf darf `-AllowUnsigned` verwenden; beschädigte
oder ungültige Signaturen werden auch dort abgelehnt.

SmartScreen-Herausgeber/Reputation und RDP werden nach M3-Q03 manuell geprüft.

## Updater und wiederverwendbarer Workflow

Repository-Secrets: `TAURI_SIGNING_PRIVATE_KEY` und
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (bei unverschlüsseltem Schlüssel leer).
Repository-Variable: `OPENSPIDER_UPDATER_PUBKEY`.
Fehlt privater oder öffentlicher Schlüssel, setzt das temporäre Build-Overlay
`createUpdaterArtifacts=false` und meldet „Updater-Artefakte übersprungen“.
Mit Schlüsseln muss für beide Installer eine zugehörige `.sig` entstehen.
Die Updater-Signatur wird über das endgültige Authenticode-signierte Paket erstellt.

Seit 1.0.1 (K3-16, OUT-254) gibt es keinen Build-Kanal mehr. `build-config.ps1` legt immer
beide festen Endpunkte in das Build-Overlay, stabil zuerst:
`https://github.com/wojcikdominik/openspider/releases/latest/download/latest.json` und
`https://github.com/wojcikdominik/openspider/releases/download/update-beta/latest-beta.json`.
Die App wählt zur Laufzeit nach `updates.channel`. Eingabe `channel`, Parameter `-Channel`
und `OPENSPIDER_UPDATE_BETA` entfallen; `-Channel` wird abgelehnt.
Fehlen Ereignis-Eingaben ganz, fällt `offline_webview` auf `false` zurück.

R04 kann den Workflow über `uses: ./.github/workflows/release-windows.yml`,
`with: { offline_webview: false }` und `secrets: inherit` aufrufen.
Der Aufrufer gewährt `contents: read` und `id-token: write`.
Das Actions-Artefakt **openspider-windows-x86_64** enthält die Original-NSIS-/MSI-Dateinamen,
deren `.sig` (bei Updater-Schlüsseln) und die signierte CLI `openspider.exe`.
R04 verwendet die Plattformschlüssel `windows-x86_64-nsis` und `windows-x86_64-msi`.
Es werden keine Release-Assets veröffentlicht.

## Nach dem Push durch den Orchestrator

```bash
gh workflow run release-windows.yml --ref m3/r02 -f offline_webview=false
gh workflow run release-windows.yml --ref m3/r02 -f offline_webview=true
gh workflow run ci.yml --ref m3/r02 -f intel=true
```

Den ersten Windows-Lauf ohne Azure-/Updater-Werte prüfen (grün, Hinweise).
Nach dem Einrichten am 02.11. mit vollständigen Werten erneut auslösen:
beide Signaturen gültig, beide `.sig` vorhanden, vier Installations-/Deinstallationszyklen
grün. Lauf-IDs im Paketbericht ergänzen. Die Offline-Variante prüft die Einbettung;
ein frisches Windows ohne Runtime bleibt Teil der Installationsmatrix M3-Q02.

Quellen: [Tauri Windows Installer](https://v2.tauri.app/distribute/windows-installer/),
[Tauri-Konfiguration](https://v2.tauri.app/reference/config/),
[Microsoft SignTool/Dlib-Integration](https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-signing-integrations),
[MSI ProductVersion](https://learn.microsoft.com/en-us/windows/win32/msi/productversion).
