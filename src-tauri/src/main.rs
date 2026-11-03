#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ratings;

use adb_core::{devices, labels, packages, processes, wireless, AdbRunner};
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

/// adb lookup order: $ADB_PATH, platform-tools next to the executable, then PATH.
fn adb() -> AdbRunner {
    let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
    if let Ok(p) = std::env::var("ADB_PATH") {
        return AdbRunner::new(p);
    }
    if let Ok(me) = std::env::current_exe() {
        if let Some(dir) = me.parent() {
            for cand in [dir.join("platform-tools").join(exe), dir.join(exe)] {
                if cand.exists() {
                    return AdbRunner::new(cand);
                }
            }
        }
    }
    AdbRunner::new(exe)
}


#[tauri::command]
async fn list_devices() -> Result<Vec<devices::Device>, String> {
    devices::list_devices(&adb()).await.map_err(|e| {
        format!("{e}. Install Android platform-tools and put `adb` on PATH (or set ADB_PATH).")
    })
}

#[tauri::command]
async fn list_apps(serial: String) -> Result<Vec<AppInfo>, String> {
    let a = adb();
    let (ar, sr) = (&a, &serial);
    let names = |f, u| async move { packages::list_packages(ar, sr, f, u).await };
    let (all_u, installed, system, disabled) = tokio::try_join!(
        names(Filter::All, true),
        names(Filter::All, false),
        names(Filter::System, true),
        names(Filter::Disabled, false)
    )
    .map_err(|e| e.to_string())?;

    let set = |v: &[packages::Package]| v.iter().map(|p| p.name.clone()).collect::<HashSet<_>>();
    let (installed, system, disabled) = (set(&installed), set(&system), set(&disabled));
    let rules = ratings();

    let mut out: Vec<AppInfo> = all_u
        .into_iter()
        .map(|p| {
            let state = if !installed.contains(&p.name) {
                "uninstalled"
            } else if disabled.contains(&p.name) {
                "disabled"
            } else {
                "enabled"
            };
            let rule = rules.lookup(&p.name);
            AppInfo {
                label: labels::fallback_label(&p.name),
                system: system.contains(&p.name),
                state,
                risk: rule.as_ref().map_or("unknown", |r| r.risk),
                disable_safety: rule.as_ref().map_or("unknown", |r| r.disable.as_str()),
                uninstall_safety: rule.as_ref().map_or("unknown", |r| r.uninstall.as_str()),
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
}

#[derive(Serialize, Clone)]
struct LabelProgress {
    done: usize,
    total: usize,
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

/// Starts resolving real app names in the background; results arrive as `labels` events.
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
    // Third-party apps first: they are what people look for.
    let mut todo: Vec<(String, String)> =
        all.into_iter().filter_map(|p| p.apk_path.map(|path| (p.name, path))).collect();
    todo.sort_by_key(|(name, _)| system.contains(name));

    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let total = todo.len();
        let mut done = 0usize;
        for batch in todo.chunks(100) {
            if state.label_generation.load(Ordering::SeqCst) != gen {
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
                        Some(l) => ready.push(LabelItem { name: name.clone(), label: l.clone() }),
                        None => missing.push((name.clone(), path.clone(), k)),
                    }
                }
            }
            done += ready.len();
            if !ready.is_empty() {
                let _ = app.emit("labels", ready);
            }
            // Resolve the rest, four at a time.
            for group in missing.chunks(4) {
                if state.label_generation.load(Ordering::SeqCst) != gen {
                    return;
                }
                let tasks: Vec<_> = group
                    .iter()
                    .cloned()
                    .map(|(name, path, k)| {
                        let (a, serial) = (a.clone(), serial.clone());
                        tauri::async_runtime::spawn(async move {
                            let label = labels::fetch_label(&a, &serial, &path).await.ok().flatten();
                            (name, k, label)
                        })
                    })
                    .collect();
                let mut out = Vec::new();
                for t in tasks {
                    if let Ok((name, k, label)) = t.await {
                        if let Some(label) = label {
                            if let Some(k) = k {
                                state.label_cache.lock().unwrap().insert(k, label.clone());
                            }
                            out.push(LabelItem { name, label });
                        }
                    }
                }
                done += group.len();
                if !out.is_empty() {
                    let _ = app.emit("labels", out);
                }
                let _ = app.emit("labels-progress", LabelProgress { done: done.min(total), total });
            }
            let _ = app.emit("labels-progress", LabelProgress { done: done.min(total), total });
            save_label_cache(&state);
        }
        let _ = app.emit("labels-progress", LabelProgress { done: total, total });
    });
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

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            let data = app.path().app_data_dir().ok();
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
            wireless_qr_start, wireless_qr_cancel, wireless_qr_wait, saved_devices, forget_device, wireless_reconnect])
        .run(tauri::generate_context!())
        .expect("error while running ADB Manager");
}
