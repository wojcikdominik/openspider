# spec: OUT-263
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Artifacts, [Parameter(Mandatory)][string]$Testsite,
      [Parameter(Mandatory)][string]$Chromium, [string]$Output = 'smoke-results')
$ErrorActionPreference = 'Stop'
function Invoke-Installer([string]$File, [string]$Arguments) {
    $p = Start-Process -FilePath $File -ArgumentList $Arguments -Wait -PassThru
    if ($p.ExitCode -notin @(0, 3010)) { throw "Installer $File failed: $($p.ExitCode)" }
}
New-Item -ItemType Directory -Force $Output | Out-Null
$Output = (Resolve-Path $Output).Path
foreach ($kind in @('msi', 'nsis')) {
    $files = @(Get-ChildItem -LiteralPath $Artifacts -Recurse -File | Where-Object {
        if ($kind -eq 'msi') { $_.Extension -eq '.msi' } else { $_.Name -like '*-setup.exe' }
    })
    if ($files.Count -ne 1) { throw "Expected one $kind installer, found $($files.Count)" }
    $installer = $files[0].FullName
    $install = Join-Path $env:RUNNER_TEMP "R03 Installed $kind"
    $installed = $false
    $primaryError = $null
    try {
        if ($kind -eq 'msi') {
            Invoke-Installer 'msiexec.exe' "/i `"$installer`" /qn /norestart INSTALLDIR=`"$install`" /l*v `"$Output/msi-install.log`""
        } else {
            Invoke-Installer $installer "/S /D=$install"
        }
        $installed = $true
        # B4-RELCHECK R1: Bundle-Kennung der installierten Desktop-EXE (Paketwahl des Updaters, OUT-262).
        & "$PSScriptRoot/../release/check-bundle-marker.ps1" -InstallDir $install -Kind $kind
        # Python discovers the CLI in the installed payload, never in the downloaded loose CLI.
        python scripts/smoke/installed.py --install-dir $install --testsite $Testsite --chromium $Chromium --output "$Output/$kind"
        if ($LASTEXITCODE -ne 0) { throw "Installed $kind smoke failed" }
    } catch {
        $primaryError = $_
        throw
    } finally {
        if ($installed) {
            try {
                if ($kind -eq 'msi') {
                    Invoke-Installer 'msiexec.exe' "/x `"$installer`" /qn /norestart /l*v `"$Output/msi-uninstall.log`""
                } elseif (Test-Path -LiteralPath $install) {
                    $uninstall = @(Get-ChildItem -LiteralPath $install -Filter '*uninstall*.exe')
                    if ($uninstall.Count -ne 1) { throw 'NSIS uninstaller missing or ambiguous' }
                    Invoke-Installer $uninstall[0].FullName "/S _?=$install"
                }
                if (Test-Path (Join-Path $install 'openspider.exe')) { throw 'CLI remains after uninstall' }
                # NSIS _?= leaves its own executable. Remove only the test installation directory.
                # The uninstaller can stay locked briefly after exit (virus scan, emulation): retry.
                for ($attempt = 1; Test-Path -LiteralPath $install; $attempt++) {
                    try {
                        Remove-Item -LiteralPath $install -Recurse -Force -ErrorAction Stop
                    } catch {
                        if ($attempt -ge 10) { throw }
                        Start-Sleep -Seconds 3
                    }
                }
            } catch {
                if ($null -ne $primaryError) {
                    Write-Warning "Uninstall also failed; original smoke error retained: $_"
                } else {
                    throw
                }
            }
        }
    }
}
