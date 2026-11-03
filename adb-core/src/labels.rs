//! Real app names: read the label out of an APK's manifest and resources.
use crate::{AdbError, AdbRunner};
use std::{
    collections::HashMap,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(ext: &str) -> PathBuf {
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

/// Label of a complete APK file on disk (`None` if it has no usable label).
pub fn label_from_apk_file(path: &Path) -> Option<String> {
    let apk = apk_info::Apk::new(path).ok()?;
    let label = apk.get_application_label()?;
    let label = label.trim();
    (!label.is_empty() && !label.starts_with("@") && !label.starts_with("0x")).then(|| label.to_string())
}

/// Label from just the two files that matter. Rebuilds a tiny stored ZIP so the same parser can be used.
pub fn label_from_parts(manifest: &[u8], arsc: Option<&[u8]>) -> Option<String> {
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
    let label = label_from_apk_file(&tmp);
    let _ = std::fs::remove_file(&tmp);
    label
}

/// Characters allowed in a device path that we interpolate into a shell command.
pub fn valid_device_path(p: &str) -> bool {
    p.starts_with('/') && !p.contains("..") && p.chars().all(|c| c.is_ascii_alphanumeric() || "/._~=+-".contains(c))
}

async fn unzip_entry(adb: &AdbRunner, serial: &str, apk: &str, entry: &str) -> Result<Vec<u8>, AdbError> {
    adb.run_bytes(&["-s", serial, "exec-out", "unzip", "-p", apk, entry]).await
}

/// Fetch the label of an APK that lives on the device.
/// Fast path: stream only the manifest and resources with `unzip -p`. Fallback: `adb pull` the whole APK.
pub async fn fetch_label(adb: &AdbRunner, serial: &str, apk_path: &str) -> Result<Option<String>, AdbError> {
    if !valid_device_path(apk_path) {
        return Err(AdbError::Device(format!("unsafe path: {apk_path}")));
    }
    if let Ok(manifest) = unzip_entry(adb, serial, apk_path, "AndroidManifest.xml").await {
        // A real binary manifest starts with the XML chunk header 0x0003.
        if manifest.len() > 8 && manifest[0] == 0x03 && manifest[1] == 0x00 {
            let arsc = unzip_entry(adb, serial, apk_path, "resources.arsc").await.ok().filter(|a| a.len() > 8);
            return Ok(label_from_parts(&manifest, arsc.as_deref()));
        }
    }
    let tmp = temp_path("apk");
    let tmp_s = tmp.to_string_lossy().into_owned();
    let pulled = adb.run(&["-s", serial, "pull", apk_path, &tmp_s]).await;
    let label = pulled.ok().and_then(|_| label_from_apk_file(&tmp));
    let _ = std::fs::remove_file(&tmp);
    Ok(label)
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
