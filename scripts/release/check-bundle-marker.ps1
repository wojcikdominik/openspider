# spec: OUT-262
# B4-RELCHECK R1: Die installierte Desktop-EXE muss die Bundle-Kennung ihres Pakettyps tragen. tauri-bundler ersetzt je
# Paket `__TAURI_BUNDLE_TYPE_VAR_UNK` durch `…_MSI` bzw. `…_NSS`. Scheitert das, gibt es beim Bau nur eine Warnung;
# die App gilt dann als unbekannte Installationsart und bekommt keine Updates (B4-MSIUPD). Reiner Byte-Vergleich,
# die EXE wird nicht gestartet.
#
# Die Vergleichstexte der Match-Arme in `tauri_utils::platform::bundle_type()` können zusätzlich in jeder EXE stehen,
# je Kennung gleich oft. Deshalb gilt: keine `…_UNK` mehr, und die erwartete Kennung kommt häufiger vor als die des
# anderen Windows-Pakets (eine Stelle mehr: der gepatchte Wert).
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$InstallDir,
    [Parameter(Mandatory)][ValidateSet('msi', 'nsis')][string]$Kind
)
$ErrorActionPreference = 'Stop'
$prefix = '__TAURI_BUNDLE_TYPE_VAR_'
$expected = if ($Kind -eq 'msi') { 'MSI' } else { 'NSS' }
$wrong = if ($Kind -eq 'msi') { 'NSS' } else { 'MSI' }

function Get-MarkerCount([string]$File) {
    # ISO-8859-1 bildet jedes Byte auf genau ein Zeichen ab; die Suche ist damit ein Byte-Vergleich.
    $text = [System.Text.Encoding]::GetEncoding(28591).GetString([System.IO.File]::ReadAllBytes($File))
    $counts = [ordered]@{}
    foreach ($suffix in @('UNK', 'MSI', 'NSS', 'DEB', 'RPM', 'APP')) {
        $needle = $prefix + $suffix
        $n = 0
        $at = $text.IndexOf($needle, [System.StringComparison]::Ordinal)
        while ($at -ge 0) {
            $n++
            $at = $text.IndexOf($needle, $at + $needle.Length, [System.StringComparison]::Ordinal)
        }
        $counts[$suffix] = $n
    }
    return $counts
}

# Desktop-EXE: die eine EXE im Installationsordner mit Tauri-Kennung; CLI und NSIS-Uninstaller scheiden aus.
$candidates = @(Get-ChildItem -LiteralPath $InstallDir -Filter '*.exe' -File |
    Where-Object { $_.Name -ne 'openspider.exe' -and $_.Name -notlike '*uninstall*' })
$found = @(foreach ($file in $candidates) {
    $counts = Get-MarkerCount $file.FullName
    if (($counts.Values | Measure-Object -Sum).Sum -gt 0) {
        [pscustomobject]@{ File = $file; Counts = $counts }
    }
})
if ($found.Count -ne 1) {
    throw "Bundle-Kennung ($Kind): genau eine Desktop-EXE mit Tauri-Kennung in '$InstallDir' erwartet, gefunden $($found.Count) (EXE-Dateien: $(@($candidates.Name) -join ', '))"
}
$name = $found[0].File.Name
$counts = $found[0].Counts
$summary = @($counts.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' '
if ($counts['UNK'] -gt 0) {
    throw "Bundle-Kennung ($Kind): $name enthält noch ${prefix}UNK. tauri-bundler hat die EXE nicht gepatcht; die App gälte als unbekannte Installationsart und bekäme keine Updates. Fundstellen: $summary"
}
if ($counts[$expected] -le $counts[$wrong]) {
    throw "Bundle-Kennung ($Kind): $name trägt nicht ${prefix}$expected (falsche oder fehlende Kennung; der Updater würde das falsche oder kein Paket wählen). Fundstellen: $summary"
}
Write-Output "OK Bundle-Kennung ($Kind): $name trägt ${prefix}$expected ($summary)"
