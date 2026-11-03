//! Finding adb, and installing Google's official platform-tools when it is missing.
//! Downloads are HTTPS-only to an allowlisted host, size-capped, unpacked safely, and the result is
//! only accepted if the `adb` inside actually runs.
use crate::AdbRunner;
use serde::Serialize;
use std::{
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// Google's download host for the Android SDK Platform-Tools.
pub const GOOGLE_HOST: &str = "dl.google.com";
pub const MAX_DOWNLOAD_BYTES: u64 = 120 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 400 * 1024 * 1024;
const MAX_ENTRIES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

pub fn current_os() -> Os {
    if cfg!(windows) { Os::Windows } else if cfg!(target_os = "macos") { Os::Mac } else { Os::Linux }
}

pub fn adb_file_name(os: Os) -> &'static str {
    if os == Os::Windows { "adb.exe" } else { "adb" }
}

pub fn download_url(os: Os) -> String {
    let name = match os {
        Os::Windows => "windows",
        Os::Mac => "darwin",
        Os::Linux => "linux",
    };
    format!("https://{GOOGLE_HOST}/android/repository/platform-tools-latest-{name}.zip")
}

/// Scheme and host of a URL (no user info allowed), or `None` if it does not look like a plain URL.
fn scheme_and_host(url: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') || authority.contains(' ') {
        return None;
    }
    let host = authority.rsplit_once(':').map_or(authority, |(h, port)| if port.chars().all(|c| c.is_ascii_digit()) { h } else { authority });
    Some((scheme, host))
}

/// HTTPS to an allowlisted host. `allow_loopback_http` is only for tests against a local server.
pub fn url_allowed(url: &str, allowed_hosts: &[&str], allow_loopback_http: bool) -> bool {
    match scheme_and_host(url) {
        Some(("https", host)) => allowed_hosts.iter().any(|h| h.eq_ignore_ascii_case(host)),
        Some(("http", host)) => allow_loopback_http && (host == "127.0.0.1" || host == "localhost"),
        _ => false,
    }
}

/// A redirect target may be absolute or a path on the same server.
fn resolve_location(current: &str, location: &str) -> String {
    if location.starts_with('/') && !location.starts_with("//") {
        if let Some((scheme, rest)) = current.split_once("://") {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
            return format!("{scheme}://{authority}{location}");
        }
    }
    location.to_string()
}

// ---- finding adb ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Env,
    AppData,
    NextToApp,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub source: Source,
}

fn which(name: &str, path_var: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path_var?).map(|d| d.join(name)).find(|p| p.is_file())
}

/// Where adb is. Order: ADB_PATH, our own downloaded copy, next to the app, then the system PATH.
pub fn find_adb(env_adb_path: Option<&str>, app_data_dir: Option<&Path>, exe_dir: Option<&Path>, path_var: Option<&std::ffi::OsStr>, os: Os) -> Option<Found> {
    let exe = adb_file_name(os);
    if let Some(p) = env_adb_path.filter(|p| !p.trim().is_empty()) {
        return Some(Found { path: PathBuf::from(p), source: Source::Env });
    }
    if let Some(d) = app_data_dir {
        let p = d.join("platform-tools").join(exe);
        if p.is_file() {
            return Some(Found { path: p, source: Source::AppData });
        }
    }
    if let Some(d) = exe_dir {
        for p in [d.join("platform-tools").join(exe), d.join(exe)] {
            if p.is_file() {
                return Some(Found { path: p, source: Source::NextToApp });
            }
        }
    }
    which(exe, path_var).map(|path| Found { path, source: Source::Path })
}

// ---- unpacking ----

/// Unpack the downloaded zip into `<parent>/platform-tools`, replacing any old copy only once the new one
/// is complete. Returns the path of adb.
pub fn extract(zip_bytes: &[u8], target_dir: &Path, os: Os) -> Result<PathBuf, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(zip_bytes)).map_err(|_| "the download is not a valid zip file".to_string())?;
    if zip.len() > MAX_ENTRIES {
        return Err("the download has too many files".into());
    }
    let staging = target_dir.with_extension("new");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| format!("could not create a folder: {e}"))?;
    let mut total: u64 = 0;
    let result = (|| -> Result<(), String> {
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| format!("could not read the zip: {e}"))?;
            // `enclosed_name` refuses absolute paths and anything with `..`.
            let Some(rel) = f.enclosed_name() else { return Err("the zip contains an unsafe path".into()) };
            let Ok(inner) = rel.strip_prefix("platform-tools") else { continue };
            if inner.as_os_str().is_empty() {
                continue;
            }
            let out = staging.join(inner);
            if f.is_dir() {
                std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
                continue;
            }
            total += f.size();
            if total > MAX_UNPACKED_BYTES {
                return Err("the download is far larger than expected".into());
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut file = std::fs::File::create(&out).map_err(|e| format!("could not write {}: {e}", out.display()))?;
            let mut buf = Vec::with_capacity(f.size().min(8 * 1024 * 1024) as usize);
            f.by_ref().take(MAX_UNPACKED_BYTES).read_to_end(&mut buf).map_err(|e| format!("could not unpack: {e}"))?;
            file.write_all(&buf).map_err(|e| e.to_string())?;
            drop(file);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let is_tool = inner.components().count() == 1 && inner.extension().is_none();
                let mode = f.unix_mode().map(|m| m & 0o777).unwrap_or(if is_tool { 0o755 } else { 0o644 });
                let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(if is_tool { mode | 0o755 } else { mode | 0o600 }));
            }
        }
        if !staging.join(adb_file_name(os)).is_file() {
            return Err("the download does not contain adb".into());
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(target_dir);
    std::fs::rename(&staging, target_dir).map_err(|e| format!("could not move the new files into place: {e}"))?;
    Ok(target_dir.join(adb_file_name(os)))
}

// ---- downloading ----

pub type Progress = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

fn agent() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| format!("secure connections are not available: {e}"))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .redirects(0) // redirects are followed by hand so each target can be checked
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .build())
}

/// Blocking download with every rule enforced. Call from a blocking thread.
pub fn download_blocking(url: &str, allowed_hosts: &[&str], allow_loopback_http: bool, max_bytes: u64, progress: &Progress) -> Result<Vec<u8>, String> {
    let agent = agent()?;
    let mut current = url.to_string();
    for _ in 0..4 {
        if !url_allowed(&current, allowed_hosts, allow_loopback_http) {
            return Err("that address is not on the list of places this app downloads from".into());
        }
        let resp = match agent.get(&current).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) if (300..400).contains(&code) => r,
            Err(ureq::Error::Status(code, _)) => return Err(format!("the server answered with an error ({code})")),
            Err(e) => return Err(format!("could not connect: {e}")),
        };
        if (300..400).contains(&resp.status()) {
            let Some(next) = resp.header("Location") else { return Err("the server redirected without saying where".into()) };
            current = resolve_location(&current, next);
            continue;
        }
        let total = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok());
        if total.is_some_and(|t| t > max_bytes) {
            return Err("the download is larger than expected".into());
        }
        let mut reader = resp.into_reader().take(max_bytes + 1);
        let mut data = Vec::with_capacity(total.unwrap_or(8 * 1024 * 1024).min(max_bytes) as usize);
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut chunk).map_err(|e| format!("the download was interrupted: {e}"))?;
            if n == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..n]);
            if data.len() as u64 > max_bytes {
                return Err("the download is larger than expected".into());
            }
            progress(data.len() as u64, total);
        }
        if let Some(t) = total {
            if data.len() as u64 != t {
                return Err("the download was cut short".into());
            }
        }
        return Ok(data);
    }
    Err("too many redirects".into())
}

/// Everything needed to set adb up: download, unpack, and run `adb version` to confirm it works.
pub async fn install(app_data_dir: &Path, url: &str, allowed_hosts: &'static [&'static str], allow_loopback_http: bool, progress: Progress) -> Result<PathBuf, String> {
    let os = current_os();
    let url = url.to_string();
    let p2 = progress.clone();
    let bytes = tokio::task::spawn_blocking(move || download_blocking(&url, allowed_hosts, allow_loopback_http, MAX_DOWNLOAD_BYTES, &p2))
        .await
        .map_err(|e| format!("the download stopped: {e}"))??;
    let target = app_data_dir.join("platform-tools");
    let adb = {
        let target = target.clone();
        tokio::task::spawn_blocking(move || extract(&bytes, &target, os)).await.map_err(|e| format!("unpacking stopped: {e}"))??
    };
    let out = AdbRunner::new(&adb).with_timeout(Duration::from_secs(20)).run(&["version"]).await.map_err(|e| format!("the downloaded adb does not run: {e}"))?;
    if crate::wireless::parse_adb_major(&out).is_none() {
        return Err("the downloaded adb did not answer like adb".into());
    }
    Ok(adb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("adbm-pt-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            for (name, data) in entries {
                let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).unix_permissions(0o644);
                w.start_file(*name, o).unwrap();
                w.write_all(data).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn urls_are_checked() {
        let ok = ["dl.google.com"];
        assert!(url_allowed(&download_url(Os::Windows), &ok, false));
        assert!(url_allowed("https://DL.GOOGLE.COM/x.zip", &ok, false));
        for bad in ["http://dl.google.com/x.zip", "https://evil.example/x.zip", "https://dl.google.com.evil.example/x", "https://user@dl.google.com/x", "ftp://dl.google.com/x", "dl.google.com/x", "https://", "https://dl.google.com@evil.example/x"] {
            assert!(!url_allowed(bad, &ok, false), "{bad}");
        }
        assert!(!url_allowed("http://127.0.0.1:80/x", &ok, false), "plain http to localhost only in tests");
        assert!(url_allowed("http://127.0.0.1:8080/x", &ok, true));
        assert!(!url_allowed("http://evil.example/x", &ok, true));
        assert_eq!(download_url(Os::Mac), "https://dl.google.com/android/repository/platform-tools-latest-darwin.zip");
    }

    #[test]
    fn relative_redirects_stay_on_the_same_server_and_protocol_relative_ones_are_not_trusted() {
        assert_eq!(resolve_location("https://dl.google.com/a/b.zip", "/c/d.zip"), "https://dl.google.com/c/d.zip");
        assert_eq!(resolve_location("http://127.0.0.1:8080/x", "/y"), "http://127.0.0.1:8080/y");
        assert_eq!(resolve_location("https://dl.google.com/a", "https://other.example/z"), "https://other.example/z");
        // "//evil.example/x" must not be turned into a trusted address; it stays as-is and is then refused.
        assert_eq!(resolve_location("https://dl.google.com/a", "//evil.example/x"), "//evil.example/x");
        assert!(!url_allowed("//evil.example/x", &["dl.google.com"], false));
    }

    #[test]
    fn finds_adb_in_the_documented_order() {
        let (data, exe, path) = (tmp("fa-data"), tmp("fa-exe"), tmp("fa-path"));
        let name = adb_file_name(Os::Linux);
        let path_var = std::env::join_paths([&path]).unwrap();
        assert_eq!(find_adb(None, Some(&data), Some(&exe), Some(&path_var), Os::Linux), None);
        std::fs::write(path.join(name), "x").unwrap();
        assert_eq!(find_adb(None, Some(&data), Some(&exe), Some(&path_var), Os::Linux).unwrap().source, Source::Path);
        std::fs::write(exe.join(name), "x").unwrap();
        assert_eq!(find_adb(None, Some(&data), Some(&exe), Some(&path_var), Os::Linux).unwrap().source, Source::NextToApp);
        std::fs::create_dir_all(data.join("platform-tools")).unwrap();
        std::fs::write(data.join("platform-tools").join(name), "x").unwrap();
        assert_eq!(find_adb(None, Some(&data), Some(&exe), Some(&path_var), Os::Linux).unwrap().source, Source::AppData);
        let e = find_adb(Some("/custom/adb"), Some(&data), Some(&exe), Some(&path_var), Os::Linux).unwrap();
        assert_eq!((e.source, e.path), (Source::Env, PathBuf::from("/custom/adb")));
        assert_eq!(find_adb(Some("  "), None, None, None, Os::Linux), None, "a blank ADB_PATH is ignored");
        assert_eq!(adb_file_name(Os::Windows), "adb.exe");
    }

    #[test]
    fn extracts_platform_tools_and_ignores_the_rest() {
        let d = tmp("extract");
        let z = make_zip(&[("platform-tools/adb", b"#!/bin/sh\n"), ("platform-tools/AdbWinApi.dll", b"dll"), ("platform-tools/lib64/libc++.so", b"so"), ("other/readme.txt", b"skip")]);
        let adb = extract(&z, &d.join("platform-tools"), Os::Linux).unwrap();
        assert!(adb.is_file() && d.join("platform-tools/lib64/libc++.so").is_file());
        assert!(!d.join("platform-tools/readme.txt").exists() && !d.join("other").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&adb).unwrap().permissions().mode() & 0o111, 0o111, "adb must be runnable");
        }
    }

    #[test]
    fn extraction_replaces_old_files_only_when_the_new_set_is_valid() {
        let d = tmp("replace");
        let target = d.join("platform-tools");
        extract(&make_zip(&[("platform-tools/adb", b"old"), ("platform-tools/stale.txt", b"x")]), &target, Os::Linux).unwrap();
        // A bad download (no adb inside) must leave the working copy alone.
        assert!(extract(&make_zip(&[("platform-tools/notes.txt", b"nothing")]), &target, Os::Linux).is_err());
        assert_eq!(std::fs::read(target.join("adb")).unwrap(), b"old");
        assert!(!d.join("platform-tools.new").exists(), "staging is cleaned up");
        extract(&make_zip(&[("platform-tools/adb", b"new")]), &target, Os::Linux).unwrap();
        assert_eq!(std::fs::read(target.join("adb")).unwrap(), b"new");
        assert!(!target.join("stale.txt").exists(), "old files do not linger");
    }

    #[test]
    fn unsafe_zips_are_refused() {
        let d = tmp("unsafe");
        assert!(extract(b"not a zip", &d.join("pt"), Os::Linux).is_err());
        let z = make_zip(&[("platform-tools/adb", b"x"), ("platform-tools/../../evil.txt", b"bad")]);
        assert!(extract(&z, &d.join("pt"), Os::Linux).is_err());
        assert!(!d.join("evil.txt").exists() && !d.parent().unwrap().join("evil.txt").exists());
        let z = make_zip(&[("/abs/platform-tools/adb", b"x")]);
        assert!(extract(&z, &d.join("pt2"), Os::Linux).is_err());
    }
}
