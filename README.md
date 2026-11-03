# adbclone

A cross-platform desktop app for managing Android apps over ADB (no root): list, disable, uninstall for the current user, restore, with safety ratings. Built with Tauri 2 (Rust) and React + TypeScript.

An original implementation, inspired by tools like ADB AppControl; it shares no code, assets or lists with them.

## Layout
- `adb-core/` — Tauri-independent Rust crate: runs the `adb` binary and parses output (devices, packages, running processes, app labels from APKs). `cargo test -p adb-core`.
- `src-tauri/` — Tauri shell and IPC commands; `data/ratings/*.json` are our own safety ratings (unreviewed guidance, not guarantees). Extra rating files can be dropped into `<app-data>/ratings/`.
- `ui/` — React UI. In a plain browser (`npm run dev`) it runs on mock data; inside Tauri it uses the real backend.

## Run
Needs Node 22+, Rust, and `adb` (Android platform-tools) on PATH, or set `ADB_PATH`, or put `platform-tools/` next to the executable. On Linux install the Tauri system libraries (webkit2gtk-4.1, gtk3).

```bash
cd ui && npm install && npx tauri dev     # development
cd ui && npx tauri build                  # installers / executable
```
`.github/workflows/build.yml` builds Windows, macOS and Linux artifacts.

## Status
Not yet tested against a real device or emulator; parsers are tested against fixtures only.
