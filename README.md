# Blink Reminder

A tiny tray app for eye-strain relief. Every 20 minutes a small always-on-top
card appears at the top of your screen reminding you to blink and look at
something 20 feet away for 20 seconds (the 20-20-20 rule). It counts down and
hides itself after 20 seconds, or you can click it to dismiss it early.

## Use it

Download `BlinkReminder.exe` from the
[latest release](https://github.com/BenjaminMatteson/blink-reminder/releases/latest)
and run it. There is no installer; it lives in the system tray.

Tray menu:

- **Remind me now** shows the reminder immediately (and restarts the 20 minutes).
- **Restart 20-minute timer** resets the countdown without showing anything.
- **Quit** exits.

Hover the tray icon to see how long until the next reminder.

Requires the Microsoft Edge WebView2 runtime, which ships with Windows 10
(recent updates) and Windows 11. Windows SmartScreen may warn about an
unrecognised app the first time since the exe is unsigned; choose
"More info" then "Run anyway".

## Build from source

Built with [Tauri 2](https://tauri.app). You need Rust (stable, MSVC
toolchain on Windows) and Node.js.

```sh
npm install
npm run build   # produces src-tauri/target/release/blink-reminder.exe
```

Tauri also builds on macOS and Linux (Linux needs `libwebkit2gtk-4.1` and
`libayatana-appindicator3`).
