//! Discovers installed browsers (via the Windows registry) and their profiles.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use crate::register::CLIENT_KEY_NAME;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    /// Chrome, Edge, Brave, Vivaldi, ... — `--profile-directory=<dir>`
    Chromium,
    /// Firefox — `-P <name>`
    Firefox,
    /// Anything else; launched without profile selection.
    Other,
}

#[derive(Clone, Debug)]
pub struct Profile {
    /// Chromium: directory name ("Default", "Profile 1"). Firefox: profile name.
    pub id: String,
    pub label: String,
    /// Account e-mail, "default profile", etc.
    pub detail: String,
    /// Firefox only: profile directory, used to detect a running instance.
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct Browser {
    pub name: String,
    pub exe: PathBuf,
    pub family: Family,
    pub profiles: Vec<Profile>,
}

pub fn scan() -> Vec<Browser> {
    let own_exe = env::current_exe().ok();
    let mut seen = HashSet::new();
    let mut browsers = Vec::new();

    let roots = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Clients\StartMenuInternet"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Clients\StartMenuInternet"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Clients\StartMenuInternet"),
    ];

    for (hive, path) in roots {
        let Ok(root) = RegKey::predef(hive).open_subkey(path) else { continue };
        for key_name in root.enum_keys().flatten() {
            if key_name == CLIENT_KEY_NAME {
                continue;
            }
            let Ok(sub) = root.open_subkey(&key_name) else { continue };
            let Ok(cmd_key) = sub.open_subkey(r"shell\open\command") else { continue };
            let Ok(cmd) = cmd_key.get_value::<String, _>("") else { continue };
            let Some(exe) = exe_from_command(&cmd) else { continue };
            if !exe.is_file() {
                continue;
            }
            let stem = exe_stem(&exe);
            if stem == "iexplore" || own_exe.as_deref().is_some_and(|o| same_file(o, &exe)) {
                continue;
            }
            if !seen.insert(exe.to_string_lossy().to_lowercase()) {
                continue;
            }
            let name = sub
                .get_value::<String, _>("")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| key_name.clone());

            let family = family_of(&stem);
            let profiles = match family {
                Family::Chromium => chromium_profiles(&exe),
                Family::Firefox => firefox_profiles(&stem),
                Family::Other => Vec::new(),
            };
            browsers.push(Browser { name, exe, family, profiles });
        }
    }

    browsers.sort_by_key(|b| b.name.to_lowercase());
    browsers
}

fn family_of(stem: &str) -> Family {
    match stem {
        "chrome" | "msedge" | "brave" | "vivaldi" | "chromium" | "thorium" => Family::Chromium,
        "firefox" | "floorp" | "waterfox" | "librewolf" | "zen" => Family::Firefox,
        _ => Family::Other,
    }
}

fn exe_stem(exe: &Path) -> String {
    exe.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Extracts the executable path from a registry command line like
/// `"C:\Program Files\X\x.exe" --flag` or `C:\X\x.exe`.
fn exe_from_command(cmd: &str) -> Option<PathBuf> {
    let cmd = expand_env(cmd.trim());
    let path = if let Some(rest) = cmd.strip_prefix('"') {
        rest.split('"').next()?.to_string()
    } else {
        let lower = cmd.to_lowercase();
        match lower.find(".exe") {
            Some(i) => cmd[..i + 4].to_string(),
            None => cmd.clone(),
        }
    };
    if path.is_empty() { None } else { Some(PathBuf::from(path)) }
}

/// Expands `%VAR%` references.
fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                match env::var(&after[..end]) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => out.push_str(&rest[start..start + end + 2]),
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------- Chromium

/// Finds the "User Data" directory for a Chromium-based browser. Installs live in
/// `...\<Vendor>\<Product>\Application\<exe>` and keep data in
/// `%LOCALAPPDATA%\<Vendor>\<Product>\User Data` (or `<Product>\User Data` with no vendor).
fn chromium_user_data(exe: &Path) -> Option<PathBuf> {
    let local = PathBuf::from(env::var_os("LOCALAPPDATA")?);
    let app_dir = exe.ancestors().find(|a| a.file_name().is_some_and(|n| n.eq_ignore_ascii_case("Application")))?;
    let product = app_dir.parent()?;
    let vendor = product.parent();

    let mut candidates = Vec::new();
    if let (Some(v), Some(p)) = (vendor.and_then(|v| v.file_name()), product.file_name()) {
        candidates.push(local.join(v).join(p).join("User Data"));
    }
    if let Some(p) = product.file_name() {
        candidates.push(local.join(p).join("User Data"));
    }
    candidates.into_iter().find(|c| c.join("Local State").is_file())
}
fn chromium_profiles(exe: &Path) -> Vec<Profile> {
    let Some(dir) = chromium_user_data(exe) else { return Vec::new() };
    let Ok(text) = fs::read_to_string(dir.join("Local State")) else { return Vec::new() };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
    let Some(cache) = json["profile"]["info_cache"].as_object() else { return Vec::new() };

    let mut profiles: Vec<Profile> = cache
        .iter()
        .filter(|(id, _)| dir.join(id).is_dir())
        .map(|(id, info)| {
            let name = info["name"].as_str().filter(|s| !s.is_empty()).unwrap_or(id);
            let detail = info["user_name"].as_str().unwrap_or("").to_string();
            Profile { id: id.clone(), label: name.to_string(), detail, path: None }
        })
        .collect();

    // "Default" first, then "Profile 1", "Profile 2", ... numerically.
    profiles.sort_by_key(|p| {
        let n = p.id.strip_prefix("Profile ").and_then(|n| n.parse::<u32>().ok());
        (p.id != "Default", n.unwrap_or(u32::MAX), p.id.clone())
    });
    profiles
}

// ----------------------------------------------------------------- Firefox

fn firefox_profiles(stem: &str) -> Vec<Profile> {
    let rel = match stem {
        "firefox" => r"Mozilla\Firefox",
        "floorp" => "Floorp",
        "waterfox" => "Waterfox",
        "librewolf" => "librewolf",
        "zen" => "zen",
        _ => return Vec::new(),
    };
    let Some(appdata) = env::var_os("APPDATA") else { return Vec::new() };
    let base = PathBuf::from(appdata).join(rel);
    let Ok(ini) = fs::read_to_string(base.join("profiles.ini")) else { return Vec::new() };

    struct Raw {
        name: String,
        path: String,
        relative: bool,
        default_flag: bool,
    }
    let mut raws: Vec<Raw> = Vec::new();
    let mut install_defaults: Vec<String> = Vec::new();
    let mut section = String::new();
    let mut cur: Option<Raw> = None;

    let flush = |cur: &mut Option<Raw>, raws: &mut Vec<Raw>| {
        if let Some(r) = cur.take() {
            if !r.name.is_empty() && !r.path.is_empty() {
                raws.push(r);
            }
        }
    };

    for line in ini.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            flush(&mut cur, &mut raws);
            section = line[1..line.len() - 1].to_string();
            if section.strip_prefix("Profile").is_some_and(|n| n.parse::<u32>().is_ok()) {
                cur = Some(Raw {
                    name: String::new(),
                    path: String::new(),
                    relative: true,
                    default_flag: false,
                });
            }
        } else if let Some((k, v)) = line.split_once('=') {
            if let Some(r) = cur.as_mut() {
                match k {
                    "Name" => r.name = v.to_string(),
                    "Path" => r.path = v.to_string(),
                    "IsRelative" => r.relative = v != "0",
                    "Default" => r.default_flag = v == "1",
                    _ => {}
                }
            } else if section.starts_with("Install") && k == "Default" {
                install_defaults.push(v.to_string());
            }
        }
    }
    flush(&mut cur, &mut raws);

    raws.into_iter()
        .filter_map(|r| {
            let path = if r.relative { base.join(r.path.replace('/', "\\")) } else { PathBuf::from(&r.path) };
            if !path.is_dir() {
                return None;
            }
            let is_default = if install_defaults.is_empty() { r.default_flag } else { false }
                || install_defaults.iter().any(|d| d.replace('/', "\\") == r.path.replace('/', "\\"));
            Some(Profile {
                id: r.name.clone(),
                label: r.name,
                detail: if is_default { "default profile".into() } else { String::new() },
                path: Some(path),
            })
        })
        .collect()
}
