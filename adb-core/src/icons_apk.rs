//! Icons read straight out of an app's APK. This is the fallback when the on-device helper is not
//! available: no adaptive-icon drawing, but the picture people recognise (the launcher PNG/WebP) is
//! almost always in the file. Vector and adaptive-icon XML cannot be drawn here, so the plain launcher
//! picture next to it is used instead.
use crate::labels::{temp_path, unzip_entry, valid_device_path, write_parts_apk};
use crate::{AdbError, AdbRunner};
use std::path::Path;

const MAX_ICON: usize = 400_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconData {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

pub fn mime_for(path: &str) -> Option<&'static str> {
    let p = path.to_ascii_lowercase();
    if p.ends_with(".png") {
        Some("image/png")
    } else if p.ends_with(".webp") {
        Some("image/webp")
    } else if p.ends_with(".jpg") || p.ends_with(".jpeg") {
        Some("image/jpeg")
    } else {
        None
    }
}

/// Whether the bytes really are that kind of picture (a name can lie, and the UI shows these as images).
pub fn looks_like(mime: &str, b: &[u8]) -> bool {
    match mime {
        "image/png" => b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        "image/webp" => b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP",
        "image/jpeg" => b.starts_with(&[0xFF, 0xD8, 0xFF]),
        _ => false,
    }
}

fn density_score(path: &str) -> i32 {
    let p = path.to_ascii_lowercase();
    for (tag, score) in [("xxxhdpi", 50), ("xxhdpi", 40), ("xhdpi", 30), ("hdpi", 20), ("mdpi", 10), ("ldpi", 5)] {
        if p.contains(tag) {
            return score;
        }
    }
    if p.contains("anydpi") { -100 } else { 8 }
}

/// Choose the best plain launcher picture from a list of files in the APK (highest density; not a
/// foreground/background/monochrome layer or a round variant when a plain one exists).
pub fn pick_launcher_icon(names: &[String]) -> Option<String> {
    let mut best: Option<(i32, &String)> = None;
    for n in names {
        let l = n.to_ascii_lowercase();
        if !l.starts_with("res/") || !(l.contains("/mipmap") || l.contains("/drawable")) || !l.contains("launcher") || mime_for(&l).is_none() {
            continue;
        }
        let mut score = density_score(&l);
        if l.contains("foreground") || l.contains("background") || l.contains("monochrome") {
            score -= 200;
        }
        if l.contains("round") {
            score -= 15;
        }
        if best.map_or(true, |(s, _)| score > s) {
            best = Some((score, n));
        }
    }
    best.filter(|(s, _)| *s > -100).map(|(_, n)| n.clone())
}

/// The manifest's icon reference resolved to a file name inside the APK.
pub fn icon_path_from_parts(manifest: &[u8], arsc: Option<&[u8]>) -> Option<String> {
    let tmp = write_parts_apk(manifest, arsc)?;
    let path = apk_info::Apk::new(&tmp).ok().and_then(|a| a.get_application_icon());
    let _ = std::fs::remove_file(&tmp);
    path.filter(|p| p.starts_with("res/") || p.contains('/'))
}

/// File names from toybox/Info-ZIP `unzip -l` output (the last column of each data line).
pub fn names_from_unzip_list(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim();
            let mut it = t.split_whitespace();
            let (size, _date, _time) = (it.next()?, it.next()?, it.next()?);
            if !size.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let mut rest = t;
            for _ in 0..3 {
                rest = rest.trim_start();
                rest = &rest[rest.find(char::is_whitespace)?..];
            }
            let name = rest.trim();
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

fn checked(path: &str, bytes: Vec<u8>) -> Result<IconData, String> {
    let mime = mime_for(path).ok_or_else(|| "the icon is a drawing, not a picture".to_string())?;
    if bytes.len() > MAX_ICON {
        return Err("the icon file is unexpectedly large".into());
    }
    if !looks_like(mime, &bytes) {
        return Err("the icon file is not a picture".into());
    }
    Ok(IconData { mime, bytes })
}

/// Read the icon from an APK file on this computer.
pub fn icon_from_apk_file(path: &Path) -> Result<IconData, String> {
    let apk = apk_info::Apk::new(path).map_err(|e| format!("could not open the app file: {e}"))?;
    let names: Vec<String> = apk.namelist().map(|s| s.to_string()).collect();
    let declared = apk.get_application_icon().filter(|p| mime_for(p).is_some() && names.contains(p));
    let chosen = declared.or_else(|| pick_launcher_icon(&names)).ok_or("no launcher picture in the app")?;
    let (bytes, _) = apk.read(&chosen).map_err(|e| format!("could not read {chosen}: {e}"))?;
    checked(&chosen, bytes)
}

/// Find the icon of an APK that lives on the phone: stream only what is needed with `unzip -p`, and pull
/// the whole file only if that is not possible.
pub async fn resolve(adb: &AdbRunner, serial: &str, apk_path: &str) -> Result<IconData, String> {
    if !valid_device_path(apk_path) {
        return Err("the app's file path has unusual characters".into());
    }
    let quick: Result<IconData, String> = async {
        let manifest = unzip_entry(adb, serial, apk_path, "AndroidManifest.xml").await.map_err(|e| e.to_string())?;
        if manifest.len() < 8 || manifest[0] != 0x03 || manifest[1] != 0x00 {
            return Err("unexpected data from the phone's unzip".to_string());
        }
        let arsc = unzip_entry(adb, serial, apk_path, "resources.arsc").await.ok().filter(|a| a.len() > 8);
        let declared = icon_path_from_parts(&manifest, arsc.as_deref()).filter(|p| mime_for(p).is_some());
        let entry = match declared {
            Some(p) => p,
            None => {
                let list = adb.run(&["-s", serial, "exec-out", "unzip", "-l", apk_path]).await.map_err(|e| e.to_string())?;
                pick_launcher_icon(&names_from_unzip_list(&list)).ok_or("no launcher picture in the app")?
            }
        };
        let bytes = unzip_entry(adb, serial, apk_path, &entry).await.map_err(|e: AdbError| e.to_string())?;
        checked(&entry, bytes)
    }
    .await;
    match quick {
        Ok(i) => Ok(i),
        Err(why) => {
            let tmp = temp_path("apk");
            let tmp_s = tmp.to_string_lossy().into_owned();
            let slow = adb.clone().with_timeout(std::time::Duration::from_secs(180));
            let out = match slow.run_full(&["-s", serial, "pull", apk_path, &tmp_s]).await {
                Ok(o) if o.code == Some(0) => icon_from_apk_file(&tmp).map_err(|e| format!("{why}; then {e}")),
                Ok(o) => Err(format!("{why}; copying the file failed: {}", o.stderr.trim())),
                Err(e) => Err(format!("{why}; copying the file failed: {e}")),
            };
            let _ = std::fs::remove_file(&tmp);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn picks_the_sharpest_plain_launcher_picture() {
        let n = names(&["res/mipmap-hdpi-v4/ic_launcher.png", "res/mipmap-xxhdpi-v4/ic_launcher.png", "res/mipmap-xxxhdpi-v4/ic_launcher_round.png", "res/mipmap-anydpi-v26/ic_launcher.xml", "res/drawable/other.png"]);
        assert_eq!(pick_launcher_icon(&n).as_deref(), Some("res/mipmap-xxhdpi-v4/ic_launcher.png"));
    }

    #[test]
    fn layers_are_not_icons_unless_nothing_else_exists() {
        let n = names(&["res/mipmap-xxhdpi/ic_launcher_foreground.png", "res/mipmap-mdpi/ic_launcher.png"]);
        assert_eq!(pick_launcher_icon(&n).as_deref(), Some("res/mipmap-mdpi/ic_launcher.png"));
        assert_eq!(pick_launcher_icon(&names(&["res/mipmap-xxhdpi/ic_launcher_foreground.png"])), None);
        assert_eq!(pick_launcher_icon(&names(&["assets/ic_launcher.png", "res/raw/launcher.png", "res/values/x.xml"])), None);
        assert_eq!(pick_launcher_icon(&[]), None);
    }

    #[test]
    fn mime_and_magic_must_agree() {
        assert_eq!(mime_for("res/a/B.PNG"), Some("image/png"));
        assert_eq!(mime_for("res/a/x.xml"), None);
        assert!(looks_like("image/png", &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]));
        assert!(!looks_like("image/png", b"<?xml"));
        assert!(looks_like("image/webp", b"RIFF\x00\x00\x00\x00WEBPVP8 "));
        assert!(checked("res/x.png", b"not a png at all".to_vec()).is_err());
        assert!(checked("res/x.xml", vec![1]).is_err());
    }

    #[test]
    fn parses_unzip_listings() {
        let t = "Archive:  base.apk\n  Length      Date    Time    Name\n---------  ---------- -----   ----\n      760  1980-01-01 00:00   AndroidManifest.xml\n       84  2024-01-01 12:30   res/mipmap-hdpi-v4/ic launcher.png\n---------                     -------\n     844                     2 files\n";
        assert_eq!(names_from_unzip_list(t), ["AndroidManifest.xml", "res/mipmap-hdpi-v4/ic launcher.png"]);
    }

    fn fixture(n: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(n)
    }

    #[test]
    fn reads_the_declared_png_from_a_real_apk() {
        let icon = icon_from_apk_file(&fixture("withicon.apk")).unwrap();
        assert_eq!(icon.mime, "image/png");
        assert!(looks_like("image/png", &icon.bytes));
        assert!(icon.bytes.len() > 70);
    }

    #[test]
    fn falls_back_to_the_launcher_png_when_the_icon_is_a_drawing() {
        let icon = icon_from_apk_file(&fixture("xmlicon.apk")).unwrap();
        assert_eq!(icon.mime, "image/png");
        assert_eq!(icon.bytes.len(), 80, "the xxhdpi one, not hdpi (77 bytes)");
    }

    #[test]
    fn an_app_without_an_icon_says_so() {
        assert!(icon_from_apk_file(&fixture("literal.apk")).unwrap_err().contains("no launcher picture"));
    }

    #[test]
    fn icon_path_comes_from_manifest_and_resources() {
        let z = std::fs::File::open(fixture("withicon.apk")).unwrap();
        let mut zip = zip::ZipArchive::new(z).unwrap();
        let read = |zip: &mut zip::ZipArchive<std::fs::File>, n: &str| {
            let mut b = Vec::new();
            std::io::Read::read_to_end(&mut zip.by_name(n).unwrap(), &mut b).unwrap();
            b
        };
        let (m, r) = (read(&mut zip, "AndroidManifest.xml"), read(&mut zip, "resources.arsc"));
        let p = icon_path_from_parts(&m, Some(&r)).unwrap();
        assert!(p.starts_with("res/mipmap-") && p.ends_with("ic_launcher.png"), "{p}");
    }
}
