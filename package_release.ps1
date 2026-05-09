#Requires -Version 5.1
<#
.SYNOPSIS
    ビルドして VST3 / CLAP を zip にまとめる。
.DESCRIPTION
    cargo xtask bundle dispersion_equalizer --release を実行し、
    target\bundled 内の .vst3 と .clap を
    "Dispersion Equalizer v{version}.zip" としてプロジェクト直下に出力する。
.EXAMPLE
    .\package_release.ps1
    .\package_release.ps1 -NoBuild   # ビルドをスキップして既存バンドルをそのまま zip 化
#>
param([switch]$NoBuild)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

# --- バージョン取得 ---
$verLine = Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' |
    Select-Object -First 1
$version = if ($verLine) { $verLine.Matches.Groups[1].Value } else { "0.0.0" }
Write-Host "=== Dispersion Equalizer v$version ===" -ForegroundColor Cyan

$bundled = "target\bundled"
$vst3Src = "$bundled\Dispersion Equalizer.vst3"
$clapSrc = "$bundled\Dispersion Equalizer.clap"
$zipOut  = "Dispersion Equalizer v$version.zip"
$stage   = "target\_release_stage"

# --- ビルド ---
if (-not $NoBuild) {
    Write-Host "[1/3] Building release..." -ForegroundColor Yellow
    cargo xtask bundle dispersion_equalizer --release
    if ($LASTEXITCODE -ne 0) { throw "Build failed (exit $LASTEXITCODE)" }
    Write-Host "Build succeeded." -ForegroundColor Green
} else {
    Write-Host "[1/3] Skipping build (-NoBuild)" -ForegroundColor DarkYellow
}

if (-not (Test-Path $vst3Src)) { throw "VST3 not found: $vst3Src" }
if (-not (Test-Path $clapSrc)) { throw "CLAP not found: $clapSrc" }

# --- ステージング ---
Write-Host "[2/3] Staging..." -ForegroundColor Yellow
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Path $stage -Force | Out-Null

Copy-Item $vst3Src (Join-Path $stage "Dispersion Equalizer.vst3") -Recurse -Force
Copy-Item $clapSrc (Join-Path $stage "Dispersion Equalizer.clap") -Force

# --- ZIP 作成 ---
Write-Host "[3/3] Creating '$zipOut'..." -ForegroundColor Yellow
if (Test-Path $zipOut) { Remove-Item $zipOut -Force }
Compress-Archive `
    -Path (Get-ChildItem $stage).FullName `
    -DestinationPath $zipOut `
    -CompressionLevel Optimal
Remove-Item $stage -Recurse -Force

$sizeMB = [math]::Round((Get-Item $zipOut).Length / 1MB, 2)
Write-Host "Done: $zipOut ($sizeMB MB)" -ForegroundColor Green
