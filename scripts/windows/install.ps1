# Installs the PiP Anywhere helper for the current user (no administrator rights needed):
# copies the native messaging host and registers it for Chromium-based browsers.
param(
    [string]$HostExe = (Join-Path $PSScriptRoot 'pip-anywhere-host.exe')
)
$ErrorActionPreference = 'Stop'

$Name = 'com.pipanywhere.host'
# Fixed by the "key" in extension/manifest.json.
$ExtensionId = 'imcnedckfcpfibpcdjpcjgjaichejlee'
$Dir = Join-Path $env:LOCALAPPDATA 'PipAnywhere'
$Browsers = @('Google\Chrome', 'Chromium', 'Microsoft\Edge', 'BraveSoftware\Brave-Browser', 'Vivaldi')

if (-not (Test-Path $HostExe)) { throw "pip-anywhere-host.exe not found next to this script ($HostExe)." }

New-Item -ItemType Directory -Force -Path $Dir | Out-Null
$InstalledExe = Join-Path $Dir 'pip-anywhere-host.exe'
Copy-Item -Force $HostExe $InstalledExe

$ManifestPath = Join-Path $Dir "$Name.json"
$Manifest = [ordered]@{
    name            = $Name
    description     = 'PiP Anywhere helper: keeps the browser window on top and changes its opacity'
    path            = $InstalledExe
    type            = 'stdio'
    allowed_origins = @("chrome-extension://$ExtensionId/")
} | ConvertTo-Json
# UTF-8 without a byte order mark.
[System.IO.File]::WriteAllText($ManifestPath, $Manifest, [System.Text.UTF8Encoding]::new($false))

foreach ($browser in $Browsers) {
    $Key = "HKCU:\Software\$browser\NativeMessagingHosts\$Name"
    New-Item -Force -Path $Key | Out-Null
    Set-ItemProperty -Path $Key -Name '(Default)' -Value $ManifestPath
}

Write-Host "PiP Anywhere helper installed to $Dir"
Write-Host 'Registered for: Chrome, Chromium, Edge, Brave, Vivaldi.'
Write-Host 'Now load the extension (chrome://extensions > Developer mode > Load unpacked).'
