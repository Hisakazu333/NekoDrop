param(
  [switch]$SkipTests,
  [switch]$NoCopy,
  [string]$Bundles = "nsis,msi",
  [switch]$Help
)

$ErrorActionPreference = "Stop"

function Show-Usage {
  @"
NekoDrop Windows desktop packaging

Usage:
  powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 [-SkipTests] [-NoCopy] [-Bundles nsis]
  npm run package:windows -- [-SkipTests] [-NoCopy] [-Bundles nsis]

Options:
  -SkipTests       Build without running cargo test.
  -NoCopy          Leave Tauri bundles under the package target directory only.
  -Bundles VALUE   Tauri bundle list. Default: nsis,msi. Examples: nsis / msi / nsis,msi.
  -Help            Show this help.

Output:
  release\desktop\<yyyyMMdd-HHmmss>\
"@
}

function Require-Command {
  param([string]$Name)

  if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
    throw "Missing required command: $Name"
  }
}

if ($Help) {
  Show-Usage
  exit 0
}

$IsWindowsHost = [string][System.IO.Path]::DirectorySeparatorChar -eq "\"
if (-not $IsWindowsHost) {
  throw "This script builds Win11/Windows desktop installers. On macOS use: npm run package:desktop"
}

$RootDir = Resolve-Path (Join-Path $PSScriptRoot "..")
$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"

Require-Command "npm"
Require-Command "cargo"

if (-not $env:CARGO_TARGET_DIR) {
  $env:CARGO_TARGET_DIR = Join-Path $RootDir "target\package-windows\$Stamp"
}

if (-not $env:RUSTC) {
  $RustcPath = (Get-Command rustc -ErrorAction SilentlyContinue)
  if ($RustcPath) {
    $env:RUSTC = $RustcPath.Source
  }
}

if (-not $env:RUSTDOC) {
  $RustdocPath = (Get-Command rustdoc -ErrorAction SilentlyContinue)
  if ($RustdocPath) {
    $env:RUSTDOC = $RustdocPath.Source
  }
}

Set-Location $RootDir

Write-Host "==> Building desktop frontend"
npm run build

if (-not $SkipTests) {
  Write-Host "==> Running Rust workspace tests"
  cargo test --workspace
}

Write-Host "==> Building Tauri Windows bundle: $Bundles"
npm --workspace apps/desktop run tauri -- build --bundles $Bundles

if (-not $NoCopy) {
  $OutputDir = Join-Path $RootDir "release\desktop\$Stamp"
  $BundleDir = Join-Path $env:CARGO_TARGET_DIR "release\bundle"
  $ExePath = Join-Path $env:CARGO_TARGET_DIR "release\nekodrop-desktop.exe"

  New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

  if (Test-Path $BundleDir) {
    Copy-Item -Recurse -Force $BundleDir $OutputDir
  }

  if (Test-Path $ExePath) {
    Copy-Item -Force $ExePath $OutputDir
  }

  Write-Host "==> Package output"
  Write-Host $OutputDir

  $InstallerFiles = Get-ChildItem -Path $OutputDir -Recurse -File -Include *.exe,*.msi,*.msix,*.appx |
    Where-Object { $_.FullName -notlike "*\release\desktop\$Stamp\nekodrop-desktop.exe" } |
    Sort-Object FullName

  if ($InstallerFiles.Count -gt 0) {
    Write-Host "==> Installers to run on Win11"
    foreach ($Installer in $InstallerFiles) {
      Write-Host $Installer.FullName
    }
  } else {
    Write-Host "==> No installer file was copied. Check the Tauri bundle output above."
  }
} else {
  Write-Host "==> Bundle output"
  Write-Host (Join-Path $env:CARGO_TARGET_DIR "release\bundle")
}
