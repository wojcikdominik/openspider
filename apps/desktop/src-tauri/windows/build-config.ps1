# Generate a temporary config; never change tracked version files in a release build.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Output,
    [switch]$Sign,
    [switch]$Updater,
    [switch]$OfflineWebView
)
$ErrorActionPreference = 'Stop'
$version = (Get-Content apps/desktop/src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
if ($version -notmatch '^(\d+)\.(\d+)\.(\d+)(?:-(beta|rc)\.(\d+))?$') { throw 'Nicht unterstützte Release-Version' }
$major = [int]$Matches[1]; $minor = [int]$Matches[2]; $patch = [int]$Matches[3]
$phase = $Matches[4]; $number = [int]$Matches[5]
if ($major -gt 255 -or $minor -gt 255 -or $patch -gt 64) { throw 'MSI-Versionsgrenze überschritten' }
$stage = 999
if ($phase) {
    # beta.0 is reserved for the R04 dry run; RC still starts at 1.
    $minimum = if ($phase -eq 'beta') { 0 } else { 1 }
    if ($number -lt $minimum -or $number -gt 499) { throw 'Beta-Nummer muss 0..499, RC-Nummer 1..499 sein' }
    $stage = $number
    if ($phase -eq 'rc') { $stage += 499 }
}
$windows = @{ wix = @{ version = "$major.$minor.$($patch * 1000 + $stage)" } }
if ($Sign) {
    $script = (Resolve-Path scripts/release/sign-windows.ps1).Path
    $windows.signCommand = @{ cmd = 'pwsh'; args = @('-NoProfile', '-NonInteractive', '-File', $script, '%1') }
}
if ($OfflineWebView) { $windows.webviewInstallMode = @{ type = 'offlineInstaller'; silent = $true } }
# OUT-254 (K3-16): no build channel; both fixed channel endpoints, stable first. The app picks the channel at runtime.
$endpoints = @(
    'https://github.com/wojcikdominik/openspider/releases/latest/download/latest.json',
    'https://github.com/wojcikdominik/openspider/releases/download/update-beta/latest-beta.json'
)
$config = @{
    bundle = @{ createUpdaterArtifacts = [bool]$Updater; windows = $windows }
    plugins = @{ updater = @{
        endpoints = $endpoints
        pubkey = if ($Updater) { $env:OPENSPIDER_UPDATER_PUBKEY } else { '' }
    } }
}
$config | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $Output -Encoding utf8
Write-Output "MSI-Version: $($windows.wix.version), Endpunkte: $($endpoints -join ', ')"
