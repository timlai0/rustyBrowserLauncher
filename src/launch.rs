//! Starts a browser (optionally with a specific profile) on a URL.

use std::fs::OpenOptions;
use std::io;
use std::path::Path;
use std::process::Command;

use crate::scan::{Browser, Family, Profile};

/// A link is acceptable if it has a URL scheme (`https:`, `file:` ...) or is an existing
/// local path (when launched for an .html file). Anything starting with `-` is rejected so
/// a crafted "URL" can never be interpreted as a browser command-line flag.
pub fn is_launchable(url: &str) -> bool {
    if url.is_empty() {
        return true;
    }
    if url.starts_with('-') {
        return false;
    }
    let has_scheme = url.split_once(':').is_some_and(|(s, _)| {
        s.len() > 1
            && s.starts_with(|c: char| c.is_ascii_alphabetic())
            && s.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    });
    has_scheme || Path::new(url).exists()
}

pub fn launch(browser: &Browser, profile: Option<&Profile>, url: &str, private: bool) -> io::Result<()> {
    if !is_launchable(url) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "refusing to open an unrecognised link"));
    }
    let mut cmd = Command::new(&browser.exe);
    match browser.family {
        Family::Chromium => {
            if let Some(p) = profile {
                cmd.arg(format!("--profile-directory={}", p.id));
            }
            if private {
                let is_edge = browser.exe.file_stem().is_some_and(|s| s.eq_ignore_ascii_case("msedge"));
                cmd.arg(if is_edge { "--inprivate" } else { "--incognito" });
            }
        }
        Family::Firefox => {
            if let Some(p) = profile {
                // Opening a profile that isn't running needs -no-remote, otherwise Firefox hands
                // the URL to whichever instance is already open (the wrong profile).
                if !profile_in_use(p) {
                    cmd.arg("-no-remote");
                }
                cmd.arg("-P").arg(&p.id);
            }
            if private {
                cmd.arg("-private-window");
            }
        }
        Family::Other => {}
    }
    if !url.is_empty() {
        cmd.arg(url);
    }
    cmd.spawn().map(|_| ())
}

/// Firefox keeps `parent.lock` open exclusively while a profile is running.
fn profile_in_use(p: &Profile) -> bool {
    let Some(dir) = &p.path else { return false };
    let lock = dir.join("parent.lock");
    lock.exists() && OpenOptions::new().write(true).open(lock).is_err()
}
