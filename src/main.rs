#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod launch;
mod register;
mod scan;

use std::cell::Cell;
use std::error::Error;
use std::rc::Rc;
use std::time::Duration;

use slint::{Color, ModelRc, SharedString, Timer, TimerMode, VecModel};

slint::include_modules!();

fn accent_for(name: &str) -> Color {
    let n = name.to_lowercase();
    let (r, g, b) = if n.contains("chrome") {
        (0x42, 0x85, 0xF4)
    } else if n.contains("edge") {
        (0x0F, 0x8E, 0xC7)
    } else if n.contains("firefox") {
        (0xE2, 0x5A, 0x1C)
    } else if n.contains("brave") {
        (0xFB, 0x54, 0x2B)
    } else if n.contains("vivaldi") {
        (0xD6, 0x3A, 0x3A)
    } else if n.contains("opera") {
        (0xE0, 0x1B, 0x2D)
    } else {
        (0x5A, 0x6B, 0x8C)
    };
    Color::from_rgb_u8(r, g, b)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--register") => return Ok(register::register()?),
        Some("--unregister") => return Ok(register::unregister()?),
        // Elevated helpers started by register()/unregister(); the exit code is the result.
        Some("--register-machine") => std::process::exit(register::register_machine().is_err() as i32),
        Some("--unregister-machine") => std::process::exit(register::unregister_machine().is_err() as i32),
        Some("--list") => {
            for b in scan::scan() {
                println!("{} [{:?}] {}", b.name, b.family, b.exe.display());
                for p in &b.profiles {
                    println!("    {} ({}) {}", p.label, p.id, p.detail);
                }
            }
            return Ok(());
        }
        _ => {}
    }
    // Everything else on the command line is the link Windows handed us.
    let url = args.into_iter().next().unwrap_or_default();

    let browsers = Rc::new(scan::scan());

    // One row per (browser, profile); browsers without profiles get a single row.
    let mut targets: Vec<(usize, Option<usize>)> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    for (bi, b) in browsers.iter().enumerate() {
        let letter: SharedString = b.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default().into();
        let accent = accent_for(&b.name);
        if b.profiles.is_empty() {
            targets.push((bi, None));
            rows.push(Row { title: b.name.as_str().into(), subtitle: "Default profile".into(), letter, accent });
        } else {
            for (pi, p) in b.profiles.iter().enumerate() {
                targets.push((bi, Some(pi)));
                let subtitle = if p.detail.is_empty() {
                    p.label.clone()
                } else {
                    format!("{} · {}", p.label, p.detail)
                };
                rows.push(Row {
                    title: b.name.as_str().into(),
                    subtitle: subtitle.into(),
                    letter: letter.clone(),
                    accent,
                });
            }
        }
    }
    let targets = Rc::new(targets);

    let ui = MainWindow::new()?;
    ui.set_url(url.as_str().into());
    ui.set_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_is_default(register::is_default());
    if !launch::is_launchable(&url) {
        ui.set_status("This link can't be opened safely.".into());
    } else if targets.is_empty() {
        ui.set_status("No browsers found.".into());
    }

    let do_launch = {
        let ui = ui.as_weak();
        let browsers = browsers.clone();
        let targets = targets.clone();
        let url = url.clone();
        Rc::new(move |idx: usize| {
            let (Some(ui), Some(&(bi, pi))) = (ui.upgrade(), targets.get(idx)) else { return };
            let browser = &browsers[bi];
            let profile = pi.map(|i| &browser.profiles[i]);
            match launch::launch(browser, profile, &url, ui.get_private_mode()) {
                Ok(()) => {
                    let _ = slint::quit_event_loop();
                }
                Err(e) => ui.set_status(format!("Couldn't start {}: {e}", browser.name).into()),
            }
        })
    };

    ui.on_launch({
        let f = do_launch.clone();
        move |i| f(i as usize)
    });
    ui.on_key({
        let f = do_launch.clone();
        let ui = ui.as_weak();
        move |text| match text.as_str() {
            "\u{1b}" => {
                let _ = slint::quit_event_loop();
            }
            "\n" | "\r" => f(0),
            t if t.len() == 1 && t.as_bytes()[0].is_ascii_digit() && t != "0" => {
                f((t.as_bytes()[0] - b'1') as usize);
            }
            _ => {
                let _ = &ui;
            }
        }
    });
    // Registration may wait on a UAC prompt, so it runs off the UI thread.
    ui.on_register({
        let ui = ui.as_weak();
        move || {
            if let Some(ui) = ui.upgrade() {
                ui.set_status("Waiting for administrator approval…".into());
            }
            let ui = ui.clone();
            std::thread::spawn(move || {
                let result = register::register();
                let _ = ui.upgrade_in_event_loop(move |ui| match result {
                    Ok(()) => {
                        ui.set_status("Registered — choose “Rusty Browser Launcher” in Windows Settings.".into());
                        register::open_default_apps_settings();
                    }
                    Err(e) => ui.set_status(format!("Registration failed: {e}").into()),
                });
            });
        }
    });
    ui.on_unregister({
        let ui = ui.as_weak();
        move || {
            if let Some(ui) = ui.upgrade() {
                ui.set_status("Waiting for administrator approval…".into());
            }
            let ui = ui.clone();
            std::thread::spawn(move || {
                let result = register::unregister();
                let _ = ui.upgrade_in_event_loop(move |ui| match result {
                    Ok(()) => {
                        ui.set_status("Unregistered. Pick another default browser in Windows Settings.".into());
                        ui.set_is_default(false);
                    }
                    Err(e) => ui.set_status(format!("Unregister failed: {e}").into()),
                });
            });
        }
    });

    // Pick up the user's choice in Windows Settings while the window stays open.
    let last = Rc::new(Cell::new(ui.get_is_default()));
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(2), {
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let now = register::is_default();
            if now != last.get() {
                last.set(now);
                ui.set_is_default(now);
                ui.set_status("".into());
            }
        }
    });

    ui.run()?;
    Ok(())
}
