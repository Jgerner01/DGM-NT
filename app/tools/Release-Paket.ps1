# Zweck:    Erhoeht die Buildnummer, testet, baut und erzeugt die beiden Dateien, die der Update-Dienst erwartet:
#             <Paket>-v<Version>-win-x64.zip  und  <Paket>-v<Version>-win-x64.zip.sha256
#           Die Version stammt aus Cargo.toml. Das Skript veroeffentlicht nichts.
#           Gemeinsamer Standard aller Rust-Programme (Vorlage: GNSS-NT).
# Aufruf:   powershell -ExecutionPolicy Bypass -File tools\Release-Paket.ps1 [-GleicherBuild]
# Hardware: Windows-PC
# Autor:    Johann Gerner

param(
    # Buildnummer nicht erhoehen (z. B. bei erneutem Packen derselben Version)
    [switch]$GleicherBuild
)

# --- projektabhaengig ---------------------------------------------------------------------------
$Programm   = "DGM-NT"
$Paket      = "DGM-NT"
$Repository = "Jgerner01/DGM-NT"
$CargoArgs  = @()
$Programme  = @("dgm-nt.exe")
$Beilagen   = @("../README.md")
# -------------------------------------------------------------------------------------------------

$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

$treffer = Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $treffer) { throw "Die Version wurde in Cargo.toml nicht gefunden." }
$version = $treffer.Matches[0].Groups[1].Value
$tag = "v$version"

$build = [int](Get-Content "BUILDNUMMER" -Raw).Trim()
if (-not $GleicherBuild) {
    $build++
    [System.IO.File]::WriteAllText((Resolve-Path "BUILDNUMMER"), "$build`n")
}

Write-Host "Baue $Programm $version (Build $build) ..."
cargo test --release @CargoArgs
if ($LASTEXITCODE -ne 0) { throw "Tests fehlgeschlagen." }
cargo build --release @CargoArgs
if ($LASTEXITCODE -ne 0) { throw "cargo build ist fehlgeschlagen." }

# Zielordner von cargo erfragen (beruecksichtigt target-dir aus .cargo\config.toml)
$ziel = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory
if (-not $ziel) { throw "Der Zielordner von cargo konnte nicht ermittelt werden." }
$dateien = @($Programme | ForEach-Object { Join-Path $ziel "release\$_" }) + $Beilagen
foreach ($d in $dateien) { if (-not (Test-Path $d)) { throw "$d fehlt." } }

New-Item -ItemType Directory -Force -Path "dist" | Out-Null
$zipName = "$Paket-$tag-win-x64.zip"
$zipPfad = Join-Path "dist" $zipName
if (Test-Path $zipPfad) { Remove-Item $zipPfad }
Compress-Archive -Path $dateien -DestinationPath $zipPfad

$hash = (Get-FileHash -Algorithm SHA256 -Path $zipPfad).Hash
# Format: "<Pruefsumme>  <Dateiname>", ohne Byte-Order-Mark
[System.IO.File]::WriteAllText((Join-Path (Resolve-Path "dist") "$zipName.sha256"), "$hash  $zipName`n", (New-Object System.Text.UTF8Encoding($false)))

Write-Host ""
Write-Host "Fertig: $zipPfad  (Build $build, $([math]::Round((Get-Item $zipPfad).Length / 1MB, 1)) MB)"
Write-Host "Naechste Schritte:"
Write-Host "  git commit -am `"$Programm $version (Build $build)`"; git tag $tag; git push --follow-tags"
Write-Host "  gh release create $tag `"$zipPfad`" `"$zipPfad.sha256`" -R $Repository --title `"$Programm $version`" --notes `"...`""
