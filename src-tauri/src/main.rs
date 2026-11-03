#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ratings;

use adb_core::{storage, wakelocks, backup, bridge, install, log as cmdlog, platform_tools, protect, report, tuning, deviceinfo, devices, health, labels, packages, processes, snapshot, wireless, AdbRunner};
use packages::Filter;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: String,
    label: String,
    system: bool,
    state: &'static str,
    risk: &'static str,
    disable_safety: &'static str,
    uninstall_safety: &'static str,
    description: Option<String>,
    tier: &'static str,
    category: Option<String>,
    impact: Option<String>,
    confidence: &'static str,
    reinstall: Option<String>,
    duplicate_of: Option<String>,
}

static RATINGS: OnceLock<ratings::Ratings> = OnceLock::new();

fn ratings() -> &'static ratings::Ratings {
    RATINGS.get_or_init(|| ratings::Ratings::load(None))
}

#[derive(Default)]
struct AppState {
    /// Previous process snapshot per device, for CPU deltas.
    prev: Mutex<HashMap<String, processes::Snapshot>>,
    /// package -> uid per device, refreshed every 30 s.
    uids: Mutex<HashMap<String, (Instant, HashMap<String, u32>)>>,
    /// Bumped whenever a new label load starts, so older loads stop.
    label_generation: AtomicU64,
    /// path|size|mtime -> label, persisted as JSON in the app data dir.
    label_cache: Mutex<HashMap<String, String>>,
    cache_file: Mutex<Option<std::path::PathBuf>>,
    /// Bumped to cancel a pending QR pairing.
    qr_generation: AtomicU64,
    saved: Mutex<Vec<SavedDevice>>,
    saved_file: Mutex<Option<std::path::PathBuf>>,
    data_dir: Mutex<Option<std::path::PathBuf>>,
    /// Changes made by the tuning features, so they can be undone later (even after a restart).
    tuning: Mutex<Vec<tuning::Record>>,
    /// Things queued in the install tool, kept until installed or discarded.
    installs: Mutex<HashMap<String, install::InstallItem>>,
}

#[derive(Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SavedDevice {
    name: String,
    address: String,
    /// mDNS instance name (`adb-<serial>-<id>`), stable across port changes.
    mdns_name: Option<String>,
}

fn persist_saved(state: &AppState) {
    let file = state.saved_file.lock().unwrap().clone();
    if let Some(f) = file {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&*state.saved.lock().unwrap()) {
            let _ = std::fs::write(f, json);
        }
    }
}

fn remember(state: &AppState, name: String, address: &str, mdns_name: Option<String>) {
    {
        let mut v = state.saved.lock().unwrap();
        // Same phone (by mDNS name) or same address: update instead of duplicating.
        v.retain(|d| d.address != address && (mdns_name.is_none() || d.mdns_name != mdns_name));
        v.insert(0, SavedDevice { name, address: address.to_string(), mdns_name });
        v.truncate(20);
    }
    persist_saved(state);
}

#[derive(Serialize)]
struct ActionResult {
    name: String,
    ok: bool,
    message: Option<String>,
}

/// Where the app keeps its own files (set at startup); our downloaded adb lives under it.
static APP_DATA: OnceLock<std::path::PathBuf> = OnceLock::new();

fn find_adb() -> Option<platform_tools::Found> {
    platform_tools::find_adb(
        std::env::var("ADB_PATH").ok().as_deref(),
        APP_DATA.get().map(|p| p.as_path()),
        std::env::current_exe().ok().as_deref().and_then(|p| p.parent()),
        std::env::var_os("PATH").as_deref(),
        platform_tools::current_os(),
    )
}

/// adb lookup order: $ADB_PATH, the copy this app downloaded, next to the app, then PATH.
fn adb() -> AdbRunner {
    match find_adb() {
        Some(f) => AdbRunner::new(f.path),
        // Not found anywhere: the plain name gives the usual "not found" error the UI turns into setup help.
        None => AdbRunner::new(platform_tools::adb_file_name(platform_tools::current_os())),
    }
}

#[tauri::command]
async fn list_devices() -> Result<Vec<devices::Device>, String> {
    devices::list_devices(&adb()).await.map_err(|e| {
        format!("{e}. Install Android platform-tools and put `adb` on PATH (or set ADB_PATH).")
    })
}

#[tauri::command]
async fn list_apps(serial: String) -> Result<Vec<AppInfo>, String> {
    let states = packages::list_states(&adb(), &serial).await.map_err(|e| e.to_string())?;
    let rules = ratings();
    let mut out: Vec<AppInfo> = states
        .into_iter()
        .map(|p| {
            let rule = rules.lookup(&p.name);
            AppInfo {
                label: labels::fallback_label(&p.name),
                system: p.system,
                state: p.state.as_str(),
                risk: rule.as_ref().map_or("unknown", |r| r.risk),
                disable_safety: rule.as_ref().map_or("unknown", |r| r.disable.as_str()),
                uninstall_safety: rule.as_ref().map_or("unknown", |r| r.uninstall.as_str()),
                tier: rule.as_ref().map_or("unrated", |r| r.tier),
                category: rule.as_ref().and_then(|r| r.category.clone()),
                impact: rule.as_ref().map(|r| r.impact.clone()).filter(|i| !i.is_empty()),
                confidence: rule.as_ref().map_or("low", |r| r.confidence),
                reinstall: rule.as_ref().and_then(|r| r.reinstall.clone()),
                duplicate_of: rule.as_ref().and_then(|r| r.duplicate_of.clone()),
                description: rule.map(|r| r.desc),
                name: p.name,
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[tauri::command]
async fn apply_action(serial: String, action: String, names: Vec<String>) -> Result<Vec<ActionResult>, String> {
    let a = adb();
    let mut results = Vec::new();
    for name in names {
        let r = match action.as_str() {
            "disable" => packages::disable(&a, &serial, &name).await,
            "enable" => packages::enable(&a, &serial, &name).await,
            "uninstall" => packages::uninstall_for_user(&a, &serial, &name).await,
            "restore" => packages::restore(&a, &serial, &name).await,
            "stop" => packages::force_stop(&a, &serial, &name).await,
            other => return Err(format!("unknown action: {other}")),
        };
        results.push(ActionResult {
            name,
            ok: r.is_ok(),
            message: r.err().map(|e| e.to_string()),
        });
    }
    Ok(results)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunningRow {
    package: Option<String>,
    rss_kb: u64,
    cpu: Option<f32>,
    processes: usize,
    risk: &'static str,
    disable_safety: &'static str,
    uninstall_safety: &'static str,
    description: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunningResponse {
    rows: Vec<RunningRow>,
    mem_total_kb: u64,
    mem_available_kb: u64,
    cores: usize,
}

async fn package_uids(state: &AppState, a: &AdbRunner, serial: &str) -> Result<HashMap<String, u32>, String> {
    if let Some((at, m)) = state.uids.lock().unwrap().get(serial) {
        if at.elapsed() < Duration::from_secs(30) {
            return Ok(m.clone());
        }
    }
    let list = packages::list_packages(a, serial, Filter::All, false).await.map_err(|e| e.to_string())?;
    let m: HashMap<String, u32> = list.into_iter().filter_map(|p| p.uid.map(|u| (p.name, u))).collect();
    state.uids.lock().unwrap().insert(serial.to_string(), (Instant::now(), m.clone()));
    Ok(m)
}

#[tauri::command]
async fn list_running(state: State<'_, AppState>, serial: String) -> Result<RunningResponse, String> {
    let a = adb();
    let cur = processes::snapshot(&a, &serial).await.map_err(|e| e.to_string())?;
    let uids = package_uids(&state, &a, &serial).await?;
    let prev = state.prev.lock().unwrap().insert(serial.clone(), cur.clone());
    let report = processes::build_report(&cur, prev.as_ref(), &uids);
    let r = ratings();
    let rows = report
        .apps
        .into_iter()
        .map(|app| {
            let rule = app.package.as_deref().and_then(|p| r.lookup(p));
            RunningRow {
                risk: rule.as_ref().map_or("unknown", |x| x.risk),
                disable_safety: rule.as_ref().map_or("unknown", |x| x.disable.as_str()),
                uninstall_safety: rule.as_ref().map_or("unknown", |x| x.uninstall.as_str()),
                description: rule.map(|x| x.desc),
                package: app.package,
                rss_kb: app.rss_kb,
                cpu: app.cpu,
                processes: app.processes,
            }
        })
        .collect();
    Ok(RunningResponse { rows, mem_total_kb: report.mem_total_kb, mem_available_kb: report.mem_available_kb, cores: report.cores })
}

#[derive(Serialize, Clone)]
struct LabelItem {
    name: String,
    label: String,
    /// Where the name came from: `helper`, `file` or `cache`.
    source: &'static str,
}

#[derive(Serialize, Clone)]
struct LabelProgress {
    done: usize,
    total: usize,
}

#[derive(Serialize, Clone)]
struct Unresolved {
    name: String,
    reason: String,
}

/// How the names were found, and why some were not.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct NamesStatus {
    helper: usize,
    file: usize,
    cached: usize,
    unresolved: Vec<Unresolved>,
    /// Why the on-device helper could not be used, if it could not.
    helper_message: Option<String>,
    finished: bool,
}

/// Extra facts the helper reports about each app.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DetailItem {
    name: String,
    launchable: bool,
    installer: String,
    version_name: String,
    version_code: i64,
    first_install: i64,
    last_update: i64,
    target_sdk: i64,
    updated_system: bool,
}

#[derive(Serialize, Clone)]
struct IconItem {
    name: String,
    /// PNG, base64.
    png: String,
}

fn save_label_cache(state: &AppState) {
    let file = state.cache_file.lock().unwrap().clone();
    if let Some(f) = file {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let snapshot = state.label_cache.lock().unwrap().clone();
        if let Ok(json) = serde_json::to_string(&snapshot) {
            let _ = std::fs::write(f, json);
        }
    }
}

/// A name that is just the package id is not a name.
fn real_name(label: &str, package: &str) -> bool {
    let l = label.trim();
    !l.is_empty() && l != package
}

/// Icons are cached as `<package>-<last update>.b64`, so an updated app gets a fresh icon.
fn icon_file(dir: &std::path::Path, pkg: &str, last_update: i64) -> Option<std::path::PathBuf> {
    packages::valid_name(pkg).then(|| dir.join(format!("{pkg}-{last_update}.b64")))
}

/// Fetch icons through the helper (cached on disk), user apps first. Deletes the helper when done.
async fn icons_task(app: AppHandle, serial: String, gen: u64, a: AdbRunner, remote: String, mut apps: Vec<bridge::BridgeApp>) {
    let state = app.state::<AppState>();
    let alive = || state.label_generation.load(Ordering::SeqCst) == gen;
    apps.sort_by_key(|x| x.system);
    let dir = data_subdir(&state, "icons").ok();
    let mut need: Vec<&bridge::BridgeApp> = Vec::new();
    let mut cached: Vec<IconItem> = Vec::new();
    for x in &apps {
        let hit = dir.as_ref().and_then(|d| icon_file(d, &x.package, x.last_update)).and_then(|f| std::fs::read_to_string(f).ok());
        match hit {
            Some(png) if !png.is_empty() => cached.push(IconItem { name: x.package.clone(), png }),
            _ => need.push(x),
        }
    }
    for chunk in cached.chunks(60) {
        let _ = app.emit("icons", chunk.to_vec());
    }
    for batch in need.chunks(40) {
        if !alive() {
            break;
        }
        let names: Vec<String> = batch.iter().map(|x| x.package.clone()).collect();
        let Ok(res) = bridge::run(&a, &serial, &remote, "icons", &names, Duration::from_secs(120)).await else { continue };
        let mut out = Vec::new();
        for l in res.parsed.lines {
            if let bridge::Line::Icon { package, png_base64 } = l {
                if let Some(x) = batch.iter().find(|x| x.package == package) {
                    if let Some(f) = dir.as_ref().and_then(|d| icon_file(d, &package, x.last_update)) {
                        if let Some(parent) = f.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(f, &png_base64);
                    }
                }
                out.push(IconItem { name: package, png: png_base64 });
            }
        }
        if !out.is_empty() {
            let _ = app.emit("icons", out);
        }
    }
    bridge::cleanup(&a, &serial, &remote).await;
}

/// Names for every app: helper first (all apps, in the phone's own words), then reading the app files for
/// whatever is still unnamed. Reasons are kept for anything that stays unnamed.
async fn names_pipeline(app: AppHandle, serial: String, gen: u64, a: AdbRunner, todo: Vec<(String, String)>, all_names: Vec<String>) {
    let state = app.state::<AppState>();
    let alive = || state.label_generation.load(Ordering::SeqCst) == gen;
    let total = all_names.len();
    let mut status = NamesStatus::default();
    let mut named: HashSet<String> = HashSet::new();

    // 1. The on-device helper.
    let mut icons_handle = None;
    match bridge::push(&a, &serial).await {
        Err(e) => status.helper_message = Some(format!("could not start the helper: {e}")),
        Ok(remote) => match bridge::run(&a, &serial, &remote, "apps", &[], Duration::from_secs(90)).await {
            Err(e) => {
                status.helper_message = Some(format!("the helper did not run: {e}"));
                bridge::cleanup(&a, &serial, &remote).await;
            }
            Ok(res) => {
                let apps: Vec<bridge::BridgeApp> = res.parsed.lines.iter().filter_map(|l| if let bridge::Line::App(x) = l { Some(x.clone()) } else { None }).collect();
                if apps.is_empty() {
                    status.helper_message = Some(res.failure_reason());
                    bridge::cleanup(&a, &serial, &remote).await;
                } else {
                    if !res.parsed.complete {
                        status.helper_message = Some("the helper stopped early; the file method covers the rest".into());
                    }
                    let items: Vec<LabelItem> = apps
                        .iter()
                        .filter(|x| real_name(&x.label, &x.package))
                        .map(|x| LabelItem { name: x.package.clone(), label: x.label.trim().to_string(), source: "helper" })
                        .collect();
                    named.extend(items.iter().map(|i| i.name.clone()));
                    status.helper = items.len();
                    for chunk in items.chunks(200) {
                        let _ = app.emit("labels", chunk.to_vec());
                    }
                    let details: Vec<DetailItem> = apps
                        .iter()
                        .map(|x| DetailItem {
                            name: x.package.clone(),
                            launchable: x.launchable,
                            installer: x.installer.clone(),
                            version_name: x.version_name.clone(),
                            version_code: x.version_code,
                            first_install: x.first_install,
                            last_update: x.last_update,
                            target_sdk: x.target_sdk,
                            updated_system: x.updated_system,
                        })
                        .collect();
                    for chunk in details.chunks(200) {
                        let _ = app.emit("bridge-details", chunk.to_vec());
                    }
                    icons_handle = Some(tauri::async_runtime::spawn(icons_task(app.clone(), serial.clone(), gen, a.clone(), remote, apps)));
                }
            }
        },
    }
    let _ = app.emit("names-status", status.clone());
    let _ = app.emit("labels-progress", LabelProgress { done: named.len(), total });

    // 2. Reading the app files, only for what is still unnamed.
    let rest: Vec<(String, String)> = todo.into_iter().filter(|(n, _)| !named.contains(n)).collect();
    let with_path: HashSet<&String> = rest.iter().map(|(n, _)| n).collect();
    for n in &all_names {
        if !named.contains(n) && !with_path.contains(n) {
            status.unresolved.push(Unresolved { name: n.clone(), reason: "the phone gave no file for this app".into() });
        }
    }
    let mut done = named.len() + status.unresolved.len();
    for batch in rest.chunks(100) {
        if !alive() {
            return; // a newer load (or device switch) superseded this one
        }
        let paths: Vec<&str> = batch.iter().map(|(_, p)| p.as_str()).collect();
        let stats = labels::stat_files(&a, &serial, &paths).await.unwrap_or_default();
        let key = |path: &str| stats.get(path).map(|(s, m)| format!("{path}|{s}|{m}"));

        let mut ready: Vec<LabelItem> = Vec::new();
        let mut missing: Vec<(String, String, Option<String>)> = Vec::new();
        {
            let cache = state.label_cache.lock().unwrap();
            for (name, path) in batch {
                let k = key(path);
                match k.as_ref().and_then(|k| cache.get(k)) {
                    Some(l) => ready.push(LabelItem { name: name.clone(), label: l.clone(), source: "cache" }),
                    None => missing.push((name.clone(), path.clone(), k)),
                }
            }
        }
        done += ready.len();
        status.cached += ready.len();
        if !ready.is_empty() {
            let _ = app.emit("labels", ready);
        }
        // Resolve the rest, four at a time.
        for group in missing.chunks(4) {
            if !alive() {
                return;
            }
            let tasks: Vec<_> = group
                .iter()
                .cloned()
                .map(|(name, path, k)| {
                    let (a, serial) = (a.clone(), serial.clone());
                    tauri::async_runtime::spawn(async move {
                        let r = labels::resolve(&a, &serial, &path).await;
                        (name, k, r)
                    })
                })
                .collect();
            let mut out = Vec::new();
            for t in tasks {
                let Ok((name, k, r)) = t.await else { continue };
                match r.label {
                    Some(label) => {
                        if let Some(k) = k {
                            state.label_cache.lock().unwrap().insert(k, label.clone());
                        }
                        status.file += 1;
                        out.push(LabelItem { name, label, source: "file" });
                    }
                    None => status.unresolved.push(Unresolved { name, reason: r.reason.unwrap_or_else(|| "unknown".into()) }),
                }
            }
            done += group.len();
            if !out.is_empty() {
                let _ = app.emit("labels", out);
            }
            let _ = app.emit("labels-progress", LabelProgress { done: done.min(total), total });
        }
        save_label_cache(&state);
    }
    if let Some(h) = icons_handle {
        let _ = h.await;
    }
    status.finished = true;
    let _ = app.emit("names-status", status);
    let _ = app.emit("labels-progress", LabelProgress { done: total, total });
}

/// Starts finding real names (and icons) for every app in the background. Results arrive as events:
/// `labels`, `labels-progress`, `names-status`, `bridge-details`, `icons`. Calling it again retries.
#[tauri::command]
async fn load_labels(app: AppHandle, state: State<'_, AppState>, serial: String) -> Result<(), String> {
    let gen = state.label_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let a = adb();
    let all = packages::list_packages(&a, &serial, Filter::All, true).await.map_err(|e| e.to_string())?;
    let system: HashSet<String> = packages::list_packages(&a, &serial, Filter::System, true)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|p| p.name)
        .collect();
    let all_names: Vec<String> = all.iter().map(|p| p.name.clone()).collect();
    // Third-party apps first: they are what people look for.
    let mut todo: Vec<(String, String)> = all.into_iter().filter_map(|p| p.apk_path.map(|path| (p.name, path))).collect();
    todo.sort_by_key(|(name, _)| system.contains(name));
    tauri::async_runtime::spawn(names_pipeline(app, serial, gen, a, todo, all_names));
    Ok(())
}

// ---- wireless ADB ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdbCheck {
    major: Option<u32>,
    ok: bool,
    message: String,
}

#[tauri::command]
async fn adb_check() -> Result<AdbCheck, String> {
    match wireless::adb_major_version(&adb()).await {
        Ok(m) if m >= 30 => Ok(AdbCheck { major: Some(m), ok: true, message: format!("adb {m}") }),
        Ok(m) => Ok(AdbCheck {
            major: Some(m),
            ok: false,
            message: format!("adb {m} is too old for wireless pairing; install platform-tools 30 or newer."),
        }),
        Err(e) => Err(format!("{e}. Install Android platform-tools and put `adb` on PATH (or set ADB_PATH).")),
    }
}

#[tauri::command]
async fn wireless_discover() -> Result<Vec<wireless::MdnsService>, String> {
    wireless::mdns_services(&adb()).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn wireless_pair(address: String, code: String) -> Result<String, String> {
    wireless::pair(&adb().with_timeout(Duration::from_secs(40)), &address, &code).await.map_err(|e| e.to_string())
}

/// Connect, and remember the phone (with its mDNS name when it can be found).
async fn connect_and_remember(state: &AppState, address: &str, fallback_name: Option<&str>) -> Result<String, String> {
    let a = adb().with_timeout(Duration::from_secs(25));
    let msg = wireless::connect(&a, address).await.map_err(|e| e.to_string())?;
    let mdns_name = wireless::mdns_services(&a)
        .await
        .ok()
        .and_then(|v| v.into_iter().find(|s| s.kind == wireless::MdnsKind::Connect && s.address == address))
        .map(|s| s.name);
    let name = fallback_name.map(str::to_string).or_else(|| mdns_name.clone()).unwrap_or_else(|| address.to_string());
    remember(state, name, address, mdns_name);
    Ok(msg)
}

#[tauri::command]
async fn wireless_connect(state: State<'_, AppState>, address: String) -> Result<String, String> {
    connect_and_remember(&state, &address, None).await
}

#[tauri::command]
async fn wireless_disconnect(address: String) -> Result<(), String> {
    wireless::disconnect(&adb(), &address).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn wireless_usb_to_wireless(state: State<'_, AppState>, serial: String) -> Result<String, String> {
    let a = adb().with_timeout(Duration::from_secs(25));
    let address = wireless::usb_to_wireless(&a, &serial, 5555).await.map_err(|e| e.to_string())?;
    remember(&state, format!("{serial} (Wi-Fi)"), &address, None);
    Ok(address)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QrStart {
    svg: String,
    name: String,
    password: String,
    id: u64,
}

#[tauri::command]
fn wireless_qr_start(state: State<'_, AppState>) -> Result<QrStart, String> {
    let s = wireless::new_qr_session().ok_or("could not create the QR code")?;
    let id = state.qr_generation.fetch_add(1, Ordering::SeqCst) + 1;
    Ok(QrStart { svg: s.svg, name: s.name, password: s.password, id })
}

#[tauri::command]
fn wireless_qr_cancel(state: State<'_, AppState>) {
    state.qr_generation.fetch_add(1, Ordering::SeqCst);
}

/// Waits (up to 90 s) for the phone to scan the QR code, pairs, then connects.
#[tauri::command]
async fn wireless_qr_wait(state: State<'_, AppState>, name: String, password: String, id: u64) -> Result<String, String> {
    let a = adb().with_timeout(Duration::from_secs(40));
    let cancelled = || state.qr_generation.load(Ordering::SeqCst) != id;
    let deadline = Instant::now() + Duration::from_secs(90);
    let pairing = loop {
        if cancelled() {
            return Err("Cancelled.".into());
        }
        if Instant::now() > deadline {
            return Err("Timed out waiting for the phone. On the phone choose Wireless debugging > Pair device with QR code, and scan the code.".into());
        }
        let found = wireless::mdns_services(&a)
            .await
            .ok()
            .and_then(|v| v.into_iter().find(|s| s.name == name && s.kind == wireless::MdnsKind::Pairing));
        if let Some(s) = found {
            break s;
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
    };
    wireless::pair(&a, &pairing.address, &password).await.map_err(|e| e.to_string())?;
    // After pairing the phone advertises its connect port on the same IP.
    let ip = pairing.address.rsplit_once(':').map(|(h, _)| h.to_string()).unwrap_or_default();
    for _ in 0..8 {
        if cancelled() {
            return Err("Cancelled.".into());
        }
        let svc = wireless::mdns_services(&a).await.ok().and_then(|v| {
            v.into_iter().find(|s| s.kind == wireless::MdnsKind::Connect && s.address.starts_with(&format!("{ip}:")))
        });
        if let Some(s) = svc {
            return connect_and_remember(&state, &s.address, None).await;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    Ok("Paired. Now connect using the port shown under Wireless debugging.".into())
}

#[tauri::command]
fn saved_devices(state: State<'_, AppState>) -> Vec<SavedDevice> {
    state.saved.lock().unwrap().clone()
}

#[tauri::command]
fn forget_device(state: State<'_, AppState>, address: String) {
    state.saved.lock().unwrap().retain(|d| d.address != address);
    persist_saved(&state);
}

/// Reconnect to a saved phone, re-discovering its port by mDNS name when it has changed.
#[tauri::command]
async fn wireless_reconnect(state: State<'_, AppState>, address: String) -> Result<String, String> {
    let saved = state.saved.lock().unwrap().iter().find(|d| d.address == address).cloned();
    let Some(saved) = saved else { return Err("Unknown saved device.".into()) };
    let a = adb().with_timeout(Duration::from_secs(25));
    let mut target = saved.address.clone();
    if let Some(n) = &saved.mdns_name {
        if let Ok(v) = wireless::mdns_services(&a).await {
            if let Some(s) = v.into_iter().find(|s| s.kind == wireless::MdnsKind::Connect && &s.name == n) {
                target = s.address;
            }
        }
    }
    let msg = connect_and_remember(&state, &target, Some(&saved.name)).await?;
    if target != saved.address {
        // Drop the stale entry that had the old port.
        state.saved.lock().unwrap().retain(|d| d.address != saved.address);
        persist_saved(&state);
    }
    Ok(msg)
}

// ---- snapshots and history ----

fn data_subdir(state: &AppState, sub: &str) -> Result<std::path::PathBuf, String> {
    let base = state.data_dir.lock().unwrap().clone().ok_or("The app data folder is not available.")?;
    Ok(base.join(sub))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

async fn device_model(a: &AdbRunner, serial: &str) -> Option<String> {
    devices::list_devices(a).await.ok()?.into_iter().find(|d| d.serial == serial)?.model
}

async fn take_snapshot(
    state: &AppState,
    a: &AdbRunner,
    serial: &str,
    name: &str,
    auto: bool,
) -> Result<snapshot::SnapshotMeta, String> {
    let dir = data_subdir(state, "snapshots")?;
    let states = packages::list_states(a, serial).await.map_err(|e| e.to_string())?;
    if states.is_empty() {
        return Err("No packages were found, so nothing was saved. Is the phone connected and authorised?".into());
    }
    let entries = states.into_iter().map(|p| snapshot::PackageEntry { name: p.name, state: p.state }).collect();
    let snap = snapshot::Snapshot::new(name, now_secs(), auto, device_model(a, serial).await, entries);
    snapshot::save(&dir, &snap).map_err(|e| format!("Could not save the snapshot: {e}"))?;
    if auto {
        snapshot::prune_auto(&dir, 20);
    }
    Ok(snap.meta())
}

#[tauri::command]
async fn snapshot_create(state: State<'_, AppState>, serial: String, name: String, auto: bool) -> Result<snapshot::SnapshotMeta, String> {
    let name = if name.trim().is_empty() { "Snapshot".to_string() } else { name };
    take_snapshot(&state, &adb(), &serial, &name, auto).await
}

#[tauri::command]
fn snapshot_list(state: State<'_, AppState>) -> Result<Vec<snapshot::SnapshotMeta>, String> {
    Ok(snapshot::list(&data_subdir(&state, "snapshots")?))
}

#[tauri::command]
fn snapshot_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    snapshot::delete(&data_subdir(&state, "snapshots")?, &id).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanResponse {
    plan: snapshot::RestorePlan,
    snapshot_name: String,
    snapshot_model: Option<String>,
    device_model: Option<String>,
}

async fn current_map(a: &AdbRunner, serial: &str) -> Result<HashMap<String, packages::PkgState>, String> {
    let states = packages::list_states(a, serial).await.map_err(|e| e.to_string())?;
    Ok(states.into_iter().map(|p| (p.name, p.state)).collect())
}

#[tauri::command]
async fn snapshot_plan(state: State<'_, AppState>, serial: String, id: String) -> Result<PlanResponse, String> {
    let a = adb();
    let snap = snapshot::load(&data_subdir(&state, "snapshots")?, &id).map_err(|e| format!("Could not read the snapshot: {e}"))?;
    let plan = snapshot::plan_restore(&snap, &current_map(&a, &serial).await?);
    Ok(PlanResponse { plan, snapshot_name: snap.name, snapshot_model: snap.device_model, device_model: device_model(&a, &serial).await })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreResponse {
    results: Vec<ActionResult>,
    /// Changes still needed after restoring (0 means the device now matches the snapshot).
    remaining: usize,
    /// The automatic snapshot taken just before restoring, so the restore itself can be undone.
    undo_snapshot: Option<snapshot::SnapshotMeta>,
}

#[tauri::command]
async fn snapshot_restore(state: State<'_, AppState>, serial: String, id: String) -> Result<RestoreResponse, String> {
    let a = adb();
    let dir = data_subdir(&state, "snapshots")?;
    let snap = snapshot::load(&dir, &id).map_err(|e| format!("Could not read the snapshot: {e}"))?;
    let plan = snapshot::plan_restore(&snap, &current_map(&a, &serial).await?);

    // Safety net first: if this snapshot saving fails, nothing has been changed yet.
    let undo = take_snapshot(&state, &a, &serial, &format!("Before restoring {}", snap.name), true).await?;

    let results = snapshot::execute(&a, &serial, &plan)
        .await
        .into_iter()
        .map(|o| ActionResult { name: o.name, ok: o.ok, message: o.message })
        .collect();
    let remaining = snapshot::plan_restore(&snap, &current_map(&a, &serial).await?).changes.len();
    Ok(RestoreResponse { results, remaining, undo_snapshot: Some(undo) })
}

#[tauri::command]
fn snapshot_export(state: State<'_, AppState>, id: String, path: String) -> Result<(), String> {
    let snap = snapshot::load(&data_subdir(&state, "snapshots")?, &id).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Could not write {path}: {e}"))
}

#[tauri::command]
fn snapshot_import(state: State<'_, AppState>, path: String) -> Result<snapshot::SnapshotMeta, String> {
    let meta = std::fs::metadata(&path).map_err(|e| format!("Could not open the file: {e}"))?;
    if meta.len() > 20_000_000 {
        return Err("That file is too large to be a snapshot.".into());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Could not read the file: {e}"))?;
    let snap = snapshot::import_from_str(&text, now_secs())?;
    snapshot::save(&data_subdir(&state, "snapshots")?, &snap).map_err(|e| e.to_string())?;
    Ok(snap.meta())
}

#[tauri::command]
fn history_load(state: State<'_, AppState>) -> serde_json::Value {
    data_subdir(&state, "history.json")
        .ok()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Array(vec![]))
}

#[tauri::command]
fn history_save(state: State<'_, AppState>, entries: Vec<serde_json::Value>) -> Result<(), String> {
    let file = data_subdir(&state, "history.json")?;
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let kept: Vec<_> = entries.into_iter().take(500).collect();
    std::fs::write(file, serde_json::to_string(&kept).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

// ---- device info and health ----

#[tauri::command]
async fn device_info(serial: String) -> Result<deviceinfo::DeviceInfo, String> {
    deviceinfo::fetch(&adb(), &serial).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn health_sample(serial: String) -> Result<health::HealthSample, String> {
    health::sample(&adb().with_timeout(Duration::from_secs(20)), &serial).await.map_err(|e| e.to_string())
}

// ---- tuning ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StepResult {
    label: String,
    ok: bool,
    message: Option<String>,
}

fn persist_tuning(state: &AppState) {
    if let Ok(file) = data_subdir(state, "tuning.json") {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string(&*state.tuning.lock().unwrap()) {
            let _ = std::fs::write(file, json);
        }
    }
}

/// Keep the first-ever previous value for a target: applying twice must not make the "original" drift.
fn store_record(state: &AppState, mut rec: tuning::Record) {
    {
        let mut all = state.tuning.lock().unwrap();
        if let Some(old) = all.iter_mut().find(|r| r.serial == rec.serial && r.target == rec.target) {
            rec.prev = old.prev.clone();
            rec.id = old.id.clone();
            *old = rec;
        } else {
            all.push(rec);
        }
    }
    persist_tuning(state);
}

async fn apply_steps(state: &AppState, a: &AdbRunner, serial: &str, steps: Vec<tuning::Step>) -> Vec<StepResult> {
    let mut out = Vec::new();
    for st in steps {
        match tuning::apply(a, serial, &st.target, st.value, st.group, st.label, now_secs()).await {
            Ok(rec) => {
                store_record(state, rec);
                out.push(StepResult { label: st.label.to_string(), ok: true, message: None });
            }
            Err(e) => out.push(StepResult { label: st.label.to_string(), ok: false, message: Some(e.to_string()) }),
        }
    }
    out
}

/// Restricting background work of core packages could break calls, notifications or sync.
fn may_restrict(pkg: &str) -> Result<(), String> {
    match ratings().lookup(pkg) {
        Some(r) if r.risk == "unsafe" || r.disable == ratings::Safety::Unsafe => {
            Err("Rated unsafe: restricting it could break calls, notifications or sync.".to_string())
        }
        _ => Ok(()),
    }
}

async fn restrict_one(state: &AppState, a: &AdbRunner, serial: &str, pkg: &str) -> StepResult {
    let label = format!("Restrict background: {pkg}");
    if !packages::valid_name(pkg) {
        return StepResult { label, ok: false, message: Some("Not a valid package name.".into()) };
    }
    if let Err(m) = may_restrict(pkg) {
        return StepResult { label, ok: false, message: Some(m) };
    }
    let mut ok = false;
    let mut notes = Vec::new();
    for (i, (target, value, step_label)) in tuning::background_steps(pkg).into_iter().enumerate() {
        match tuning::apply(a, serial, &target, value, "background", step_label, now_secs()).await {
            Ok(rec) => {
                store_record(state, rec);
                if i == 0 {
                    ok = true;
                }
            }
            // The standby bucket needs Android 9; the app-op alone is still useful.
            Err(e) if i == 1 && ok => notes.push(format!("standby bucket not set ({e})")),
            Err(e) => return StepResult { label, ok: false, message: Some(e.to_string()) },
        }
    }
    StepResult { label, ok, message: (!notes.is_empty()).then(|| notes.join("; ")) }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TuningInfo {
    status: tuning::Status,
    /// The 60 Hz cap only makes sense when the screen can go above 60 Hz.
    refresh_supported: bool,
    records: Vec<tuning::Record>,
}

#[tauri::command]
async fn tuning_status(state: State<'_, AppState>, serial: String) -> Result<TuningInfo, String> {
    let status = tuning::read_status(&adb(), &serial).await.map_err(|e| e.to_string())?;
    let records = state.tuning.lock().unwrap().iter().filter(|r| r.serial == serial).cloned().collect();
    Ok(TuningInfo { refresh_supported: status.max_fps.map_or(false, |f| f > 61.0), status, records })
}

/// Turn one of the quick toggles (`animations`, `battery_saver`, `refresh`) on or off.
#[tauri::command]
async fn tuning_toggle(state: State<'_, AppState>, serial: String, id: String, on: bool) -> Result<Vec<StepResult>, String> {
    let a = adb();
    let group = match id.as_str() {
        "animations" | "battery_saver" | "refresh" => id.clone(),
        _ => return Err("Unknown setting.".into()),
    };
    if on {
        let steps = match id.as_str() {
            "animations" => tuning::animation_steps(),
            "battery_saver" => tuning::battery_saver_steps(),
            _ => {
                let st = tuning::read_status(&a, &serial).await.map_err(|e| e.to_string())?;
                if !st.max_fps.map_or(false, |f| f > 61.0) {
                    return Err("This screen has no refresh rate above 60 Hz, so there is nothing to cap.".into());
                }
                tuning::refresh_steps()
            }
        };
        return Ok(apply_steps(&state, &a, &serial, steps).await);
    }
    Ok(undo_matching(&state, &a, &serial, Some(&group), None, None).await)
}

/// Set the three animation speeds separately (window, transition, in-app). Undo restores the originals.
#[tauri::command]
async fn tuning_animations(state: State<'_, AppState>, serial: String, window: String, transition: String, animator: String) -> Result<Vec<StepResult>, String> {
    let steps = tuning::animation_mix_steps(&window, &transition, &animator).ok_or("That animation speed is not one of the offered choices.")?;
    Ok(apply_steps(&state, &adb(), &serial, steps).await)
}

async fn undo_matching(
    state: &AppState,
    a: &AdbRunner,
    serial: &str,
    group: Option<&str>,
    id: Option<&str>,
    package: Option<&str>,
) -> Vec<StepResult> {
    let todo: Vec<tuning::Record> = state
        .tuning
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.serial == serial)
        .filter(|r| group.map_or(true, |g| r.group == g))
        .filter(|r| id.map_or(true, |i| r.id == i))
        .filter(|r| {
            package.map_or(true, |p| match &r.target {
                tuning::Target::AppOp { pkg, .. } | tuning::Target::Standby { pkg } => pkg == p,
                _ => false,
            })
        })
        .cloned()
        .collect();
    let mut out = Vec::new();
    // Undo newest first, mirroring how the changes were made.
    for r in todo.iter().rev() {
        match tuning::undo(a, serial, r).await {
            Ok(()) => {
                state.tuning.lock().unwrap().retain(|x| x.id != r.id);
                out.push(StepResult { label: format!("Undone: {}", r.label), ok: true, message: None });
            }
            Err(e) => out.push(StepResult { label: format!("Undo: {}", r.label), ok: false, message: Some(e.to_string()) }),
        }
    }
    persist_tuning(state);
    out
}

#[tauri::command]
async fn tuning_undo(
    state: State<'_, AppState>,
    serial: String,
    group: Option<String>,
    id: Option<String>,
    package: Option<String>,
) -> Result<Vec<StepResult>, String> {
    Ok(undo_matching(&state, &adb(), &serial, group.as_deref(), id.as_deref(), package.as_deref()).await)
}

#[tauri::command]
async fn tuning_restrict(state: State<'_, AppState>, serial: String, names: Vec<String>, restrict: bool) -> Result<Vec<StepResult>, String> {
    let a = adb();
    let mut out = Vec::new();
    for n in names {
        if restrict {
            out.push(restrict_one(&state, &a, &serial, &n).await);
        } else {
            if !packages::valid_name(&n) {
                out.push(StepResult { label: format!("Allow background: {n}"), ok: false, message: Some("Not a valid package name.".into()) });
                continue;
            }
            let mut r = undo_matching(&state, &a, &serial, Some("background"), None, Some(&n)).await;
            if r.is_empty() {
                r.push(StepResult { label: format!("Allow background: {n}"), ok: true, message: Some("It had no saved restriction from this app.".into()) });
            }
            out.extend(r);
        }
    }
    Ok(out)
}

#[tauri::command]
async fn tuning_trim_caches(serial: String) -> Result<(), String> {
    tuning::trim_caches(&adb().with_timeout(Duration::from_secs(60)), &serial).await.map_err(|e| e.to_string())
}

/// The one-click cool-down: battery saver, calmer animations, 60 Hz when the screen allows more,
/// and background restrictions (and optionally a stop) for the chosen apps. Every step is reversible.
#[tauri::command]
async fn tuning_cooldown(state: State<'_, AppState>, serial: String, apps: Vec<String>, stop: bool) -> Result<Vec<StepResult>, String> {
    let a = adb();
    let mut out = Vec::new();
    out.extend(apply_steps(&state, &a, &serial, tuning::battery_saver_steps()).await);
    out.extend(apply_steps(&state, &a, &serial, tuning::animation_steps()).await);
    if let Ok(st) = tuning::read_status(&a, &serial).await {
        if st.max_fps.map_or(false, |f| f > 61.0) {
            out.extend(apply_steps(&state, &a, &serial, tuning::refresh_steps()).await);
        }
    }
    for n in &apps {
        out.push(restrict_one(&state, &a, &serial, n).await);
    }
    if stop {
        for n in &apps {
            let r = if may_restrict(n).is_ok() { packages::force_stop(&a, &serial, n).await.map_err(|e| e.to_string()) } else { Err("skipped (rated unsafe)".into()) };
            out.push(StepResult { label: format!("Stop {n}"), ok: r.is_ok(), message: r.err() });
        }
    }
    Ok(out)
}

// ---- device report ----

#[derive(Serialize)]
struct ReportSection {
    name: String,
    ok: bool,
    bytes: usize,
}

#[derive(Serialize)]
struct ReportOut {
    text: String,
    sections: Vec<ReportSection>,
}

#[tauri::command]
async fn device_report(serial: String) -> Result<ReportOut, String> {
    let a = adb();
    let mut sections = report::collect(&a, &serial).await;
    // Which preinstalled apps still have no rating: names of system apps only, so ratings can grow from real phones.
    if let Ok(states) = packages::list_states(&a, &serial).await {
        let r = ratings();
        let mut names: Vec<String> = states.into_iter().filter(|p| p.system && r.lookup(&p.name).is_none()).map(|p| p.name).collect();
        names.sort();
        let text = format!("{} preinstalled apps have no rating yet:\n{}", names.len(), names.join("\n"));
        sections.push(report::Section { name: "preinstalled apps without a rating".into(), command: "(computed from the app list and the built-in ratings)".into(), ok: true, output: report::truncate(&text, 40_000) });
    }
    let text = report::render(&report::Meta { app_version: env!("CARGO_PKG_VERSION"), unix_time: now_secs() }, &sections);
    let sections = sections.iter().map(|s| ReportSection { name: s.name.clone(), ok: s.ok, bytes: s.output.len() }).collect();
    Ok(ReportOut { text, sections })
}

/// Write text the user chose to save (from the save dialog). Limited in size.
#[tauri::command]
fn save_text_file(path: String, text: String) -> Result<(), String> {
    if text.len() > 5_000_000 {
        return Err("That is too large to save.".into());
    }
    std::fs::write(&path, text).map_err(|e| format!("Could not write {path}: {e}"))
}

// ---- command log ----

#[derive(Serialize)]
struct LogOut {
    entries: Vec<cmdlog::Entry>,
    seq: u64,
}

#[tauri::command]
fn get_log(since: u64) -> LogOut {
    let (entries, seq) = cmdlog::since(since);
    LogOut { entries, seq }
}

#[tauri::command]
fn clear_log() {
    cmdlog::clear();
}

// ---- setting up adb ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdbStatus {
    found: bool,
    /// `env`, `appData`, `nextToApp` or `path`.
    source: Option<platform_tools::Source>,
    path: Option<String>,
    version: Option<String>,
    major: Option<u32>,
}

#[tauri::command]
async fn adb_status() -> AdbStatus {
    let Some(f) = find_adb() else {
        return AdbStatus { found: false, source: None, path: None, version: None, major: None };
    };
    let out = AdbRunner::new(&f.path).with_timeout(Duration::from_secs(10)).run(&["version"]).await.ok();
    let major = out.as_deref().and_then(wireless::parse_adb_major);
    let version = out.as_deref().and_then(|o| o.lines().find(|l| l.trim_start().starts_with("Version")).map(|l| l.trim().to_string()));
    AdbStatus { found: major.is_some(), source: Some(f.source), path: Some(f.path.to_string_lossy().into_owned()), version, major }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AdbDownloadEvent {
    /// `downloading`, `unpacking`, `done` or `error`.
    stage: &'static str,
    received: u64,
    total: Option<u64>,
    message: Option<String>,
}

static ADB_DOWNLOADING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Download Google's platform-tools into the app's own folder. Progress arrives as `adb-download` events.
#[tauri::command]
async fn adb_download(app: AppHandle) -> Result<(), String> {
    let Some(dir) = APP_DATA.get().cloned() else { return Err("The app data folder is not available.".into()) };
    if ADB_DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("A download is already running.".into());
    }
    tauri::async_runtime::spawn(async move {
        let a2 = app.clone();
        let progress: platform_tools::Progress = std::sync::Arc::new(move |received, total| {
            let _ = a2.emit("adb-download", AdbDownloadEvent { stage: "downloading", received, total, message: None });
        });
        let url = platform_tools::download_url(platform_tools::current_os());
        let result = platform_tools::install(&dir, &url, &[platform_tools::GOOGLE_HOST], false, progress).await;
        let ev = match result {
            Ok(_) => {
                let _ = adb().start_server().await;
                AdbDownloadEvent { stage: "done", received: 0, total: None, message: None }
            }
            Err(e) => AdbDownloadEvent { stage: "error", received: 0, total: None, message: Some(e) },
        };
        ADB_DOWNLOADING.store(false, Ordering::SeqCst);
        let _ = app.emit("adb-download", ev);
    });
    Ok(())
}

// ---- apps you are using right now ----

/// Apps the phone uses for its main jobs; the wizard never offers them.
#[tauri::command]
async fn get_protections(serial: String) -> Result<Vec<protect::Protection>, String> {
    protect::detect(&adb().with_timeout(Duration::from_secs(20)), &serial).await.map_err(|e| e.to_string())
}

// ---- install tool and app backup ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectOut {
    item: Option<install::InstallItem>,
    error: Option<String>,
}

/// Look at what was dropped or chosen and queue what can be installed.
#[tauri::command]
async fn install_inspect(state: State<'_, AppState>, paths: Vec<String>) -> Result<Vec<InspectOut>, String> {
    let paths: Vec<std::path::PathBuf> = paths.into_iter().take(50).map(Into::into).collect();
    let results = tokio::task::spawn_blocking(move || install::inspect(&paths)).await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in results {
        match r {
            Ok(item) => {
                state.installs.lock().unwrap().insert(item.id.clone(), item.clone());
                out.push(InspectOut { item: Some(item), error: None });
            }
            Err(e) => out.push(InspectOut { item: None, error: Some(e) }),
        }
    }
    Ok(out)
}

#[tauri::command]
async fn install_item(state: State<'_, AppState>, serial: String, id: String, replace: bool, allow_downgrade: bool, allow_test: bool) -> Result<String, String> {
    let item = state.installs.lock().unwrap().get(&id).cloned().ok_or("That item is no longer queued. Add it again.")?;
    install::install(&adb(), &serial, &item, install::Options { replace, allow_downgrade, allow_test }).await
}

#[tauri::command]
fn install_discard(state: State<'_, AppState>, ids: Vec<String>) {
    let mut q = state.installs.lock().unwrap();
    for id in ids {
        if let Some(item) = q.remove(&id) {
            install::discard(&item);
        }
    }
}

/// Save one app's files into `<dest>/<package>/`.
#[tauri::command]
async fn backup_one(serial: String, package: String, dest: String, label: Option<String>, version_name: Option<String>, installer: Option<String>) -> Result<backup::ExportResult, String> {
    let dest = std::path::PathBuf::from(dest);
    if !dest.is_dir() {
        return Err("The folder to save into does not exist.".into());
    }
    backup::export(&adb(), &serial, &package, &dest, &backup::ExportInfo { label, version_name, installer }).await
}

// ---- storage and last opened ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageReport {
    /// False when the phone did not report sizes (some Android versions or makers do not).
    available: bool,
    apps: Vec<storage::AppStorage>,
}

#[tauri::command]
async fn storage_report(serial: String) -> Result<StorageReport, String> {
    let apps = storage::read_sizes(&adb().with_timeout(Duration::from_secs(60)), &serial).await.map_err(|e| e.to_string())?;
    Ok(StorageReport { available: !apps.is_empty(), apps })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageReport {
    available: bool,
    entries: Vec<storage::Usage>,
}

#[tauri::command]
async fn usage_report(serial: String) -> Result<UsageReport, String> {
    let entries = storage::read_usage(&adb().with_timeout(Duration::from_secs(90)), &serial).await.map_err(|e| e.to_string())?;
    Ok(UsageReport { available: !entries.is_empty(), entries })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WakeLockRow {
    package: Option<String>,
    uid: u32,
    kind: String,
    tag: String,
    held_secs: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WakeReport {
    available: bool,
    locks: Vec<WakeLockRow>,
    wakeups: Vec<wakelocks::Wakeups>,
}

/// Held wake locks (matched to apps by user id) and the apps that wake the phone most.
#[tauri::command]
async fn wake_report(serial: String) -> Result<WakeReport, String> {
    let a = adb().with_timeout(Duration::from_secs(60));
    let data = wakelocks::read(&a, &serial).await.map_err(|e| e.to_string())?;
    let pk = packages::list_packages(&a, &serial, Filter::All, false).await.unwrap_or_default();
    let by_uid = |uid: u32| pk.iter().find(|p| p.uid == Some(uid)).map(|p| p.name.clone());
    let locks = data
        .locks
        .into_iter()
        .map(|l| WakeLockRow { package: by_uid(l.uid), uid: l.uid, kind: l.kind, tag: l.tag, held_secs: l.held_secs })
        .collect::<Vec<_>>();
    Ok(WakeReport { available: !locks.is_empty() || !data.wakeups.is_empty(), locks, wakeups: data.wakeups })
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            let data = app.path().app_data_dir().ok();
            if let Some(d) = &data {
                let _ = APP_DATA.set(d.clone()); // before anything looks for adb
            }
            // Start the adb server once, hidden, instead of leaving it to whichever call comes first.
            tauri::async_runtime::spawn(async { let _ = adb().start_server().await; });
            *app.state::<AppState>().data_dir.lock().unwrap() = data.clone();
            if let Some(f) = data.as_ref().map(|d| d.join("tuning.json")) {
                if let Ok(t) = std::fs::read_to_string(f) {
                    if let Ok(v) = serde_json::from_str(&t) {
                        *app.state::<AppState>().tuning.lock().unwrap() = v;
                    }
                }
            }
            // Optional user rating files: <app-data>/ratings/*.json override built-ins.
            let dir = data.as_ref().map(|d| d.join("ratings"));
            let _ = RATINGS.set(ratings::Ratings::load(dir.as_deref()));
            // Persistent label cache.
            if let Some(d) = data {
                let file = d.join("labels.json");
                let state = app.state::<AppState>();
                if let Ok(text) = std::fs::read_to_string(&file) {
                    if let Ok(m) = serde_json::from_str(&text) {
                        *state.label_cache.lock().unwrap() = m;
                    }
                }
                *state.cache_file.lock().unwrap() = Some(file);
                let saved_file = d.join("saved_devices.json");
                if let Ok(text) = std::fs::read_to_string(&saved_file) {
                    if let Ok(v) = serde_json::from_str(&text) {
                        *state.saved.lock().unwrap() = v;
                    }
                }
                *state.saved_file.lock().unwrap() = Some(saved_file);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_devices, list_apps, apply_action, list_running, load_labels,
            adb_check, wireless_discover, wireless_pair, wireless_connect, wireless_disconnect, wireless_usb_to_wireless,
            wireless_qr_start, wireless_qr_cancel, wireless_qr_wait, saved_devices, forget_device, wireless_reconnect,
            snapshot_create, snapshot_list, snapshot_delete, snapshot_plan, snapshot_restore, snapshot_export, snapshot_import,
            history_load, history_save, device_info, health_sample,
            tuning_status, tuning_toggle, tuning_animations, tuning_undo, tuning_restrict, tuning_trim_caches, tuning_cooldown,
            device_report, save_text_file, get_log, clear_log, adb_status, adb_download, get_protections,
            install_inspect, install_item, install_discard, backup_one,
            storage_report, usage_report, wake_report])
        .run(tauri::generate_context!())
        .expect("error while running Coolant");
}
