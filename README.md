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

## Features
- **Set-up:** downloads Google's platform-tools if adb is missing; first-run guide for USB and Wireless debugging with maker notes (Xiaomi, Samsung and others); wireless ADB (pair by code or QR, USB-to-Wi-Fi switch, saved phones).
- **Apps:** real names and icons through a temporary on-device helper (nothing installed); plain-language mode (names and a one-line reason instead of package names); filters by category, "installed by" and duplicates of Google apps; disable / enable / uninstall for you / restore with undo; ~700 own ratings with Safe / Balanced / Aggressive tiers, "what stops working" lines, and apps you actually use (home, dialer, browser, keyboard…) protected automatically.
- **Debloat wizard** with presets and RAM estimates; **snapshots** before every removal, restore, export/import; **Rescue mode** (restore the newest snapshot, Safe Mode steps per maker).
- **Install and backup:** drag-drop APK / APKS / XAPK / split APKs; export installed APKs including splits.
- **Storage:** space per app, one-click cache cleanup, apps not opened in 30/60/90/180 days.
- **Performance:** Running (RAM, CPU, force stop), Temperature, Tune (battery saver, 60 Hz cap, background limits, cool-down), **Animation speeds** (presets and separate window / transition / in-app mixes), **Profiles** (Gaming, Battery saver, Work), before/after history with battery drain, a visible "phone is warm" suggestion (never an automatic action), and a **Wake-ups** view (held wake locks, apps that wake the phone most).
- **Ratings sharing:** rate apps yourself, export and import ratings files. Imported files are sanitised: never "recommended", never pre-ticked, never override built-in ratings or core system apps.
- **Several phones:** a Phones overview, "also do this on" other phones in the confirmation window (each phone saves its own snapshot), screenshots, and screen mirroring by launching scrcpy (install it separately).
- **Trust:** a Logs tab shows every adb command; light / dark theme; English, Spanish, French, Portuguese and German (translations of the main screens are drafts).
- "Save device report" (read-only, privacy-scrubbed) gathers what the app reads from a phone, to fix parsers from real data.

## Status: what is and is not verified
Verified by automated tests: the parsers against fixtures in the formats I expect, a fake `adb` that exercises each command end to end, the Rust code compiled for Windows, the whole UI against mock data in a real browser, and a clean-checkout build (`scripts/check.sh`).

**Not yet run on a real phone:** the on-device helper, usage statistics, storage (`dumpsys diskstats`), wake-lock and alarm parsers, animation and refresh-rate settings on every maker's firmware, scrcpy launching, and downloading platform-tools from Google (blocked in my build environment). Phones differ; where a reading is unavailable the app says so instead of guessing. Please use "Save device report" and send it if something looks wrong.
Ratings are unreviewed guidance written from public knowledge, not guarantees. Translations are drafts that need a native speaker's review.

## The on-device helper
`helper/` holds a tiny Java program (compiled to `helper/dist/helper.jar`, embedded in the app). It runs with `app_process` as the shell user, like scrcpy's server, asks Android's own PackageManager for every app's name, icon and basic details, prints them, and is deleted again. Rebuild it with `helper/build.sh` (needs a JDK, `dalvik-exchange` and `android-sdk-platform-23`); the build is reproducible and CI checks the jar against `helper/dist/helper.jar.sha256`.
It has not yet been run on a real phone from here; if it fails, the app says why ("Names" line) and "Save device report" includes its self-test.
