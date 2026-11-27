# Zweck:    Kompletter Veroeffentlichungsablauf in einem Schritt:
#             1. Version in Cargo.toml setzen (feste Nummer oder patch/minor/major)
#             2. tools\Release-Paket.ps1: Buildnummer erhoehen, testen, bauen, ZIP + SHA256 nach dist\
#             3. "Flashen": fertige EXE in den Auslieferungsordner des Projekts kopieren (falls vorgesehen)
#             4. Alle Aenderungen committen, Tag v<Version> anlegen, Zweig und Tag pushen
#             5. GitHub-Release mit ZIP und Pruefsumme erstellen
#           Gemeinsamer Standard aller Rust-Programme.
# Aufruf:   powershell -ExecutionPolicy Bypass -File tools\Veroeffentlichen.ps1 -Version patch `
#               -Titel "Kurzbeschreibung" -NotizenDatei notizen.md [-Vorabversion] [-Probelauf]
#           -Version       feste Nummer (z. B. 1.2.3) oder patch / minor / major / gleich
#           -Titel         erste Zeile der Commit-Nachricht (", Version x.y.z" wird angehaengt)
#           -Notizen       Release-Text (Zeilen mit `n trennen) oder
#           -NotizenDatei  Release-Text aus UTF-8-Datei (auch Commit-Text)
#           -CoAutor       optionale Zeile "Co-Authored-By: ..." fuer die Commit-Nachricht
#           -Vorabversion  Release als Vorabversion (wird von der Update-Suche nicht angeboten)
#           -Probelauf     nur anzeigen, was geschehen wuerde; nichts wird veraendert
# Hardware: Windows-PC mit git, cargo und gh (angemeldet)
# Autor:    Johann Gerner

param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Titel,
    [string]$Notizen = "",
    [string]$NotizenDatei = "",
    [string]$CoAutor = "",
    [switch]$Vorabversion,
    [switch]$Probelauf
)

# --- projektabhaengig ---------------------------------------------------------------------------
$Programm      = "DGM-NT"
$Paket         = "DGM-NT"
$Repository    = "Jgerner01/DGM-NT"
$Zweig         = "rust"
# Flashen: Programmdatei im cargo-Zielordner (release\...) -> Ziel relativ zum Crate-Ordner; leer = kein Flashen
$Flashen       = @{}
$StandardVorab = $false
# -------------------------------------------------------------------------------------------------

$ErrorActionPreference = "Stop"
$crate = Split-Path $PSScriptRoot -Parent
Set-Location $crate
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Invoke-Git {
    & git @args
    if ($LASTEXITCODE -ne 0) { throw "git $($args -join ' ') ist fehlgeschlagen." }
}

# --- Vorpruefungen ------------------------------------------------------------------------------
foreach ($werkzeug in @("git", "cargo", "gh")) {
    if (-not (Get-Command $werkzeug -ErrorAction SilentlyContinue)) { throw "$werkzeug wurde nicht gefunden." }
}
$aktuellerZweig = (& git rev-parse --abbrev-ref HEAD).Trim()
if ($aktuellerZweig -ne $Zweig) { throw "Aktueller Zweig ist '$aktuellerZweig', erwartet '$Zweig'." }
Invoke-Git fetch -q origin
$hinten = [int](& git rev-list --count "HEAD..origin/$Zweig")
if ($hinten -gt 0) { throw "Der lokale Zweig liegt $hinten Commit(s) hinter origin/$Zweig. Erst 'git pull'." }

$cargoPfad = Join-Path $crate "Cargo.toml"
$cargoText = [System.IO.File]::ReadAllText($cargoPfad)
$muster = [regex]'(?m)^version\s*=\s*"([^"]+)"'
$treffer = $muster.Match($cargoText)
if (-not $treffer.Success) { throw "Die Version wurde in Cargo.toml nicht gefunden." }
$alt = $treffer.Groups[1].Value
$teile = $alt.Split('.') | ForEach-Object { [int]$_ }
switch ($Version) {
    "patch"  { $neu = "$($teile[0]).$($teile[1]).$($teile[2] + 1)" }
    "minor"  { $neu = "$($teile[0]).$($teile[1] + 1).0" }
    "major"  { $neu = "$($teile[0] + 1).0.0" }
    "gleich" { $neu = $alt }
    default  {
        if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Ungueltige Version '$Version'." }
        $neu = $Version
    }
}
$tag = "v$neu"
if (& git tag --list $tag) { throw "Der Tag $tag existiert bereits." }
if (& git ls-remote --tags origin "refs/tags/$tag") { throw "Der Tag $tag existiert bereits auf GitHub." }

if ($NotizenDatei) {
    if (-not (Test-Path $NotizenDatei)) { throw "$NotizenDatei wurde nicht gefunden." }
    $Notizen = [System.IO.File]::ReadAllText((Resolve-Path $NotizenDatei))
}
if (-not $Notizen.Trim()) { throw "Release-Notizen fehlen (-Notizen oder -NotizenDatei)." }
$vorab = $Vorabversion -or $StandardVorab
$buildAlt = [int]([System.IO.File]::ReadAllText((Join-Path $crate "BUILDNUMMER")).Trim())

Write-Host "$Programm $alt -> $neu (Build $buildAlt -> $($buildAlt + 1)), Zweig $Zweig, Repository $Repository"
if ($vorab) { Write-Host "Release als Vorabversion" }
foreach ($e in $Flashen.GetEnumerator()) { Write-Host "Flashen: $($e.Key) -> $($e.Value)" }
Write-Host "Geaenderte Dateien:"
& git status --short
if ($Probelauf) {
    Write-Host ""
    Write-Host "Probelauf: nichts veraendert."
    exit 0
}

# --- 1. Version setzen --------------------------------------------------------------------------
if ($neu -ne $alt) {
    $cargoText = $muster.Replace($cargoText, "version = `"$neu`"", 1)
    [System.IO.File]::WriteAllText($cargoPfad, $cargoText, $utf8)
}

# --- 2. Paket bauen -----------------------------------------------------------------------------
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "Release-Paket.ps1")
if ($LASTEXITCODE -ne 0) { throw "Release-Paket.ps1 ist fehlgeschlagen (Cargo.toml und BUILDNUMMER ggf. von Hand zuruecksetzen)." }
$build = [int]([System.IO.File]::ReadAllText((Join-Path $crate "BUILDNUMMER")).Trim())
$zip = Join-Path $crate "dist\$Paket-$tag-win-x64.zip"
foreach ($d in @($zip, "$zip.sha256")) { if (-not (Test-Path $d)) { throw "$d fehlt." } }

# --- 3. Flashen ---------------------------------------------------------------------------------
if ($Flashen.Count -gt 0) {
    $ziel = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory
    foreach ($e in $Flashen.GetEnumerator()) {
        $quelle = Join-Path $ziel "release\$($e.Key)"
        $nach = Join-Path $crate $e.Value
        New-Item -ItemType Directory -Force -Path (Split-Path $nach -Parent) | Out-Null
        Copy-Item $quelle $nach -Force
        Write-Host "Geflasht: $nach"
    }
}

# --- 4. Commit, Tag, Push -----------------------------------------------------------------------
$temp = [System.IO.Path]::GetTempFileName()
try {
    $nachricht = "$Titel, Version $neu`n`n$($Notizen.Trim())`n`nBuild $build."
    if ($CoAutor) { $nachricht += "`n`n$CoAutor" }
    [System.IO.File]::WriteAllText($temp, $nachricht + "`n", $utf8)
    Invoke-Git add -A
    Invoke-Git commit -q -F $temp
    Invoke-Git tag -a $tag -m "$Programm $neu"
    Invoke-Git push -q origin $Zweig
    Invoke-Git push -q origin $tag

    # --- 5. GitHub-Release ----------------------------------------------------------------------
    [System.IO.File]::WriteAllText($temp, "$($Notizen.Trim())`n`nBuild $build.`n", $utf8)
    $argumente = @("release", "create", $tag, $zip, "$zip.sha256", "-R", $Repository, "--target", $Zweig,
                   "--title", "$Programm $neu", "--notes-file", $temp)
    if ($vorab) { $argumente += "--prerelease" }
    & gh @argumente
    if ($LASTEXITCODE -ne 0) { throw "gh release create ist fehlgeschlagen (Commit und Tag sind bereits gepusht)." }
} finally {
    Remove-Item $temp -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "Fertig: $Programm $neu (Build $build) veroeffentlicht."
Write-Host "  https://github.com/$Repository/releases/tag/$tag"
