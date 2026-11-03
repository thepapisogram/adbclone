//! Copying installed apps off the phone as .apk files, including every split part, with a small info file.
//! Only the app itself is saved, not its data. Restore with the install tool.
use crate::install::safe_name;
use crate::labels::valid_device_path;
use crate::packages::valid_name;
use crate::{AdbError, AdbRunner};
use serde::Serialize;
use std::{path::Path, time::Duration};

/// `pm path` prints `package:/data/app/.../base.apk` once per part.
pub fn parse_pm_path(out: &str) -> Vec<String> {
    out.lines().filter_map(|l| l.trim().strip_prefix("package:")).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

pub async fn device_apk_paths(adb: &AdbRunner, serial: &str, pkg: &str) -> Result<Vec<String>, AdbError> {
    if !valid_name(pkg) {
        return Err(AdbError::Device(format!("invalid package name: {pkg}")));
    }
    Ok(parse_pm_path(&adb.shell(serial, &[&format!("pm path {pkg}")]).await.unwrap_or_default()))
}

#[derive(Debug, Clone, Default)]
pub struct ExportInfo {
    pub label: Option<String>,
    pub version_name: Option<String>,
    pub installer: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub package: String,
    pub folder: String,
    pub files: Vec<String>,
    pub bytes: u64,
}

/// Save one app into `<dest_root>/<package>/`. Errors are plain sentences.
pub async fn export(adb: &AdbRunner, serial: &str, pkg: &str, dest_root: &Path, info: &ExportInfo) -> Result<ExportResult, String> {
    let paths = device_apk_paths(adb, serial, pkg).await.map_err(|e| e.to_string())?;
    if paths.is_empty() {
        return Err("This app is not installed for you right now, so its files cannot be copied.".into());
    }
    if let Some(bad) = paths.iter().find(|p| !valid_device_path(p)) {
        return Err(format!("The app's file has an unusual path and was skipped: {bad}"));
    }
    let folder = dest_root.join(pkg);
    std::fs::create_dir_all(&folder).map_err(|e| format!("Could not create the folder: {e}"))?;
    let slow = adb.clone().with_timeout(Duration::from_secs(900));
    let (mut files, mut bytes, mut used) = (Vec::new(), 0u64, std::collections::HashSet::new());
    for p in &paths {
        let mut name = safe_name(p.rsplit('/').next().unwrap_or("app.apk"));
        if !name.to_ascii_lowercase().ends_with(".apk") {
            name.push_str(".apk");
        }
        while !used.insert(name.clone()) {
            name = format!("{}_{}", crate::wireless::random_token(3), name);
        }
        let local = folder.join(&name);
        let local_s = local.to_string_lossy().into_owned();
        let out = slow.run_full(&["-s", serial, "pull", p, &local_s]).await.map_err(|e| e.to_string())?;
        if out.code != Some(0) {
            return Err(format!("Copying {name} failed: {}", out.stderr.trim().lines().next().unwrap_or("unknown error")));
        }
        bytes += std::fs::metadata(&local).map(|m| m.len()).unwrap_or(0);
        files.push(name);
    }
    let meta = serde_json::json!({
        "package": pkg, "label": info.label, "versionName": info.version_name, "installer": info.installer,
        "files": files, "splitApp": files.len() > 1, "savedBy": "Coolant",
        "note": "Only the app itself is saved, not its data. Restore it with Coolant's install tool by dropping this folder.",
    });
    std::fs::write(folder.join("info.json"), serde_json::to_string_pretty(&meta).unwrap_or_default()).map_err(|e| format!("Could not save the info file: {e}"))?;
    Ok(ExportResult { package: pkg.to_string(), folder: folder.to_string_lossy().into_owned(), files, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pm_path_lists_every_part() {
        let out = "package:/data/app/~~x==/com.a-y==/base.apk\npackage:/data/app/~~x==/com.a-y==/split_config.arm64_v8a.apk\n\n  garbage\n";
        assert_eq!(parse_pm_path(out), ["/data/app/~~x==/com.a-y==/base.apk", "/data/app/~~x==/com.a-y==/split_config.arm64_v8a.apk"]);
        assert!(parse_pm_path("").is_empty());
        assert!(parse_pm_path("package:\n").is_empty());
    }
}
