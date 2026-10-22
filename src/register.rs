//! Registers this app as a Windows browser.
//!
//! Windows 10/11 won't let an app silently become the default browser: we register
//! the capabilities, then the user confirms in Settings > Default apps.
//!
//! Two registrations are written:
//! * per-user (`HKCU`): no admin needed.
//! * machine-wide (`HKLM`): needs admin. In testing, Windows' Settings and "open with"
//!   lists only showed the app once this one existed, so `register()` re-launches the
//!   exe elevated (one UAC prompt) to write it.

use std::env;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_SET_VALUE};

pub const CLIENT_KEY_NAME: &str = "RustyBrowserLauncher";
pub const APP_NAME: &str = "Rusty Browser Launcher";
const PROGID: &str = "RustyBrowserLauncherHTML";
/// Left by earlier versions; removed on unregister.
const LEGACY_URL_PROGID: &str = "RustyBrowserLauncherURL";
const DESCRIPTION: &str = "Choose which browser and profile opens each link.";

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn client_path() -> String {
    format!(r"Software\Clients\StartMenuInternet\{CLIENT_KEY_NAME}")
}

fn uninstall_path() -> String {
    format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{CLIENT_KEY_NAME}")
}

fn exe_path() -> io::Result<String> {
    Ok(env::current_exe()?.to_string_lossy().into_owned())
}

fn start_menu_shortcut() -> Option<PathBuf> {
    let appdata = env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join(format!(r"Microsoft\Windows\Start Menu\Programs\{APP_NAME}.lnk")))
}

/// Writes the ProgID, client key, capabilities and RegisteredApplications entry under `root`
/// (`HKCU` or `HKLM`; both use the same `Software\...` layout).
fn write_registration(root: &RegKey, exe: &str) -> io::Result<()> {
    let icon = format!("{exe},0");
    let set = |path: &str, values: &[(&str, &str)]| -> io::Result<()> {
        let (key, _) = root.create_subkey(path)?;
        for (name, value) in values {
            key.set_value(name, value)?;
        }
        Ok(())
    };

    let progid = format!(r"Software\Classes\{PROGID}");
    set(
        &progid,
        &[
            ("", &format!("{APP_NAME} Document")),
            ("FriendlyTypeName", APP_NAME),
            ("AppUserModelId", CLIENT_KEY_NAME),
        ],
    )?;
    set(
        &format!(r"{progid}\Application"),
        &[
            ("ApplicationName", APP_NAME),
            ("ApplicationDescription", DESCRIPTION),
            ("ApplicationIcon", &icon),
            ("ApplicationCompany", "Rusty"),
            ("AppUserModelId", CLIENT_KEY_NAME),
        ],
    )?;
    set(&format!(r"{progid}\DefaultIcon"), &[("", &icon)])?;
    set(&format!(r"{progid}\shell\open\command"), &[("", &format!("\"{exe}\" \"%1\""))])?;

    let client = client_path();
    set(&client, &[("", APP_NAME)])?;
    set(&format!(r"{client}\DefaultIcon"), &[("", &icon)])?;
    set(&format!(r"{client}\shell\open\command"), &[("", &format!("\"{exe}\""))])?;
    set(
        &format!(r"{client}\InstallInfo"),
        &[
            ("ReinstallCommand", &format!("\"{exe}\" --register")),
            ("HideIconsCommand", &format!("\"{exe}\" --hide-icons")),
            ("ShowIconsCommand", &format!("\"{exe}\" --show-icons")),
        ],
    )?;
    let (info, _) = root.create_subkey(format!(r"{client}\InstallInfo"))?;
    info.set_value("IconsVisible", &1u32)?;

    let caps = format!(r"{client}\Capabilities");
    set(
        &caps,
        &[("ApplicationName", APP_NAME), ("ApplicationDescription", DESCRIPTION), ("ApplicationIcon", &icon)],
    )?;
    set(&format!(r"{caps}\Startmenu"), &[("StartMenuInternet", CLIENT_KEY_NAME)])?;
    set(&format!(r"{caps}\URLAssociations"), &[("http", PROGID), ("https", PROGID)])?;
    set(
        &format!(r"{caps}\FileAssociations"),
        &[(".htm", PROGID), (".html", PROGID), (".xhtml", PROGID), (".shtml", PROGID)],
    )?;

    set(
        r"Software\RegisteredApplications",
        &[(CLIENT_KEY_NAME, &format!(r"{client}\Capabilities"))],
    )
}

fn remove_registration(root: &RegKey) -> io::Result<()> {
    let ignore_missing = |r: io::Result<()>| match r {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    };
    ignore_missing(root.delete_subkey_all(client_path()))?;
    ignore_missing(root.delete_subkey_all(format!(r"Software\Classes\{PROGID}")))?;
    ignore_missing(root.delete_subkey_all(format!(r"Software\Classes\{LEGACY_URL_PROGID}")))?;
    if let Ok(k) = root.open_subkey_with_flags(r"Software\RegisteredApplications", KEY_SET_VALUE) {
        ignore_missing(k.delete_value(CLIENT_KEY_NAME))?;
    }
    Ok(())
}

/// Adds the app to "Installed apps" and the Start menu; Windows' default-app lists
/// are built around installed apps.
fn write_install_footprint(exe: &str) -> io::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(uninstall_path())?;
    key.set_value("DisplayName", &APP_NAME)?;
    key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))?;
    key.set_value("Publisher", &"Rusty")?;
    key.set_value("DisplayIcon", &format!("{exe},0"))?;
    if let Some(dir) = std::path::Path::new(exe).parent() {
        key.set_value("InstallLocation", &dir.to_string_lossy().into_owned())?;
    }
    key.set_value("UninstallString", &format!("\"{exe}\" --unregister"))?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;

    if let Some(lnk) = start_menu_shortcut() {
        let q = |s: &str| s.replace('\'', "''");
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.IconLocation='{},0';$s.Description='{}';$s.Save()",
            q(&lnk.to_string_lossy()),
            q(exe),
            q(exe),
            APP_NAME,
        );
        let _ = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    Ok(())
}

fn remove_install_footprint() {
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(uninstall_path());
    if let Some(lnk) = start_menu_shortcut() {
        let _ = std::fs::remove_file(lnk);
    }
}

/// True when the machine-wide registration exists and points at this exe.
fn machine_registered(exe: &str) -> bool {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(format!(r"{}\shell\open\command", client_path()))
        .and_then(|k| k.get_value::<String, _>(""))
        .is_ok_and(|cmd| cmd.trim_matches('"').eq_ignore_ascii_case(exe))
}

fn machine_present() -> bool {
    RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(client_path()).is_ok()
}

/// Per-user registration, then (if needed) one UAC prompt for the machine-wide one.
pub fn register() -> io::Result<()> {
    let exe = exe_path()?;
    write_registration(&RegKey::predef(HKEY_CURRENT_USER), &exe)?;
    write_install_footprint(&exe)?;
    if !machine_registered(&exe) {
        run_elevated("--register-machine")?;
    }
    notify_assoc_changed();
    Ok(())
}

pub fn unregister() -> io::Result<()> {
    remove_registration(&RegKey::predef(HKEY_CURRENT_USER))?;
    remove_install_footprint();
    if machine_present() {
        run_elevated("--unregister-machine")?;
    }
    notify_assoc_changed();
    Ok(())
}

/// Entry points for the elevated child process (`--register-machine` / `--unregister-machine`).
pub fn register_machine() -> io::Result<()> {
    write_registration(&RegKey::predef(HKEY_LOCAL_MACHINE), &exe_path()?)?;
    notify_assoc_changed();
    Ok(())
}

pub fn unregister_machine() -> io::Result<()> {
    remove_registration(&RegKey::predef(HKEY_LOCAL_MACHINE))?;
    notify_assoc_changed();
    Ok(())
}

/// Re-launches this exe with `arg` under a UAC prompt and waits for it to finish.
fn run_elevated(arg: &str) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain([0]).collect::<Vec<u16>>();
    let verb = wide("runas".as_ref());
    let file = wide(env::current_exe()?.as_os_str());
    let params = wide(arg.as_ref());

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.lpDirectory = null();
    info.nShow = SW_HIDE;

    unsafe {
        if ShellExecuteExW(&mut info) == 0 {
            // ERROR_CANCELLED (1223) when the user declines the UAC prompt.
            return Err(io::Error::last_os_error());
        }
        let mut code = 1u32;
        if !info.hProcess.is_null() {
            WaitForSingleObject(info.hProcess, INFINITE);
            GetExitCodeProcess(info.hProcess, &mut code);
            CloseHandle(info.hProcess);
        }
        if code == 0 {
            Ok(())
        } else {
            Err(io::Error::other(format!("administrator step failed (exit code {code})")))
        }
    }
}

/// Tells the shell the association tables changed so Settings lists the app without a sign-out.
fn notify_assoc_changed() {
    use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, std::ptr::null(), std::ptr::null()) };
}

/// True when Windows' current https handler is this app.
pub fn is_default() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice")
        .and_then(|k| k.get_value::<String, _>("ProgId"))
        .is_ok_and(|id| id == PROGID || id == LEGACY_URL_PROGID)
}

/// Opens Windows' Default apps settings, targeting this app's page when the OS supports it.
pub fn open_default_apps_settings() {
    let uri = format!("ms-settings:defaultapps?registeredAppUser={}", APP_NAME.replace(' ', "%20"));
    let _ = Command::new("explorer.exe").arg(uri).creation_flags(CREATE_NO_WINDOW).spawn();
}
