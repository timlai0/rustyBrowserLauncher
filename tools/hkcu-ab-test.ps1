param([switch]$Cleanup)
$ErrorActionPreference = 'Stop'
$dir  = Join-Path $env:LOCALAPPDATA 'Programs\RustyBrowserLauncher'
$src  = "$dir\rusty-browser-launcher.exe"
$prog = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'

$tests = @(
    @{ Id = 'A'; Name = 'Rusty Test A'; Client = 'RustyTestA';        ProgId = 'RustyTestAHTML';       Exe = "$dir\rbl-test-a.exe"; Extras = $false },
    @{ Id = 'B'; Name = 'Rusty Test B'; Client = 'RustyTestB.0A1B2C'; ProgId = 'RustyTestBHTM.0A1B2C'; Exe = "$dir\rbl-test-b.exe"; Extras = $true  }
)

function Remove-Test($t) {
    $exeName = Split-Path $t.Exe -Leaf
    Remove-Item "HKCU:\Software\Clients\StartMenuInternet\$($t.Client)" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item "HKCU:\Software\Classes\$($t.ProgId)" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-ItemProperty 'HKCU:\Software\RegisteredApplications' -Name $t.Client -ErrorAction SilentlyContinue
    Remove-Item "HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths\$exeName" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item "HKCU:\Software\Classes\Applications\$exeName" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item "HKCU:\Software\Classes\AppUserModelId\$($t.Client)" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$($t.Client)" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $prog "$($t.Name).lnk") -Force -ErrorAction SilentlyContinue
    Remove-Item $t.Exe -Force -ErrorAction SilentlyContinue
}

Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)] public struct PROPERTYKEY2 { public Guid fmtid; public uint pid; }
[StructLayout(LayoutKind.Sequential)] public struct PROPVARIANT2 { public ushort vt, r1, r2, r3; public IntPtr p; public IntPtr p2; }
[ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IPropertyStore2 {
  [PreserveSig] int GetCount(out uint c);
  [PreserveSig] int GetAt(uint i, out PROPERTYKEY2 k);
  [PreserveSig] int GetValue(ref PROPERTYKEY2 k, out PROPVARIANT2 v);
  [PreserveSig] int SetValue(ref PROPERTYKEY2 k, ref PROPVARIANT2 v);
  [PreserveSig] int Commit();
}
public static class Aumid2 {
  [DllImport("shell32.dll", CharSet=CharSet.Unicode)] static extern int SHGetPropertyStoreFromParsingName(string path, IntPtr bc, int flags, ref Guid riid, out IPropertyStore2 ps);
  public static int Set(string lnk, string id) {
    var iid = new Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"); IPropertyStore2 ps;
    int hr = SHGetPropertyStoreFromParsingName(lnk, IntPtr.Zero, 2, ref iid, out ps); if (hr != 0) return hr;
    var key = new PROPERTYKEY2 { fmtid = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), pid = 5 };
    var v = new PROPVARIANT2 { vt = 31, p = Marshal.StringToCoTaskMemUni(id) };
    hr = ps.SetValue(ref key, ref v); if (hr != 0) return hr;
    return ps.Commit();
  }
}
'@

function Set-Key($path, $values) {
    New-Item $path -Force | Out-Null
    foreach ($k in $values.Keys) { Set-ItemProperty -Path $path -Name $k -Value $values[$k] }
}

foreach ($t in $tests) {
    Remove-Test $t
    if ($Cleanup) { continue }

    Copy-Item $src $t.Exe -Force
    $exe = $t.Exe; $icon = "$exe,0"; $c = "HKCU:\Software\Clients\StartMenuInternet\$($t.Client)"
    $p = "HKCU:\Software\Classes\$($t.ProgId)"
    $aumid = $t.Client

    if ($t.Extras) {
        Set-Key $p @{ '(default)' = "$($t.Name) HTML Document"; AppUserModelId = $aumid }
        Set-Key "$p\Application" @{ AppUserModelId = $aumid; ApplicationIcon = $icon; ApplicationName = $t.Name; ApplicationDescription = 'Internet'; ApplicationCompany = 'Rusty' }
    } else {
        Set-Key $p @{ '(default)' = "$($t.Name) HTML Document"; FriendlyTypeName = $t.Name }
    }
    Set-Key "$p\DefaultIcon" @{ '(default)' = $icon }
    Set-Key "$p\shell\open\command" @{ '(default)' = "`"$exe`" `"%1`"" }

    Set-Key $c @{ '(default)' = $t.Name }
    Set-Key "$c\DefaultIcon" @{ '(default)' = $icon }
    Set-Key "$c\shell\open\command" @{ '(default)' = "`"$exe`"" }
    Set-Key "$c\Capabilities" @{ ApplicationName = $t.Name; ApplicationDescription = 'Test registration'; ApplicationIcon = $icon }
    Set-Key "$c\Capabilities\Startmenu" @{ StartMenuInternet = $t.Client }
    $urls = if ($t.Extras) { 'http','https','irc','mailto','mms','news','nntp','sms','smsto','snews','tel','urn','webcal' } else { 'http','https' }
    $u = @{}; foreach ($s in $urls) { $u[$s] = $t.ProgId }
    Set-Key "$c\Capabilities\URLAssociations" $u
    $f = @{}; foreach ($e in '.htm','.html','.shtml','.xht','.xhtml') { $f[$e] = $t.ProgId }
    Set-Key "$c\Capabilities\FileAssociations" $f
    New-Item 'HKCU:\Software\RegisteredApplications' -Force | Out-Null
    Set-ItemProperty 'HKCU:\Software\RegisteredApplications' -Name $t.Client -Value "Software\Clients\StartMenuInternet\$($t.Client)\Capabilities"

    if ($t.Extras) {
        Set-Key "$c\InstallInfo" @{ ReinstallCommand = "`"$exe`" --register"; HideIconsCommand = "`"$exe`" --hide-icons"; ShowIconsCommand = "`"$exe`" --show-icons" }
        New-ItemProperty "$c\InstallInfo" -Name IconsVisible -Value 1 -PropertyType DWord -Force | Out-Null
        $exeName = Split-Path $exe -Leaf
        Set-Key "HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths\$exeName" @{ '(default)' = $exe; Path = $dir }
        Set-Key "HKCU:\Software\Classes\Applications\$exeName" @{ FriendlyAppName = $t.Name }
        Set-Key "HKCU:\Software\Classes\Applications\$exeName\shell\open\command" @{ '(default)' = "`"$exe`" `"%1`"" }
        Set-Key "HKCU:\Software\Classes\AppUserModelId\$aumid" @{ DisplayName = $t.Name; IconUri = $exe }
        Set-Key "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$($t.Client)" @{ DisplayName = $t.Name; Publisher = 'Rusty'; DisplayVersion = '0.1.0'; Version = '0.1.0'; DisplayIcon = $icon; InstallLocation = $dir; UninstallString = "`"$exe`" --unregister"; InstallDate = (Get-Date -Format yyyyMMdd) }
        $lnk = Join-Path $prog "$($t.Name).lnk"
        $s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
        $s.TargetPath = $exe; $s.IconLocation = $icon; $s.Save()
        [Aumid2]::Set($lnk, $aumid) | Out-Null
    }
}

Add-Type -Namespace N -Name SB -MemberDefinition '[DllImport("shell32.dll")] public static extern void SHChangeNotify(int e, uint f, IntPtr a, IntPtr b);'
[N.SB]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
if (-not $Cleanup) {
    Get-Process SystemSettings -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep 1; Start-Process explorer.exe 'ms-settings:defaultapps'
}
if ($Cleanup) { 'cleaned' } else { 'test registrations created' }
