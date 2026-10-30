#Requires -Version 5.1
<#
.SYNOPSIS
    Download and install the latest Octa release on Windows, without admin
    rights and without tripping the SmartScreen "downloaded .exe" prompt.

.DESCRIPTION
    Resolves a GitHub release (latest by default), downloads the Windows zip,
    verifies its SHA256 against the release SHA256SUMS, extracts
    it, and installs the binary + assets into a per-user directory. The
    mark-of-the-web is stripped from the binary (Unblock-File) so the unsigned
    exe launches without a SmartScreen warning. A Start Menu shortcut is created.

.PARAMETER Version
    Release tag to install (e.g. "1.2.3"), or "latest" (default).

.PARAMETER InstallDir
    Target directory. Default: %LOCALAPPDATA%\Programs\Octa (no admin needed).

.PARAMETER NoShortcut
    Skip creating the Start Menu shortcut.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1 -Version 1.2.3
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

# The release page, not api.github.com: the API answers 60 anonymous requests
# an hour per IP address, shared by everything on the network, and refuses the
# rest. releases/latest redirects to .../releases/tag/<version>.
function Get-LatestVersion {
    $request = [Net.WebRequest]::Create("https://github.com/$Repo/releases/latest")
    $request.AllowAutoRedirect = $false
    $request.UserAgent = $Headers["User-Agent"]
    $response = $request.GetResponse()
    try { $location = $response.GetResponseHeader("Location") } finally { $response.Close() }
    if ($location -notmatch '/releases/tag/([^/]+)$') {
        throw "Could not determine the latest release (GitHub answered '$location')."
    }
    return $Matches[1]
}

if ($Version -eq "latest") { $Version = Get-LatestVersion }
$assetName = "octa-$Version-windows-x86_64.zip"
$base = "https://github.com/$Repo/releases/download/$Version"

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("octa-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $zip = Join-Path $tmp $assetName
    Write-Host "Downloading $assetName ..."
    Invoke-WebRequest -Uri "$base/$assetName" -OutFile $zip -Headers $Headers

    Write-Host "Verifying SHA256 ..."
    $sumsFile = Join-Path $tmp "SHA256SUMS"
    Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile $sumsFile -Headers $Headers
    $line = Select-String -Path $sumsFile -Pattern ([Regex]::Escape($assetName)) | Select-Object -First 1
    if (-not $line) {
        throw "$assetName is not listed in the release's SHA256SUMS. Nothing was installed."
    }
    $expected = (($line.Line -split '\s+')[0]).ToLower()
    $actual = (Get-FileHash -Path $zip -Algorithm SHA256).Hash.ToLower()
    if ($expected -ne $actual) {
        throw "Checksum mismatch for ${assetName}: expected $expected, got $actual."
    }
    Write-Host "Checksum OK."

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
    Write-Host "Octa $Version installed."
    Write-Host "  Binary: $exe"
    Write-Host ""
    Write-Host "Tip: add '$InstallDir' to your PATH to run 'octa' from any terminal."
    Write-Host "Octa is not code-signed. The downloaded binary was unblocked, but if"
    Write-Host "SmartScreen still prompts, click 'More info' then 'Run anyway'."
}
finally {
    Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
