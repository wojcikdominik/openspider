# spec: OUT-266
# spec: OUT-166 [M3]
# spec: OUT-272 [M3]
# spec: OUT-262
[CmdletBinding()]
param([Parameter(Mandatory)][string]$BundleDir, [switch]$Signed)
$ErrorActionPreference = 'Stop'
function Assert($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Run-Installer([string]$File, [string]$Arguments) {
    $p = Start-Process -FilePath $File -ArgumentList $Arguments -Wait -PassThru
    Assert ($p.ExitCode -eq 0) "Installer Exit-Code $($p.ExitCode): $File"
}
$nsis = @(Get-ChildItem "$BundleDir/nsis" -Filter '*-setup.exe')
$msi = @(Get-ChildItem "$BundleDir/msi" -Filter '*.msi')
Assert ($nsis.Count -eq 1 -and $msi.Count -eq 1) 'Genau ein NSIS- und MSI-Installer erwartet'
# Task namespace fixture independent of the local XML index.
$service = New-Object -ComObject 'Schedule.Service'
$service.Connect()
try { $null = $service.GetFolder('\OpenSpider') }
catch { $null = $service.GetFolder('\').CreateFolder('OpenSpider') }
foreach ($kind in @('nsis', 'msi')) {
    foreach ($purge in @($false, $true)) {
        $install = Join-Path $env:RUNNER_TEMP "OpenSpider $kind"
        $machine = $kind -eq 'msi'
        $scope = if ($machine) { 'Machine' } else { 'User' }
        $menuRoot = if ($machine) { [Environment]::GetFolderPath('CommonPrograms') } else { [Environment]::GetFolderPath('Programs') }
        if ($machine) {
            Run-Installer 'msiexec.exe' "/i `"$($msi[0].FullName)`" /qn /norestart INSTALLDIR=`"$install`" ADDTOPATH=1 /l*v `"$env:RUNNER_TEMP/r02-msi-install.log`""
        } else {
            # /D must be the final argument, without quotes, even with spaces.
            Run-Installer $nsis[0].FullName "/S /D=$install"
        }
        $cli = Join-Path $install 'openspider.exe'
        Assert (Test-Path -LiteralPath $cli) 'CLI fehlt'
        Assert (@(Get-ChildItem $install -Filter '*.exe').Count -ge 2) 'Desktop-Binary fehlt'
        & $cli --help | Out-Null
        Assert ($LASTEXITCODE -eq 0) 'Installierte CLI startet nicht'
        # B4-RELCHECK R1: Die Desktop-EXE trägt die Bundle-Kennung ihres Pakettyps (Paketwahl des Updaters, OUT-262).
        & "$PSScriptRoot/../../../../scripts/release/check-bundle-marker.ps1" -InstallDir $install -Kind $kind
        $pathEntries = @([Environment]::GetEnvironmentVariable('Path', $scope) -split ';' |
            ForEach-Object { $_.TrimEnd('\') })
        Assert ($pathEntries -contains $install.TrimEnd('\')) "$scope PATH fehlt"
        Assert (@(Get-ChildItem $menuRoot -Recurse -Filter '*OpenSpider*.lnk').Count -gt 0) 'Startmenü fehlt'
        $classes = if ($machine) { 'HKLM:\Software\Classes' } else { 'HKCU:\Software\Classes' }
        Assert (Test-Path "$classes\.ospider") 'Dateizuordnung fehlt'
        if ($Signed) { & "$PSScriptRoot/../../../../scripts/release/verify-windows.ps1" -Path $install }
        $data = Join-Path $env:APPDATA 'OpenSpider'
        New-Item -ItemType Directory -Force $data | Out-Null
        Set-Content -LiteralPath (Join-Path $data 'r02-keep.txt') 'retention fixture'
        $action = New-ScheduledTaskAction -Execute $cli -Argument '--help'
        $null = Register-ScheduledTask -TaskName 'r02-fixture' -TaskPath '\OpenSpider\' -Action $action -Force
        $null = Register-ScheduledTask -TaskName 'OpenSpiderR02Keep' -Action $action -Force
        $remove = if ($purge) { '1' } else { '0' }
        if ($machine) {
            Run-Installer 'msiexec.exe' "/x `"$($msi[0].FullName)`" /qn /norestart REMOVEUSERDATA=$remove /l*v `"$env:RUNNER_TEMP/r02-msi-uninstall.log`""
        } else {
            $uninstaller = @(Get-ChildItem $install -Filter '*uninstall*.exe')
            Assert ($uninstaller.Count -eq 1) 'NSIS-Uninstaller fehlt'
            Run-Installer $uninstaller[0].FullName "/S /REMOVEUSERDATA=$remove _?=$install"
        }
        Assert (-not (Test-Path $cli)) 'CLI nach Deinstallation vorhanden'
        $remainingExe = @(Get-ChildItem $install -Recurse -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Extension -in @('.exe', '.dll') -and $_.Name -notlike '*uninstall*' })
        Assert ($remainingExe.Count -eq 0) 'Programmdateien nach Deinstallation vorhanden'
        $pathEntries = @([Environment]::GetEnvironmentVariable('Path', $scope) -split ';' |
            ForEach-Object { $_.TrimEnd('\') })
        Assert ($pathEntries -notcontains $install.TrimEnd('\')) 'PATH nicht entfernt'
        Assert (@(Get-ChildItem $menuRoot -Recurse -Filter '*OpenSpider*.lnk').Count -eq 0) 'Startmenü nicht entfernt'
        Assert (-not (Get-ScheduledTask -TaskPath '\OpenSpider\' -TaskName 'r02-fixture' -ErrorAction SilentlyContinue)) 'Aufgabe nicht entfernt'
        Assert ([bool](Get-ScheduledTask -TaskName 'OpenSpiderR02Keep' -ErrorAction SilentlyContinue)) 'Fremde Aufgabe entfernt'
        Assert ((Test-Path (Join-Path $data 'r02-keep.txt')) -eq (-not $purge)) 'Benutzerdaten-Regel verletzt'
        Unregister-ScheduledTask -TaskName 'OpenSpiderR02Keep' -Confirm:$false
        # With _?= NSIS does not delete its running executable; clean only this test remnant.
        if (Test-Path $install) { Remove-Item -LiteralPath $install -Recurse -Force }
        Write-Output "OK: $kind, REMOVEUSERDATA=$remove"
    }
}
