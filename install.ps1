#Requires -Version 5.1
<#
.SYNOPSIS
    ASCII Forge installer for Windows.

.DESCRIPTION
    Installs ascii-forge.exe into a per-user directory. No administrator
    rights are required and no files other than the installed executable are
    touched.

.PARAMETER Prefix
    Install directory. Defaults to $env:LOCALAPPDATA\Programs\ascii-forge\bin.

.PARAMETER Uninstall
    Remove ascii-forge.exe from the install directory and exit.

.PARAMETER Build
    Run "cargo build --release" before installing.

.PARAMETER NoBuild
    Install an existing executable and never invoke cargo.

.PARAMETER Binary
    Executable to install. Defaults to target\release\ascii-forge.exe when
    present, otherwise to ascii-forge.exe next to this script.

.PARAMETER Help
    Show usage and exit.
#>
[CmdletBinding()]
param(
    [string]$Prefix,
    [switch]$Uninstall,
    [switch]$Build,
    [switch]$NoBuild,
    [string]$Binary,
    [switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ProgramName = 'ascii-forge'
$ExecutableName = 'ascii-forge.exe'
$DefaultPrefixSuffix = 'Programs\ascii-forge\bin'

function Fail {
    param([string]$Message)
    [Console]::Error.WriteLine("install.ps1: $Message")
    exit 1
}

function Show-Usage {
    $usage = @'
ASCII Forge installer for Windows

Usage: pwsh -File install.ps1 [options]

Installs ascii-forge.exe into PREFIX (default:
%LOCALAPPDATA%\Programs\ascii-forge\bin). The script needs no administrator
rights and never removes files other than the installed executable.

Options:
  -Prefix DIR   Install into DIR instead of the default directory
  -Build        Run "cargo build --release" before installing
  -NoBuild      Install an existing executable and never invoke cargo
  -Binary PATH  Executable to install (same as ASCII_FORGE_BINARY)
  -Uninstall    Remove ascii-forge.exe from PREFIX and exit
  -Help         Show this help and exit

Environment:
  PREFIX               Install directory. A -Prefix option takes precedence.
  ASCII_FORGE_BINARY   Executable to install. Defaults to
                       target\release\ascii-forge.exe when present, otherwise
                       to ascii-forge.exe next to this script.

Without -Build or -NoBuild, an existing executable is installed as-is and
cargo is only invoked when no executable can be found.

FFmpeg and ffprobe are runtime dependencies for video export and video/GIF
import; a monospace TTF font is needed for raster output and espeak-ng is
optional for --voice. Run "ascii-forge doctor" to check your environment.

Examples:
  pwsh -File install.ps1
  pwsh -File install.ps1 -Prefix "$env:USERPROFILE\bin"
  pwsh -File install.ps1 -NoBuild
  pwsh -File install.ps1 -Binary .\ascii-forge.exe -NoBuild
  pwsh -File install.ps1 -Uninstall
'@
    Write-Output $usage
}

function Resolve-Prefix {
    if (-not [string]::IsNullOrWhiteSpace($Prefix)) {
        return $Prefix
    }

    if (-not [string]::IsNullOrWhiteSpace($env:PREFIX)) {
        return $env:PREFIX
    }

    if (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        return Join-Path $env:LOCALAPPDATA $DefaultPrefixSuffix
    }

    $localAppData = [Environment]::GetFolderPath('LocalApplicationData')
    if (-not [string]::IsNullOrWhiteSpace($localAppData)) {
        return Join-Path $localAppData $DefaultPrefixSuffix
    }

    Fail 'LOCALAPPDATA is not set; pass -Prefix DIR or set PREFIX'
}

function Resolve-SourceBinary {
    if (-not [string]::IsNullOrWhiteSpace($Binary)) {
        return $Binary
    }

    if (-not [string]::IsNullOrWhiteSpace($env:ASCII_FORGE_BINARY)) {
        return $env:ASCII_FORGE_BINARY
    }

    $candidates = @(
        (Join-Path $PSScriptRoot "target\release\$ExecutableName"),
        (Join-Path $PSScriptRoot $ExecutableName)
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return $candidate
        }
    }

    return $candidates[0]
}

function Invoke-Build {
    $manifest = Join-Path $PSScriptRoot 'Cargo.toml'
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
        Fail "cannot build from source: no Cargo.toml next to install.ps1 in '$PSScriptRoot'; extract a release archive or pass -Binary PATH"
    }

    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Fail 'cargo was not found on PATH; install Rust from https://rustup.rs, or pass -Binary PATH and rerun with -NoBuild'
    }

    Write-Output "Building $ProgramName with cargo build --release..."

    Push-Location -LiteralPath $PSScriptRoot
    try {
        & cargo build --release
        $buildExitCode = $LASTEXITCODE
    }
    catch {
        Fail "failed to run cargo build --release: $($_.Exception.Message)"
    }
    finally {
        Pop-Location
    }

    if ($buildExitCode -ne 0) {
        Fail 'cargo build --release failed; fix the build errors and rerun install.ps1'
    }
}

function Install-Executable {
    param(
        [string]$Source,
        [string]$Target
    )

    try {
        $sourcePath = [System.IO.Path]::GetFullPath($Source)
        $targetPath = [System.IO.Path]::GetFullPath($Target)
    }
    catch {
        Fail "invalid install path: $($_.Exception.Message)"
    }

    if ($sourcePath -eq $targetPath) {
        Write-Output "ASCII Forge is already installed at $targetPath"
        return
    }

    if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
        Fail "no executable was found at '$sourcePath'; run with -Build to compile from source, or pass -Binary PATH"
    }

    $temporaryPath = Join-Path (Split-Path -Parent $targetPath) (".$ExecutableName.install.$PID")
    try {
        Copy-Item -LiteralPath $sourcePath -Destination $temporaryPath -Force -ErrorAction Stop
        Move-Item -LiteralPath $temporaryPath -Destination $targetPath -Force -ErrorAction Stop
    }
    catch {
        Fail "failed to install '$sourcePath' to '$targetPath'; check that '$(Split-Path -Parent $targetPath)' is writable: $($_.Exception.Message)"
    }
    finally {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath -Force -ErrorAction SilentlyContinue
        }
    }

    Write-Output "Installed ASCII Forge to $targetPath"
}

function Uninstall-Executable {
    param([string]$Target)

    if (Test-Path -LiteralPath $Target -PathType Container) {
        Fail "refusing to remove '$Target': it is a directory, not the ascii-forge executable"
    }

    if (Test-Path -LiteralPath $Target -PathType Leaf) {
        try {
            Remove-Item -LiteralPath $Target -Force -ErrorAction Stop
        }
        catch {
            Fail "failed to remove '$Target': $($_.Exception.Message)"
        }
        Write-Output "Removed ASCII Forge from $Target"
    }
    else {
        Write-Output "ASCII Forge is not installed at $Target; nothing to remove"
    }
}

if ($Help) {
    Show-Usage
    exit 0
}

if ($Build -and $NoBuild) {
    Fail 'options -Build and -NoBuild are mutually exclusive'
}

if ($PSBoundParameters.ContainsKey('Prefix') -and [string]::IsNullOrWhiteSpace($Prefix)) {
    Fail 'option -Prefix requires a directory'
}

$Prefix = Resolve-Prefix

$executablePath = Join-Path $Prefix $ExecutableName

if ($Uninstall) {
    Uninstall-Executable -Target $executablePath
    Write-Output "Left the install directory $Prefix in place."
    exit 0
}

if (Test-Path -LiteralPath $Prefix) {
    if (-not (Test-Path -LiteralPath $Prefix -PathType Container)) {
        Fail "install prefix '$Prefix' exists and is not a directory"
    }
}
else {
    try {
        [System.IO.Directory]::CreateDirectory($Prefix) | Out-Null
    }
    catch {
        Fail "cannot create install directory '$Prefix': $($_.Exception.Message)"
    }
}

if (Test-Path -LiteralPath $executablePath -PathType Container) {
    Fail "refusing to replace directory '$executablePath'; move it aside and rerun install.ps1"
}

$sourcePath = Resolve-SourceBinary
$sourceExists = Test-Path -LiteralPath $sourcePath -PathType Leaf

if ($Build -or (-not $NoBuild -and -not $sourceExists)) {
    Invoke-Build
    $sourcePath = Resolve-SourceBinary
    $sourceExists = Test-Path -LiteralPath $sourcePath -PathType Leaf
}

if (-not $sourceExists) {
    if ($NoBuild) {
        Fail "-NoBuild was given, but no executable was found at '$sourcePath'; run with -Build to compile from source, or pass -Binary PATH or set ASCII_FORGE_BINARY to a prebuilt executable"
    }
    Fail "no executable was found at '$sourcePath'; run with -Build to compile from source, or pass -Binary PATH or set ASCII_FORGE_BINARY to a prebuilt executable"
}

Install-Executable -Source $sourcePath -Target $executablePath

Write-Output 'FFmpeg and ffprobe are runtime dependencies for video export and video/GIF import; a monospace TTF font is needed for raster output and espeak-ng is optional.'
Write-Output "Run `"$ProgramName doctor`" to check your environment."
exit 0
