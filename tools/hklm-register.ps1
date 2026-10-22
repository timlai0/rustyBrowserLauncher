# Registers Rusty Browser Launcher machine-wide (HKLM). Run from an elevated PowerShell:
#   pwsh -ExecutionPolicy Bypass -File C:\Users\Public\rbl-hklm.ps1
# Undo: pwsh -ExecutionPolicy Bypass -File C:\Users\Public\rbl-hklm.ps1 -Undo
param([switch]$Undo)
$ErrorActionPreference = 'Stop'

$exe    = Join-Path $env:LOCALAPPDATA 'Programs\RustyBrowserLauncher\rusty-browser-launcher.exe'
$name   = 'Rusty Browser Launcher'
$client = 'HKLM:\SOFTWARE\Clients\StartMenuInternet\RustyBrowserLauncher'
$htmlId = 'RustyBrowserLauncherHTML'
$urlId  = 'RustyBrowserLauncherURL'

if ($Undo) {
    Remove-Item $client -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item "HKLM:\SOFTWARE\Classes\$htmlId" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item "HKLM:\SOFTWARE\Classes\$urlId" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-ItemProperty 'HKLM:\SOFTWARE\RegisteredApplications' -Name RustyBrowserLauncher -ErrorAction SilentlyContinue
    'Removed.'; return
}

if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Not elevated: start PowerShell with "Run as administrator".'
}
if (-not (Test-Path $exe)) { throw "Exe not found (is this machine/user path right?): $exe" }

function Set-Key($path, $values) {
    New-Item $path -Force | Out-Null
    foreach ($k in $values.Keys) {
        $n = if ($k -eq '(default)') { '(default)' } else { $k }
        Set-ItemProperty -Path $path -Name $n -Value $values[$k]
    }
}

$icon = "$exe,0"
foreach ($id in $htmlId, $urlId) {
    $p = "HKLM:\SOFTWARE\Classes\$id"
    Set-Key $p @{ '(default)' = "$name Document"; FriendlyTypeName = $name; AppUserModelId = 'RustyBrowserLauncher' }
    Set-Key "$p\Application" @{ ApplicationName = $name; ApplicationDescription = 'Choose which browser and profile opens each link.'; ApplicationIcon = $icon; ApplicationCompany = 'Rusty'; AppUserModelId = 'RustyBrowserLauncher' }
    Set-Key "$p\DefaultIcon" @{ '(default)' = $icon }
    Set-Key "$p\shell\open\command" @{ '(default)' = "`"$exe`" `"%1`"" }
}

Set-Key $client @{ '(default)' = $name }
Set-Key "$client\DefaultIcon" @{ '(default)' = $icon }
Set-Key "$client\shell\open\command" @{ '(default)' = "`"$exe`"" }
Set-Key "$client\InstallInfo" @{ ReinstallCommand = "`"$exe`" --register"; HideIconsCommand = "`"$exe`" --hide-icons"; ShowIconsCommand = "`"$exe`" --show-icons" }
New-ItemProperty "$client\InstallInfo" -Name IconsVisible -Value 1 -PropertyType DWord -Force | Out-Null
Set-Key "$client\Capabilities" @{ ApplicationName = $name; ApplicationDescription = 'Choose which browser and profile opens each link.'; ApplicationIcon = $icon }
Set-Key "$client\Capabilities\Startmenu" @{ StartMenuInternet = 'RustyBrowserLauncher' }
Set-Key "$client\Capabilities\URLAssociations" @{ http = $htmlId; https = $htmlId }
Set-Key "$client\Capabilities\FileAssociations" @{ '.htm' = $htmlId; '.html' = $htmlId; '.xhtml' = $htmlId; '.shtml' = $htmlId }

New-Item 'HKLM:\SOFTWARE\RegisteredApplications' -Force | Out-Null
Set-ItemProperty 'HKLM:\SOFTWARE\RegisteredApplications' -Name RustyBrowserLauncher -Value 'SOFTWARE\Clients\StartMenuInternet\RustyBrowserLauncher\Capabilities'

Add-Type -Namespace N -Name S9 -MemberDefinition '[DllImport("shell32.dll")] public static extern void SHChangeNotify(int e, uint f, IntPtr a, IntPtr b);'
[N.S9]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
'Registered machine-wide. Now check Settings > Apps > Default apps.'
