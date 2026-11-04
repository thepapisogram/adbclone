//! Real app names: read the label out of an APK's manifest and resources.
use crate::{AdbError, AdbRunner};
use std::{
    collections::HashMap,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_path(ext: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("adbm-{}-{n}.{ext}", std::process::id()))
}

/// Android package id to a readable guess: `com.google.android.gm` -> "Gm".
pub fn fallback_label(pkg: &str) -> String {
    let last = pkg.rsplit('.').next().unwrap_or(pkg);
    let mut c = last.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => pkg.to_string(),
    }
}

fn usable(label: &str) -> Option<String> {
    let l = label.trim();
    (!l.is_empty() && !l.starts_with('@') && !l.starts_with("0x") && !l.starts_with("resourceId")).then(|| l.to_string())
}

/// Many system apps have no application-level name; Android then shows the launcher activity's name.
fn launcher_label(apk: &apk_info::Apk) -> Option<String> {
    for a in apk.get_activities() {
        let is_launcher = a.intent_filters.iter().any(|f| {
            f.actions.iter().any(|x| *x == "android.intent.action.MAIN") && f.categories.iter().any(|c| *c == "android.intent.category.LAUNCHER")
        });
        if !is_launcher {
            continue;
        }
        let Some(raw) = a.label else { continue };
        let value = if raw.starts_with('@') { apk.get_resource_value(raw) } else { Some(raw.to_string()) };
        if let Some(v) = value.as_deref().and_then(usable) {
            return Some(v);
        }
    }
    None
}

/// Label of a complete APK file on disk (`None` if it has no usable label).
pub fn label_from_apk_file(path: &Path) -> Option<String> {
    let apk = apk_info::Apk::new(path).ok()?;
    apk.get_application_label().as_deref().and_then(usable).or_else(|| launcher_label(&apk))
}

/// Rebuild a tiny stored ZIP holding just the manifest (and resources) so the full APK parser can read it.
/// The caller deletes the returned file.
pub(crate) fn write_parts_apk(manifest: &[u8], arsc: Option<&[u8]>) -> Option<PathBuf> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("AndroidManifest.xml", opts).ok()?;
        w.write_all(manifest).ok()?;
        if let Some(a) = arsc {
            w.start_file("resources.arsc", opts).ok()?;
            w.write_all(a).ok()?;
        }
        w.finish().ok()?;
    }
    let tmp = temp_path("apk");
    std::fs::write(&tmp, buf.into_inner()).ok()?;
    Some(tmp)
}

/// Label from just the two files that matter.
pub fn label_from_parts(manifest: &[u8], arsc: Option<&[u8]>) -> Option<String> {
    let tmp = write_parts_apk(manifest, arsc)?;
    let label = label_from_apk_file(&tmp);
    let _ = std::fs::remove_file(&tmp);
    label
}

/// Characters allowed in a device path that we interpolate into a shell command.
pub fn valid_device_path(p: &str) -> bool {
    p.starts_with('/') && !p.contains("..") && p.chars().all(|c| c.is_ascii_alphanumeric() || "/._~=+-".contains(c))
}

pub(crate) async fn unzip_entry(adb: &AdbRunner, serial: &str, apk: &str, entry: &str) -> Result<Vec<u8>, AdbError> {
    adb.run_bytes(&["-s", serial, "exec-out", "unzip", "-p", apk, entry]).await
}

/// The outcome of looking for one app's name, with the reason when there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub label: Option<String>,
    /// Why no name was found (`None` when one was).
    pub reason: Option<String>,
}

impl Resolved {
    fn found(label: String) -> Self {
        Resolved { label: Some(label), reason: None }
    }
    fn missing(reason: impl Into<String>) -> Self {
        Resolved { label: None, reason: Some(reason.into()) }
    }
}

/// Find the name of an APK that lives on the phone.
/// Fast path: stream only the manifest and resources with `unzip -p`. If that cannot decide (no `unzip`,
/// or the resources could not be read), pull the whole APK with a generous timeout.
pub async fn resolve(adb: &AdbRunner, serial: &str, apk_path: &str) -> Resolved {
    if !valid_device_path(apk_path) {
        return Resolved::missing("the app's file path has unusual characters");
    }
    let why_quick_failed: String = match unzip_entry(adb, serial, apk_path, "AndroidManifest.xml").await {
        // A real binary manifest starts with the XML chunk header 0x0003.
        Ok(manifest) if manifest.len() > 8 && manifest[0] == 0x03 && manifest[1] == 0x00 => {
            let arsc = unzip_entry(adb, serial, apk_path, "resources.arsc").await.ok().filter(|a| a.len() > 8);
            if let Some(l) = label_from_parts(&manifest, arsc.as_deref()) {
                return Resolved::found(l);
            }
            if arsc.is_some() {
                // Both files were read fine and there is simply no name in them.
                return Resolved::missing("the app declares no name");
            }
            "could not read the app's resources".to_string()
        }
        Ok(_) => "the phone's unzip returned unexpected data".to_string(),
        Err(e) => format!("quick read failed ({e})"),
    };
    // Whole-file fallback. Big system APKs can take a while, especially over Wi-Fi.
    let tmp = temp_path("apk");
    let tmp_s = tmp.to_string_lossy().into_owned();
    let slow = adb.clone().with_timeout(std::time::Duration::from_secs(180));
    let pulled = slow.run_full(&["-s", serial, "pull", apk_path, &tmp_s]).await;
    let result = match pulled {
        Ok(o) if o.code == Some(0) => match label_from_apk_file(&tmp) {
            Some(l) => Resolved::found(l),
            None => Resolved::missing("the app declares no name"),
        },
        Ok(o) => Resolved::missing(format!("{why_quick_failed}; copying the file failed: {}", o.stderr.trim())),
        Err(e) => Resolved::missing(format!("{why_quick_failed}; copying the file failed: {e}")),
    };
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Backwards-compatible wrapper: just the label.
pub async fn fetch_label(adb: &AdbRunner, serial: &str, apk_path: &str) -> Result<Option<String>, AdbError> {
    if !valid_device_path(apk_path) {
        return Err(AdbError::Device(format!("unsafe path: {apk_path}")));
    }
    Ok(resolve(adb, serial, apk_path).await.label)
}

/// Size and mtime for each path, via one `stat` call: used as a cache key.
pub async fn stat_files(adb: &AdbRunner, serial: &str, paths: &[&str]) -> Result<HashMap<String, (u64, u64)>, AdbError> {
    let safe: Vec<&str> = paths.iter().copied().filter(|p| valid_device_path(p)).collect();
    if safe.is_empty() {
        return Ok(HashMap::new());
    }
    let cmd = format!("stat -c '%s %Y %n' {} 2>/dev/null", safe.join(" "));
    Ok(parse_stat(&adb.shell(serial, &[&cmd]).await?))
}

pub fn parse_stat(out: &str) -> HashMap<String, (u64, u64)> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, ' ');
            let size = it.next()?.parse().ok()?;
            let mtime = it.next()?.parse().ok()?;
            Some((it.next()?.trim().to_string(), (size, mtime)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    fn entry(apk: &Path, name: &str) -> Option<Vec<u8>> {
        let mut z = zip::ZipArchive::new(std::fs::File::open(apk).ok()?).ok()?;
        let mut f = z.by_name(name).ok()?;
        let mut v = Vec::new();
        f.read_to_end(&mut v).ok()?;
        Some(v)
    }

    #[test]
    fn literal_label_from_file() {
        assert_eq!(label_from_apk_file(&fixture("literal.apk")).as_deref(), Some("Literal App"));
    }

    #[test]
    fn resource_reference_resolves_to_default_locale() {
        assert_eq!(label_from_apk_file(&fixture("reference.apk")).as_deref(), Some("Referenced App"));
        assert_eq!(label_from_apk_file(&fixture("entities.apk")).as_deref(), Some("Fish & Chips"));
    }

    #[test]
    fn launcher_activity_name_is_used_when_the_app_has_none() {
        assert_eq!(label_from_apk_file(&fixture("activityref.apk")).as_deref(), Some("Launcher Label"), "the launcher one, not the first activity");
        assert_eq!(label_from_apk_file(&fixture("activityliteral.apk")).as_deref(), Some("Literal Launcher"));
    }

    #[test]
    fn missing_label_gives_none() {
        assert_eq!(label_from_apk_file(&fixture("nolabel.apk")), None);
        assert_eq!(label_from_apk_file(&fixture("does-not-exist.apk")), None);
    }

    #[test]
    fn parts_path_matches_full_apk() {
        for (f, want) in [("literal.apk", "Literal App"), ("reference.apk", "Referenced App"), ("entities.apk", "Fish & Chips")] {
            let m = entry(&fixture(f), "AndroidManifest.xml").unwrap();
            let a = entry(&fixture(f), "resources.arsc");
            assert_eq!(label_from_parts(&m, a.as_deref()).as_deref(), Some(want), "{f}");
        }
    }

    #[test]
    fn garbage_is_not_a_label() {
        assert_eq!(label_from_parts(b"not an apk at all", None), None);
        assert_eq!(label_from_parts(&[], Some(&[])), None);
    }

    #[test]
    fn fallback_and_path_safety() {
        assert_eq!(fallback_label("com.google.android.gm"), "Gm");
        assert!(valid_device_path("/data/app/~~abc==/com.x-1==/base.apk"));
        assert!(!valid_device_path("/data/app/x; rm -rf /"));
        assert!(!valid_device_path("/data/../etc/passwd"));
        assert!(!valid_device_path("relative/path.apk"));
    }

    #[test]
    fn stat_output_parses() {
        let m = parse_stat("1234 1700000000 /system/app/Foo/Foo.apk\n99 5 /data/app/x/base.apk\nbogus\n");
        assert_eq!(m["/system/app/Foo/Foo.apk"], (1234, 1700000000));
        assert_eq!(m.len(), 2);
    }
}
