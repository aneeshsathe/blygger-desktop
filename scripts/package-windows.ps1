# Package a release build of Burrow for Windows (x64) into dist/:
#
#   dist/Burrow-<version>-windows-x64.zip
#   dist/Burrow-windows-x64.zip            (version-less copy, for
#                                           releases/latest/download/… links)
#   dist/SHA256SUMS-windows                (both zips; the release workflow
#                                           folds these lines into the signed
#                                           SHA256SUMS)
#
# The zip holds one folder, Burrow\, with blygger.exe and licenses\ (the same
# license files scripts/bundle.sh puts in the macOS bundle). The executable
# keeps the `blygger` name; only the display name is Burrow.
#
# Run from the repository root after `cargo build --release -p blyg-app`
# (CI and .github/workflows/release.yml do both):
#
#   pwsh scripts/package-windows.ps1 [-Exe target\release\blygger.exe]

param(
    [string]$Exe = "target/release/blygger.exe"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if (-not (Test-Path $Exe)) {
    throw "no $Exe; run cargo build --release -p blyg-app first"
}

# The workspace version, as release.yml reads it.
$inWorkspace = $false
$version = $null
foreach ($line in Get-Content Cargo.toml) {
    if ($line -match '^\[workspace\.package\]') { $inWorkspace = $true; continue }
    if ($line -match '^\[') { $inWorkspace = $false }
    if ($inWorkspace -and $line -match '^version = "(.+)"') { $version = $Matches[1]; break }
}
if (-not $version) { throw "no [workspace.package] version in Cargo.toml" }

$dist = "dist"
$stage = Join-Path $dist "windows-stage"
$app = Join-Path $stage "Burrow"
$licenses = Join-Path $app "licenses"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force -Path $licenses | Out-Null

Copy-Item $Exe (Join-Path $app "blygger.exe")

# Fonts and icons are compiled into the executable; only their licenses ship.
Copy-Item LICENSE (Join-Path $licenses "LICENSE")
Copy-Item LICENSE-docs (Join-Path $licenses "LICENSE-docs") # covers the icon
Copy-Item packaging/THIRD_PARTY.md (Join-Path $licenses "THIRD_PARTY.md")
foreach ($dir in Get-ChildItem -Directory crates/blyg-app/assets/fonts) {
    foreach ($lic in Get-ChildItem -File $dir.FullName | Where-Object { $_.Name -like "LICENSE*" -or $_.Name -like "OFL*" }) {
        Copy-Item $lic.FullName (Join-Path $licenses "font-$($dir.Name)-$($lic.Name)")
    }
}
Copy-Item crates/blyg-app/assets/icons/LICENSE (Join-Path $licenses "icons-lucide-LICENSE")

$stem = "Burrow-$version-windows-x64"
$latest = "Burrow-windows-x64"
$zip = Join-Path $dist "$stem.zip"
Remove-Item -Force -ErrorAction SilentlyContinue $zip, (Join-Path $dist "$latest.zip")
Compress-Archive -Path $app -DestinationPath $zip
Copy-Item $zip (Join-Path $dist "$latest.zip")
Remove-Item -Recurse -Force $stage

# `shasum -a 256` format (two spaces), LF endings, so the lines can join the
# macOS SHA256SUMS byte for byte.
$sums = foreach ($name in "$stem.zip", "$latest.zip") {
    $hash = (Get-FileHash (Join-Path $dist $name) -Algorithm SHA256).Hash.ToLower()
    "$hash  $name"
}
[System.IO.File]::WriteAllText(
    (Join-Path (Resolve-Path $dist) "SHA256SUMS-windows"),
    (($sums -join "`n") + "`n")
)
Get-ChildItem $dist
Get-Content (Join-Path $dist "SHA256SUMS-windows")
if ($env:GITHUB_ENV) { "ZIP_STEM=$stem" | Out-File -Append -Encoding utf8 $env:GITHUB_ENV }
