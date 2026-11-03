<p align="center"><img src="brand/out/icon-1024.png" width="120" alt="Coolant logo"></p>

# Coolant

**Debloat. Cool down. Speed up.** An Android cleaner that runs on your computer: no root, works over USB or Wi-Fi, takes a safety snapshot before every removal.

(The repository is still called `adbclone`; the product name lives in `ui/src/brand.ts`, `src-tauri/tauri.conf.json` and `brand/`.)

A cross-platform desktop app for managing Android apps over ADB (no root): list, disable, uninstall for the current user, restore, with safety ratings. Built with Tauri 2 (Rust) and React + TypeScript.

An original implementation, inspired by tools like ADB AppControl; it shares no code, assets or lists with them.

## Layout
- `adb-core/` — Tauri-independent Rust crate: runs the `adb` binary and parses output (devices, packages, running processes, app labels from APKs). `cargo test -p adb-core`.
- `src-tauri/` — Tauri shell and IPC commands; `data/ratings/*.json` are our own safety ratings (unreviewed guidance, not guarantees). Extra rating files can be dropped into `<app-data>/ratings/`.
- `ui/` — React UI. In a plain browser (`npm run dev`) it runs on mock data; inside Tauri it uses the real backend.

## Run
If adb is not on your computer, the app offers to download Google's official platform-tools into its own folder (from dl.google.com, checked by running it).
Needs Node 22+, Rust, and `adb` (Android platform-tools) on PATH, or set `ADB_PATH`, or put `platform-tools/` next to the executable. On Linux install the Tauri system libraries (webkit2gtk-4.1, gtk3).

```bash
cd ui && npm install && npx tauri dev     # development
cd ui && npx tauri build                  # installers / executable
```
`.github/workflows/build.yml` builds Windows, macOS and Linux artifacts.

## Status
Features: app names and icons for every app through a temporary on-device helper (pushed, run once, deleted; nothing is installed; falls back to reading app files, and explains any app it cannot name), "Save device report" (read-only, privacy-scrubbed dump of what the app reads from the phone, for fixing parsers from real data), Tune tab (calmer animations, battery saver, 60 Hz cap, per-app background restrictions, one-click cool-down with before/after temperatures; every change is verified on the phone and undoable), temperature tab (battery and sensor temperatures, throttling status, CPU speed), device info, debloat wizard (Safe/Balanced/Aggressive presets with RAM estimates), wireless ADB (pair by code or QR, USB-to-Wi-Fi switch, saved phones that reconnect when the port changes), snapshots of app states (automatic before every disable/uninstall batch, restorable, exportable), saved action history, app list with real names (read from APKs, cached), disable/enable/uninstall/restore with undo, a Running tab (RAM, CPU, force stop) and safe-to-disable / safe-to-uninstall tags from ~465 own ratings.

Not yet tested against a real device or emulator. Parsers and the label/process pipelines are tested against fixtures and a fake `adb` (`adb-core/tests`).

## The on-device helper
`helper/` holds a tiny Java program (compiled to `helper/dist/helper.jar`, embedded in the app). It runs with `app_process` as the shell user, like scrcpy's server, asks Android's own PackageManager for every app's name, icon and basic details, prints them, and is deleted again. Rebuild it with `helper/build.sh` (needs a JDK, `dalvik-exchange` and `android-sdk-platform-23`); the build is reproducible and CI checks the jar against `helper/dist/helper.jar.sha256`.
It has not yet been run on a real phone from here; if it fails, the app says why ("Names" line) and "Save device report" includes its self-test.
