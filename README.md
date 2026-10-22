# Rusty Browser Launcher

A small Windows app (Rust + [Slint](https://slint.dev)) that registers itself as a browser.
When you click a link, it shows a picker: choose the browser **and profile**, and it opens the link there.

## Features

- **Auto-detects browsers** from the registry (`Clients\StartMenuInternet`, HKLM + HKCU): Chrome (stable, Beta, Dev, Canary), Edge (incl. Canary), Brave, Vivaldi, Chromium-based browsers such as Helium, Firefox and Floorp, plus any other registered browser.
- **Auto-detects profiles**
  - Chromium family: reads `Local State` in the browser's `User Data` folder (profile name + signed-in account).
  - Firefox family: reads `profiles.ini`.
- **Opens the link in the chosen profile**
  - Chromium: `--profile-directory=<dir> <url>`
  - Firefox: `-P <name> <url>`; `-no-remote` is added unless that profile is already running (detected through its `parent.lock`), so the link doesn't land in the wrong profile.
- **Private window** checkbox (`--inprivate` for Edge, `--incognito` for other Chromium browsers, `-private-window` for Firefox).
- **Keyboard**: `1`-`9` open that row, `Enter` the first row, `Esc` closes.
- **Default-browser registration** from inside the app ("Set as default…" / "Unregister").
- **Safety**: a link starting with `-` or without a URL scheme (and not an existing file path) is refused, so a crafted link can't be read as a browser flag.

## Build

Requirements: Rust (the project was built with `stable-x86_64-pc-windows-gnu`, Rust 1.98) and a resource compiler (`windres` from MinGW/WinLibs, or `rc.exe` with the MSVC toolchain) for the exe icon and version info.

```powershell
cargo build --release
```

Output: `target\release\rusty-browser-launcher.exe` (about 10 MB, LTO + stripped).
If your project drive is unreliable, build elsewhere: `$env:CARGO_TARGET_DIR = "C:\path\to\target"`.

## Install and use

1. Copy the exe to a **permanent** location, for example `%LOCALAPPDATA%\Programs\RustyBrowserLauncher\`. Registration stores the exe's path; if you move the exe, unregister and register again.
2. Start it and click **Set as default…**. Windows shows **one UAC prompt** (see "Known issue" below). Approve it; Settings > Default apps then opens.
3. In Settings, pick **Rusty Browser Launcher** for `http` and `https` (and `.htm`/`.html` if you want). Windows does not allow an app to set itself as default silently.
4. Click any link: the picker appears.

## Command line

| Command | Effect |
| --- | --- |
| `rusty-browser-launcher.exe <url>` | Open the picker for that link (what Windows runs) |
| `rusty-browser-launcher.exe` | Open the picker without a link |
| `--register` / `--unregister` | Same as the buttons (may show the UAC prompt) |
| `--register-machine` / `--unregister-machine` | Machine-wide part only; run elevated, used internally |
| `--list` | Print detected browsers and profiles (debug builds show it in the console; the release build has no console) |

## What registration writes

`src/register.rs` writes the same layout Chrome and Firefox use:

- `Software\Classes\RustyBrowserLauncherHTML` (ProgID with `Application` subkey, `shell\open\command`)
- `Software\Clients\StartMenuInternet\RustyBrowserLauncher` with `Capabilities` (`URLAssociations` http/https, `FileAssociations` .htm/.html/.xhtml/.shtml), `InstallInfo`, `shell\open\command`
- `Software\RegisteredApplications\RustyBrowserLauncher`

Written under **HKCU** (no admin) and **HKLM** (admin, via the elevated re-launch). The app also adds a per-user "Installed apps" entry (`...\Uninstall\RustyBrowserLauncher`) and a Start menu shortcut. The exe embeds an icon and version info (`build.rs`, `assets/icon.ico`).

## Known issue: registration needs administrator approval

**Status: open, to be fixed later.**

With HKCU-only registration, Windows 11 (build 26300 here) did **not** list the app in Settings > Default apps or in the "choose an app for https links" picker. The app appeared only after the same entries were also written under **HKLM**, which needs admin rights. So `register()` currently re-launches the exe elevated once.

What was verified:

- Windows' own enumeration (`SHAssocEnumHandlersForProtocolByApplication("https")`) lists the app as an `http`/`https` handler even with HKCU only, and `SHAssocEnumHandlers(".html")` lists it too. The registry content is valid; the Settings UI and picker just don't show it.
- Removing the HKLM entries makes the app disappear from both lists again.
- None of these helped HKCU-only: restarting Explorer, restarting `SystemSettings.exe`, `SHChangeNotify(SHCNE_ASSOCCHANGED)`, a Start menu shortcut, an "Installed apps" entry, an `AppUserModelID` on the shortcut matching the ProgID, `InstallInfo`/`Startmenu` keys, exe version info and icon, a single HTML ProgID for links and files.
- A second A/B test with two fake per-user browsers (plain, and with every extra incl. `App Paths`, `Classes\Applications`, `AppUserModelId` class, Helium's full protocol list) showed neither in Settings.
- Chrome Canary and Helium, installed per-user, *are* listed with HKCU-only registrations that look the same as ours.

Ideas not tried yet:

- **Sign out/in** after an HKCU-only registration. Windows may read per-user entries only at sign-in (Canary and Helium were registered before the session started). Test apps for this can be made with `tools/hkcu-ab-test.ps1`.
- Diffing the full HKCU footprint of Helium against ours with a registry monitor (Process Monitor) while Settings builds its list.
- Packaging the app (MSIX with sparse package / package identity), which makes Windows treat it as a first-class default-app candidate.
- A one-time installer that does the HKLM part, so the app itself never needs elevation.

Helper scripts in `tools/`:

- `hklm-register.ps1` writes (or with `-Undo` removes) the machine-wide registration from an elevated PowerShell.
- `hkcu-ab-test.ps1` creates two fake per-user browsers for the experiment above; `-Cleanup` removes them.

## Troubleshooting

- **Not in Default apps / picker**: make sure the machine-wide registration exists (`reg query HKLM\SOFTWARE\RegisteredApplications /v RustyBrowserLauncher`), then close and reopen Settings; Settings keeps an old list while it is running. The picker lists the default app at the top, other apps below it.
- **Links open nothing after moving the exe**: registration points at the old path. Run `--unregister`, then register from the new location.
- **H:/removable drive builds**: if the project lives on an unreliable drive, build with `CARGO_TARGET_DIR` on a local disk.
- **Undo everything**: click **Unregister** (or `--unregister`). It removes the per-user entries, the Start menu shortcut and the uninstall entry, and asks for approval again to remove the HKLM entries.

## Not yet verified

- Clicking a row to open each browser/profile combination was not exercised end to end in testing; only the picker window starting with the right URL was confirmed.
- The Firefox `-no-remote`/`parent.lock` logic and the private-window flags are untested.
- Firefox-family forks other than Firefox and Floorp (Waterfox, LibreWolf, Zen) are mapped to their `%APPDATA%` folders by guess.

## Project layout

```
build.rs            compiles ui/main.slint, embeds icon + version info
assets/icon.ico     app icon
ui/main.slint       picker window
src/main.rs         CLI flags, window wiring, launching
src/scan.rs         browser + profile discovery
src/launch.rs       command-line building, URL validation
src/register.rs     registry registration, UAC re-launch, default detection
tools/              helper PowerShell scripts for the registration experiments
```
