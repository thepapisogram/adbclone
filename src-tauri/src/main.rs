#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ratings;

use adb_core::{devices, labels, packages, processes, AdbRunner};
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
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_devices, list_apps, apply_action, list_running, load_labels])
        .run(tauri::generate_context!())
        .expect("error while running ADB Manager");
}
