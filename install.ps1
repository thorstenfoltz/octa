#Requires -Version 5.1
<#
.SYNOPSIS
    Download and install the latest Octa release on Windows, without admin
    rights and without tripping the SmartScreen "downloaded .exe" prompt.

.DESCRIPTION
    Resolves a GitHub release (latest by default), downloads the Windows zip,
    verifies its SHA256 against the release SHA256SUMS (when present), extracts
    it, and installs the binary + assets into a per-user directory. The
    mark-of-the-web is stripped from the binary (Unblock-File) so the unsigned
    exe launches without a SmartScreen warning. A Start Menu shortcut is created.

.PARAMETER Version
    Release tag to install (e.g. "v1.2.3"), or "latest" (default).

.PARAMETER InstallDir
    Target directory. Default: %LOCALAPPDATA%\Programs\Octa (no admin needed).

.PARAMETER NoShortcut
    Skip creating the Start Menu shortcut.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1 -Version v1.2.3
#>
[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Octa"),
    [switch]$NoShortcut
)

$ErrorActionPreference = "Stop"
$Repo = "thorstenfoltz/octa"
$Headers = @{ "User-Agent" = "octa-installer" }
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

function Get-OctaRelease {
    param([string]$Version)
    if ($Version -eq "latest") {
        $url = "https://api.github.com/repos/$Repo/releases/latest"
    } else {
        $url = "https://api.github.com/repos/$Repo/releases/tags/$Version"
    }
    return Invoke-RestMethod -Uri $url -Headers $Headers
}

$release = Get-OctaRelease -Version $Version
$asset = $release.assets | Where-Object { $_.name -like "octa-*-windows-x86_64.zip" } | Select-Object -First 1
if (-not $asset) {
    throw "No Windows zip asset (octa-*-windows-x86_64.zip) found in release $($release.tag_name)."
}
$sumsAsset = $release.assets | Where-Object { $_.name -eq "SHA256SUMS" } | Select-Object -First 1

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("octa-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $zip = Join-Path $tmp $asset.name
    Write-Host "Downloading $($asset.name) ..."
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zip -Headers $Headers

    if ($sumsAsset) {
        Write-Host "Verifying SHA256 ..."
        $sumsFile = Join-Path $tmp "SHA256SUMS"
        Invoke-WebRequest -Uri $sumsAsset.browser_download_url -OutFile $sumsFile -Headers $Headers
        $line = Select-String -Path $sumsFile -Pattern ([Regex]::Escape($asset.name)) | Select-Object -First 1
        if ($line) {
            $expected = (($line.Line -split '\s+')[0]).ToLower()
            $actual = (Get-FileHash -Path $zip -Algorithm SHA256).Hash.ToLower()
            if ($expected -ne $actual) {
                throw "Checksum mismatch for $($asset.name): expected $expected, got $actual."
            }
            Write-Host "Checksum OK."
        } else {
            Write-Warning "No checksum entry for $($asset.name); skipping verification."
        }
    } else {
        Write-Warning "Release has no SHA256SUMS; skipping checksum verification."
    }

    $extract = Join-Path $tmp "extract"
    Expand-Archive -Path $zip -DestinationPath $extract -Force

    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir | Out-Null
    }

    Write-Host "Installing to $InstallDir ..."
    $exe = Join-Path $InstallDir "octa.exe"
    Copy-Item -Path (Join-Path $extract "octa.exe") -Destination $exe -Force
    # Strip the mark-of-the-web so the unsigned binary launches without a
    # SmartScreen "Windows protected your PC" prompt.
    Unblock-File -Path $exe

    foreach ($name in @("octa.svg", "octa.png", "octa.ico")) {
        $src = Join-Path $extract "assets\$name"
        if (Test-Path $src) { Copy-Item -Path $src -Destination (Join-Path $InstallDir $name) -Force }
    }
    foreach ($name in @("LICENSE", "THIRD_PARTY_LICENSES.md", "README.md")) {
        $src = Join-Path $extract $name
        if (Test-Path $src) { Copy-Item -Path $src -Destination (Join-Path $InstallDir $name) -Force }
    }
    $licDir = Join-Path $extract "licenses"
    if (Test-Path $licDir) { Copy-Item -Path $licDir -Destination $InstallDir -Recurse -Force }

    if (-not $NoShortcut) {
        $startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
        $ico = Join-Path $InstallDir "octa.ico"
        $icon = if (Test-Path $ico) { $ico } else { $exe }
        $ws = New-Object -ComObject WScript.Shell
        $sc = $ws.CreateShortcut((Join-Path $startMenu "Octa.lnk"))
        $sc.TargetPath = $exe
        $sc.IconLocation = $icon
        $sc.WorkingDirectory = $env:USERPROFILE
        $sc.Description = "Multi-format data viewer and editor"
        $sc.Save()
        Write-Host "Start Menu shortcut created."
    }

    Write-Host ""
    Write-Host "Octa $($release.tag_name) installed."
    Write-Host "  Binary: $exe"
    Write-Host ""
    Write-Host "Tip: add '$InstallDir' to your PATH to run 'octa' from any terminal."
    Write-Host "Octa is not code-signed. The downloaded binary was unblocked, but if"
    Write-Host "SmartScreen still prompts, click 'More info' then 'Run anyway'."
}
finally {
    Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
