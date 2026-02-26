<#
.SYNOPSIS
    Build script for App Manager.

.DESCRIPTION
    Produces release artifacts in the dist\ directory:
      - app-manager.exe              (standalone executable)
      - app-manager-X.Y.Z-ARCH.zip  (ZIP archive containing the EXE)
      - app-manager-X.Y.Z-ARCH.msi  (Windows Installer package)

.PARAMETER Target
    Build target: exe, zip, msi, all, or clean. Default is "all".

.EXAMPLE
    .\build.ps1 exe
    .\build.ps1 zip
    .\build.ps1 msi
    .\build.ps1 all
    .\build.ps1 clean
#>

param(
    [ValidateSet("exe", "zip", "msi", "all", "clean")]
    [string]$Target = "all"
)

$ErrorActionPreference = "Stop"

# --- Configuration ---
$ProjectName    = "app-manager"
$DistDir        = "dist"
$CargoReleaseDir = "target\release"
$WixDir         = "tools\wix3\bin"
$WixSrc         = "wix\main.wxs"

# --- Ensure we run from the project root ---
Set-Location $PSScriptRoot

# --- Extract version from Cargo.toml ---
$cargoContent = Get-Content "Cargo.toml" -Raw
if ($cargoContent -match '(?m)^version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    Write-Error "Could not extract version from Cargo.toml"
}

# --- Detect architecture from Rust toolchain ---
$rustTarget = & rustup show active-toolchain 2>$null
$Arch = "x86_64"
$WixArch = "x64"
if ($rustTarget) {
    if ($rustTarget -match "aarch64") {
        $Arch = "aarch64"; $WixArch = "arm64"
    } elseif ($rustTarget -match "i686") {
        $Arch = "x86"; $WixArch = "x86"
    }
}

$ExeName = "$ProjectName.exe"
$ZipName = "$ProjectName-$Version-$Arch.zip"
$MsiName = "$ProjectName-$Version-$Arch.msi"

# --- Build functions ---

function Build-Exe {
    Write-Host "==> Building $ExeName (release)..." -ForegroundColor Cyan
    cargo build --release
    if ($LASTEXITCODE -ne 0) { Write-Error "cargo build failed" }

    New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
    Copy-Item "$CargoReleaseDir\$ExeName" "$DistDir\$ExeName" -Force
    Write-Host "  -> $DistDir\$ExeName" -ForegroundColor Green
}

function Build-Zip {
    Write-Host "==> Creating $ZipName..." -ForegroundColor Cyan

    $zipPath = "$DistDir\$ZipName"
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }

    Compress-Archive -Path "$DistDir\$ExeName", "README.md", "LICENSE.md" -DestinationPath $zipPath
    Write-Host "  -> $zipPath" -ForegroundColor Green
}

function Build-Msi {
    Write-Host "==> Creating $MsiName..." -ForegroundColor Cyan

    $candle = "$WixDir\candle.exe"
    $light  = "$WixDir\light.exe"

    if (-not (Test-Path $candle)) {
        Write-Error "WiX candle.exe not found at $candle`nDownload WiX 3 toolset to tools\wix3\"
    }

    # Compile .wxs -> .wixobj
    & $candle -nologo -arch $WixArch `
        "-dVersion=$Version" `
        "-dCargoTargetBinDir=$CargoReleaseDir" `
        -out "$DistDir\main.wixobj" `
        $WixSrc
    if ($LASTEXITCODE -ne 0) { Write-Error "candle.exe failed" }

    # Link .wixobj -> .msi
    & $light -nologo -ext WixUIExtension `
        -out "$DistDir\$MsiName" `
        "$DistDir\main.wixobj"
    if ($LASTEXITCODE -ne 0) { Write-Error "light.exe failed" }

    # Clean up intermediate files
    Remove-Item "$DistDir\main.wixobj" -Force -ErrorAction SilentlyContinue
    $wixpdb = $MsiName -replace '\.msi$', '.wixpdb'
    Remove-Item "$DistDir\$wixpdb" -Force -ErrorAction SilentlyContinue

    Write-Host "  -> $DistDir\$MsiName" -ForegroundColor Green
}

function Invoke-Clean {
    Write-Host "==> Cleaning $DistDir\..." -ForegroundColor Cyan
    if (Test-Path $DistDir) { Remove-Item $DistDir -Recurse -Force }
    Write-Host "  -> Done" -ForegroundColor Green
}

function Show-Summary {
    Write-Host ""
    Write-Host "Build complete! Artifacts in $DistDir\:" -ForegroundColor Cyan
    Get-ChildItem $DistDir -File | Format-Table Name, @{N="Size";E={
        if ($_.Length -ge 1MB) { "{0:N1} MB" -f ($_.Length / 1MB) }
        else { "{0:N0} KB" -f ($_.Length / 1KB) }
    }} -AutoSize
}

# --- Main ---

switch ($Target) {
    "exe" {
        Build-Exe
        Show-Summary
    }
    "zip" {
        Build-Exe
        Build-Zip
        Show-Summary
    }
    "msi" {
        Build-Exe
        Build-Msi
        Show-Summary
    }
    "all" {
        Build-Exe
        Build-Zip
        Build-Msi
        Show-Summary
    }
    "clean" {
        Invoke-Clean
    }
}
