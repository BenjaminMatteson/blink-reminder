// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, RunEvent, State};

/// How often the reminder appears (the 20-20-20 rule).
const INTERVAL: Duration = Duration::from_secs(20 * 60);
/// How long the reminder stays on screen before it hides itself.
const SHOW_FOR: Duration = Duration::from_secs(20);
/// Gap between the overlay and the top of the screen, in logical pixels.
const TOP_MARGIN: f64 = 40.0;

struct Timer {
    next_due: Mutex<Instant>,
    /// Bumped each time the overlay is shown, so a stale auto-hide does not
    /// hide a newer reminder.
    shown: AtomicU64,
}

#[tauri::command]
fn dismiss(app: AppHandle, timer: State<'_, Arc<Timer>>) {
    timer.shown.fetch_add(1, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("reminder") {
        let _ = window.hide();
    }
}

fn show_reminder(app: &AppHandle, timer: &Arc<Timer>) {
    let Some(window) = app.get_webview_window("reminder") else {
        return;
    };

    // Centre the overlay near the top of the screen the user is most likely on.
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale = monitor.scale_factor();
        let (mon_pos, mon_size) = (monitor.position(), monitor.size());
        if let Ok(win_size) = window.outer_size() {
            let x = mon_pos.x + (mon_size.width as i32 - win_size.width as i32) / 2;
            let y = mon_pos.y + (TOP_MARGIN * scale) as i32;
            let _ = window.set_position(PhysicalPosition::new(x, y));
        }
    }

    let id = timer.shown.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = window.set_always_on_top(true);
    let _ = window.show();
    let _ = window.emit("blink", SHOW_FOR.as_secs());

    let app = app.clone();
    let timer = timer.clone();
    thread::spawn(move || {
        thread::sleep(SHOW_FOR);
        if timer.shown.load(Ordering::SeqCst) == id {
            if let Some(window) = app.get_webview_window("reminder") {
                let _ = window.hide();
            }
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
                        let (h, t) = (handle.clone(), timer.clone());
                        let _ = handle.run_on_main_thread(move || show_reminder(&h, &t));
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
