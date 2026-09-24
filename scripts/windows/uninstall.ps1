# Removes everything install.ps1 added.
$ErrorActionPreference = 'Continue'
$Name = 'com.pipanywhere.host'
$Browsers = @('Google\Chrome', 'Chromium', 'Microsoft\Edge', 'BraveSoftware\Brave-Browser', 'Vivaldi')

foreach ($browser in $Browsers) {
    Remove-Item -Force -ErrorAction SilentlyContinue -Path "HKCU:\Software\$browser\NativeMessagingHosts\$Name"
}
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -Path (Join-Path $env:LOCALAPPDATA 'PipAnywhere')
Write-Host 'PiP Anywhere helper removed.'
