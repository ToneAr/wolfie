#!/usr/bin/env pwsh
param(
    [string]$InstallDir = $env:INSTALL_DIR,
    [string]$Version = $env:VERSION,
    [switch]$BuildFromSource,
    [switch]$Force,
    [switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$BinaryName = 'wolfram-cli'
$ExecutableName = "$BinaryName.exe"
$GitHubRepo = if ([string]::IsNullOrWhiteSpace($env:GITHUB_REPO)) { 'ToneAr/wolfram-cli' } else { $env:GITHUB_REPO }

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = 'latest'
}

function Show-Usage {
    @"
Install wolfram-cli on Windows.

Usage:
  .\install.ps1 [options]

Options:
  -InstallDir DIR    Install the binary into DIR.
                     Defaults to `%LOCALAPPDATA%\Programs\wolfram-cli\bin,
                     unless `%USERPROFILE%\.local\bin is writable and already on PATH.
  -Version TAG       Install a specific GitHub release tag, such as v0.2.0.
                     Defaults to the latest release.
  -BuildFromSource   Build this checkout with cargo and install the result.
  -Force             Replace an existing binary at the destination.
  -Help              Show this help.

Environment:
  INSTALL_DIR         Same as -InstallDir.
  VERSION             Same as -Version.
  GITHUB_REPO         GitHub repo to download from. Defaults to ToneAr/wolfram-cli.
  WOLFRAM_CLI_SHA256  Optional expected SHA-256 checksum for the release archive.
"@
}

function Write-Log {
    param([string]$Message)
    Write-Host $Message
}

function Fail {
    param([string]$Message)
    throw "install.ps1: $Message"
}

function Test-HasCommand {
    param([string]$Command)
    $null -ne (Get-Command $Command -ErrorAction SilentlyContinue)
}

function Require-Command {
    param([string]$Command)
    if (-not (Test-HasCommand $Command)) {
        Fail "required command not found: $Command"
    }
}

function Test-IsWindows {
    $isWindowsVariable = Get-Variable -Name IsWindows -Scope Global -ErrorAction SilentlyContinue
    if ($null -ne $isWindowsVariable) {
        return [bool]$isWindowsVariable.Value
    }

    return $env:OS -eq 'Windows_NT'
}

function Normalize-PathEntry {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return ''
    }

    try {
        return [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    } catch {
        return $Path.TrimEnd('\', '/')
    }
}

function Test-PathContains {
    param([string]$Directory)

    $normalizedDirectory = Normalize-PathEntry $Directory
    foreach ($entry in (($env:PATH -split ';') | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })) {
        if ((Normalize-PathEntry $entry) -ieq $normalizedDirectory) {
            return $true
        }
    }

    return $false
}

function Test-WritableDirectory {
    param([string]$Directory)

    if (-not (Test-Path -LiteralPath $Directory -PathType Container)) {
        return $false
    }

    $probe = Join-Path $Directory ".wolfram-cli-install-test-$PID"
    try {
        New-Item -ItemType File -Path $probe -Force | Out-Null
        Remove-Item -LiteralPath $probe -Force
        return $true
    } catch {
        return $false
    }
}

function Get-DefaultInstallDir {
    $homeDirectory = if ([string]::IsNullOrWhiteSpace($HOME)) { $env:USERPROFILE } else { $HOME }
    if ([string]::IsNullOrWhiteSpace($homeDirectory)) {
        Fail 'HOME or USERPROFILE is not set'
    }

    $localBin = Join-Path $homeDirectory '.local\bin'
    if ((Test-PathContains $localBin) -and (Test-WritableDirectory $localBin)) {
        return $localBin
    }

    $localAppData = $env:LOCALAPPDATA
    if ([string]::IsNullOrWhiteSpace($localAppData)) {
        $localAppData = Join-Path $homeDirectory 'AppData\Local'
    }

    return (Join-Path $localAppData 'Programs\wolfram-cli\bin')
}

function Get-ReleaseTargetName {
    if (-not (Test-IsWindows)) {
        Fail 'unsupported operating system: this installer is for Windows'
    }

    if (-not [Environment]::Is64BitOperatingSystem) {
        Fail 'unsupported CPU architecture: wolfram-cli release builds are only available for Windows x86_64'
    }

    return 'windows-x86_64'
}

function Invoke-DownloadFile {
    param(
        [string]$Url,
        [string]$OutputPath
    )

    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri $Url -OutFile $OutputPath -UseBasicParsing
}

function Test-Sha256 {
    param(
        [string]$FilePath,
        [string]$ExpectedHash
    )

    if ([string]::IsNullOrWhiteSpace($ExpectedHash)) {
        return
    }

    $actualHash = (Get-FileHash -LiteralPath $FilePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $ExpectedHash.ToLowerInvariant()) {
        Fail 'checksum mismatch for downloaded archive'
    }
}

function Install-Binary {
    param(
        [string]$SourcePath,
        [string]$DestinationPath
    )

    if (-not (Test-Path -LiteralPath $SourcePath -PathType Leaf)) {
        Fail "binary not found at $SourcePath"
    }

    if (Test-Path -LiteralPath $DestinationPath -PathType Container) {
        Fail "$DestinationPath already exists as a directory"
    }

    if ((Test-Path -LiteralPath $DestinationPath -PathType Leaf) -and (-not $Force)) {
        Fail "$DestinationPath already exists; rerun with -Force to replace it"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $temporaryDestination = "$DestinationPath.tmp.$PID"

    try {
        Copy-Item -LiteralPath $SourcePath -Destination $temporaryDestination -Force
        if (Test-Path -LiteralPath $DestinationPath -PathType Leaf) {
            Remove-Item -LiteralPath $DestinationPath -Force
        }
        Move-Item -LiteralPath $temporaryDestination -Destination $DestinationPath
    } catch {
        if (Test-Path -LiteralPath $temporaryDestination) {
            Remove-Item -LiteralPath $temporaryDestination -Force -ErrorAction SilentlyContinue
        }
        throw
    }
}

function Install-Release {
    $target = Get-ReleaseTargetName
    $package = "$BinaryName-$target"
    $archive = "$package.zip"

    if ($Version -eq 'latest') {
        $url = "https://github.com/$GitHubRepo/releases/latest/download/$archive"
    } else {
        $url = "https://github.com/$GitHubRepo/releases/download/$Version/$archive"
    }

    $temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) "$BinaryName-install-$([Guid]::NewGuid())"
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null

    try {
        $archivePath = Join-Path $temporaryDirectory $archive
        Write-Log "Downloading $url"
        Invoke-DownloadFile -Url $url -OutputPath $archivePath
        Test-Sha256 -FilePath $archivePath -ExpectedHash $env:WOLFRAM_CLI_SHA256

        Expand-Archive -LiteralPath $archivePath -DestinationPath $temporaryDirectory -Force
        $binaryPath = Join-Path (Join-Path $temporaryDirectory $package) $ExecutableName

        if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
            $binary = Get-ChildItem -Path $temporaryDirectory -Recurse -File -Filter $ExecutableName | Select-Object -First 1
            if ($null -eq $binary) {
                Fail "archive did not contain $ExecutableName"
            }
            $binaryPath = $binary.FullName
        }

        Install-Binary -SourcePath $binaryPath -DestinationPath (Join-Path $InstallDir $ExecutableName)
    } finally {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Install-FromSource {
    Require-Command 'cargo'

    $scriptDirectory = if (-not [string]::IsNullOrWhiteSpace($PSScriptRoot)) { $PSScriptRoot } else { (Get-Location).Path }
    if (-not (Test-Path -LiteralPath (Join-Path $scriptDirectory 'Cargo.toml') -PathType Leaf)) {
        Fail '-BuildFromSource must be run from a source checkout'
    }

    Write-Log "Building $BinaryName from source"
    Push-Location $scriptDirectory
    try {
        & cargo build --release --locked
        if ($LASTEXITCODE -ne 0) {
            Fail "cargo build failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }

    Install-Binary -SourcePath (Join-Path $scriptDirectory "target\release\$ExecutableName") -DestinationPath (Join-Path $InstallDir $ExecutableName)
}

if ($Help) {
    Show-Usage
    exit 0
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Get-DefaultInstallDir
}

if (-not [System.IO.Path]::IsPathRooted($InstallDir)) {
    Fail '-InstallDir must be an absolute path'
}

$destination = Join-Path $InstallDir $ExecutableName

if ($BuildFromSource) {
    Install-FromSource
} else {
    Install-Release
}

Write-Log "Installed $BinaryName to $destination"

if (-not (Test-PathContains $InstallDir)) {
    Write-Log "Add $InstallDir to PATH to run $BinaryName without a full path."
}
