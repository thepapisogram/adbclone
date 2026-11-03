#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ratings;

use adb_core::{devices, packages, AdbRunner};
use packages::Filter;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::OnceLock;
use tauri::Manager;

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

fn label_from(name: &str) -> String {
    let last = name.rsplit('.').next().unwrap_or(name);
    let mut c = last.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => name.to_string(),
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
    let rules = RATINGS.get_or_init(|| ratings::Ratings::load(None));

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
                label: label_from(&p.name),
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

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Optional user rating files: <app-data>/ratings/*.json override built-ins.
            let dir = app.path().app_data_dir().ok().map(|d| d.join("ratings"));
            let _ = RATINGS.set(ratings::Ratings::load(dir.as_deref()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_devices, list_apps, apply_action])
        .run(tauri::generate_context!())
        .expect("error while running ADB Manager");
}
