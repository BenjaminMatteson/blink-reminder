// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// How often the reminder appears (the 20-20-20 rule).
const INTERVAL: Duration = Duration::from_secs(20 * 60);
/// How long the reminder stays on screen before it hides itself.
const SHOW_FOR: Duration = Duration::from_secs(20);

struct Timer {
    next_due: Mutex<Instant>,
    /// Numbers each reminder window so every one gets its own label.
    shown: AtomicU64,
}

const OVERLAY_WIDTH: f64 = 420.0;
const OVERLAY_HEIGHT: f64 = 150.0;

#[tauri::command]
fn dismiss(window: WebviewWindow) {
    let _ = window.destroy();
}

/// Creates a fresh overlay window for each reminder and destroys it when it is
/// done, rather than keeping one hidden webview alive between reminders. A
/// long-lived hidden window could be lost (for example closed with Alt+F4),
/// after which no reminder would ever show again. Must not be called on the
/// main thread: building a webview there deadlocks on Windows.
fn show_reminder(app: &AppHandle, timer: &Timer) {
    // Only one reminder on screen at a time.
    for (label, window) in app.webview_windows() {
        if label.starts_with("reminder") {
            let _ = window.destroy();
        }
    }

    // Centre the overlay on the primary screen.
    let (mut x, mut y) = (100.0, 100.0);
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let scale = monitor.scale_factor();
        let (pos, size) = (monitor.position(), monitor.size());
        x = pos.x as f64 / scale + (size.width as f64 / scale - OVERLAY_WIDTH) / 2.0;
        y = pos.y as f64 / scale + (size.height as f64 / scale - OVERLAY_HEIGHT) / 2.0;
    }

    let label = format!("reminder-{}", timer.shown.fetch_add(1, Ordering::SeqCst));
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("Blink Reminder")
        .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .build();
    if built.is_err() {
        return;
    }

    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(SHOW_FOR);
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.destroy();
        }
    });
}

fn update_tooltip(app: &AppHandle, remaining: Duration) {
    if let Some(tray) = app.tray_by_id("main") {
        let mins = (remaining.as_secs() + 59) / 60;
        let text = format!("Blink Reminder: next reminder in {mins} min");
        let _ = tray.set_tooltip(Some(text));
    }
}

fn main() {
    let timer = Arc::new(Timer {
        next_due: Mutex::new(Instant::now() + INTERVAL),
        shown: AtomicU64::new(0),
    });

    let app = tauri::Builder::default()
        .manage(timer.clone())
        .invoke_handler(tauri::generate_handler![dismiss])
        .setup(move |app| {
            let now_item = MenuItem::with_id(app, "now", "Remind me now", true, None::<&str>)?;
            let reset_item =
                MenuItem::with_id(app, "reset", "Restart 20-minute timer", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &now_item,
                    &reset_item,
                    &PredefinedMenuItem::separator(app)?,
                    &quit_item,
                ],
            )?;

            let menu_timer = timer.clone();
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Blink Reminder")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "now" => *menu_timer.next_due.lock().unwrap() = Instant::now(),
                    "reset" => {
                        *menu_timer.next_due.lock().unwrap() = Instant::now() + INTERVAL;
                        update_tooltip(app, INTERVAL);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // One background thread drives the schedule; it wakes every second
            // so "Remind me now" and the tray tooltip respond promptly.
            let handle = app.handle().clone();
            let timer = timer.clone();
            thread::spawn(move || {
                let mut last_minute = u64::MAX;
                loop {
                    thread::sleep(Duration::from_secs(1));
                    let now = Instant::now();
                    let due = {
                        let mut next = timer.next_due.lock().unwrap();
                        if now >= *next {
                            *next = now + INTERVAL;
                            true
                        } else {
                            false
                        }
                    };
                    if due {
                        show_reminder(&handle, &timer);
                    }
                    let remaining = timer.next_due.lock().unwrap().saturating_duration_since(now);
                    let minute = (remaining.as_secs() + 59) / 60;
                    if minute != last_minute {
                        last_minute = minute;
                        update_tooltip(&handle, remaining);
                    }
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start Blink Reminder");

    // Keep running in the tray when the overlay window is hidden or closed.
    app.run(|_app, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
