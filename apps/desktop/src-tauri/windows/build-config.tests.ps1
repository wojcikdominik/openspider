# Deterministic fixtures for K3-07, no real signing key.
# spec: OUT-254 (K3-16): both fixed channel endpoints, no build channel.
$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot 'build-config.ps1'
$stable = 'https://github.com/wojcikdominik/openspider/releases/latest/download/latest.json'
$beta = 'https://github.com/wojcikdominik/openspider/releases/download/update-beta/latest-beta.json'
function Assert-Endpoints($config) {
    $actual = @($config.plugins.updater.endpoints)
    if ($actual.Count -ne 2 -or $actual[0] -ne $stable -or $actual[1] -ne $beta) { throw "Kanal-Endpunkte falsch: $($actual -join ', ')" }
}
$root = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force (Join-Path $root 'apps/desktop/src-tauri') | Out-Null
Push-Location $root
try {
    $previous = -1
    foreach ($case in @(
        @('1.0.0-beta.0', '1.0.0', 0), @('1.0.0-beta.1', '1.0.1', 1), @('1.0.0-beta.499', '1.0.499', 499),
        @('1.0.0-rc.1', '1.0.500', 500), @('1.0.0-rc.499', '1.0.998', 998),
        @('1.0.0', '1.0.999', 999), @('1.0.1-beta.1', '1.0.1001', 1001)
    )) {
        @{ version = $case[0] } | ConvertTo-Json | Set-Content apps/desktop/src-tauri/tauri.conf.json
        & $script -Output config.json
        $config = Get-Content config.json -Raw | ConvertFrom-Json
        if ($config.bundle.windows.wix.version -ne $case[1]) { throw 'MSI-Versionsabbildung falsch' }
        if ($case[2] -le $previous) { throw 'MSI-Version nicht monoton' }
        if ($config.bundle.createUpdaterArtifacts) { throw 'Updater ohne Schlüssel aktiviert' }
        Assert-Endpoints $config
        $previous = $case[2]
    }
    # Defaults: no offline WebView, no updater artifacts without key, same endpoints with and without updater.
    foreach ($arguments in @(@{}, @{ Updater = $true })) {
        & $script -Output config.json @arguments
        $config = Get-Content config.json -Raw | ConvertFrom-Json
        Assert-Endpoints $config
        if ($config.bundle.windows.webviewInstallMode) { throw 'Offline-WebView ohne Eingabe aktiviert' }
        if ([bool]$config.bundle.createUpdaterArtifacts -ne [bool]$arguments.Updater) { throw 'Updater-Artefakte falsch' }
    }
    # The former build channel parameter is gone (OUT-254): a channel can no longer be passed in.
    $rejected = $false
    try { & $script -Output config.json -Channel beta } catch { $rejected = $true }
    if (-not $rejected) { throw 'Build-Kanal wird noch angenommen' }
    foreach ($version in @('256.0.0', '1.256.0', '1.0.65', '1.0.0-rc.0', '1.0.0-beta.500', '1.0.0-rc.500', '1.0.0-other.1')) {
        @{ version = $version } | ConvertTo-Json | Set-Content apps/desktop/src-tauri/tauri.conf.json
        $rejected = $false
        try { & $script -Output config.json } catch { $rejected = $true }
        if (-not $rejected) { throw "Ungültige Version akzeptiert: $version" }
    }
    Write-Output 'OK: MSI-Versionen, Grenzen, feste Kanal-Endpunkte, Offline-Standard und fehlende Updater-Schlüssel'
} finally {
    Pop-Location
    Remove-Item -LiteralPath $root -Recurse -Force
}
