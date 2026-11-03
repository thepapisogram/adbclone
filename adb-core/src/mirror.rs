//! Screenshots (adb's own `screencap`) and screen mirroring by starting scrcpy, which this app does not
//! bundle or build: it is a separate, well-known open-source tool the user installs.
use crate::{AdbError, AdbRunner};
use std::path::{Path, PathBuf};

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const MAX_PNG: usize = 60_000_000;

/// Take a screenshot of what is on the phone's screen right now and return the PNG bytes.
pub async fn screenshot(adb: &AdbRunner, serial: &str) -> Result<Vec<u8>, AdbError> {
    let bytes = adb.clone().with_timeout(std::time::Duration::from_secs(30)).run_bytes(&["-s", serial, "exec-out", "screencap", "-p"]).await?;
    check_png(&bytes).map_err(AdbError::Device)?;
    Ok(bytes)
}

/// The phone sometimes answers with an error message instead of an image (for example when a
/// protected screen is showing); never save that as a ".png".
pub fn check_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_PNG {
        return Err("The screenshot is unexpectedly large, so it was not saved.".into());
    }
    if bytes.len() < 100 || bytes[..8] != PNG_MAGIC {
        let text: String = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).trim().chars().filter(|c| !c.is_control()).collect();
        return Err(if text.is_empty() { "The phone did not return a picture.".into() } else { format!("The phone did not return a picture: {text}") });
    }
    Ok(())
}

pub fn scrcpy_file_name(windows: bool) -> &'static str {
    if windows { "scrcpy.exe" } else { "scrcpy" }
}

/// Where scrcpy might be: next to this app, in the app's data folder (also one folder level down, which
/// is how its release zip unpacks), then on PATH.
pub fn find_scrcpy(app_data: Option<&Path>, exe_dir: Option<&Path>, path_var: Option<&std::ffi::OsStr>, windows: bool) -> Option<PathBuf> {
    let name = scrcpy_file_name(windows);
    let mut cands: Vec<PathBuf> = Vec::new();
    for base in [exe_dir, app_data].into_iter().flatten() {
        cands.push(base.join(name));
        cands.push(base.join("scrcpy").join(name));
        if let Ok(rd) = std::fs::read_dir(base) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with("scrcpy-") {
                    cands.push(e.path().join(name));
                }
            }
        }
    }
    if let Some(p) = path_var {
        cands.extend(std::env::split_paths(p).map(|d| d.join(name)));
    }
    cands.into_iter().find(|p| p.is_file())
}

/// Device serials are letters, digits and `. : - _` (USB serials and `ip:port`); anything else is refused.
pub fn valid_serial(s: &str) -> bool {
    !s.is_empty() && s.len() <= 80 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_'))
}

/// Start scrcpy for one phone. It opens its own window and keeps running on its own.
pub fn start_scrcpy(scrcpy: &Path, adb: &Path, serial: &str) -> Result<(), AdbError> {
    if !valid_serial(serial) {
        return Err(AdbError::Device("That device name is not valid.".into()));
    }
    crate::runner::spawn_detached(scrcpy, &["-s".to_string(), serial.to_string()], &[("ADB", adb.as_os_str())])
        .map_err(|e| AdbError::Device(format!("Could not start scrcpy: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut v = PNG_MAGIC.to_vec();
        v.extend(std::iter::repeat(0u8).take(200));
        v
    }

    #[test]
    fn only_real_pngs_pass() {
        assert!(check_png(&png()).is_ok());
        assert!(check_png(b"").unwrap_err().contains("did not return a picture"));
        assert!(check_png(b"Error: secure window\n").unwrap_err().contains("secure window"));
        let mut huge = png();
        huge.resize(MAX_PNG + 1, 0);
        assert!(check_png(&huge).is_err());
    }

    #[test]
    fn serials() {
        for ok in ["R58M123ABC", "192.168.1.5:5555", "emulator-5554", "adb-XYZ_1._adb-tls-connect._tcp"] {
            assert!(valid_serial(ok), "{ok}");
        }
        for bad in ["", "a b", "a;b", "-s x", "a\nb", "$(x)", &"a".repeat(81)] {
            assert!(!valid_serial(bad), "{bad}");
        }
    }

    #[test]
    fn finds_scrcpy_in_the_expected_places() {
        let root = std::env::temp_dir().join(format!("coolant-scrcpy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let unpacked = root.join("data").join("scrcpy-win64-v2.4");
        std::fs::create_dir_all(&unpacked).unwrap();
        std::fs::create_dir_all(root.join("exe")).unwrap();
        assert_eq!(find_scrcpy(Some(&root.join("data")), Some(&root.join("exe")), None, true), None);
        std::fs::write(unpacked.join("scrcpy.exe"), b"x").unwrap();
        assert_eq!(find_scrcpy(Some(&root.join("data")), Some(&root.join("exe")), None, true), Some(unpacked.join("scrcpy.exe")));
        // PATH is the last resort.
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("scrcpy"), b"x").unwrap();
        let path = std::env::join_paths([&bin]).unwrap();
        assert_eq!(find_scrcpy(None, None, Some(&path), false), Some(bin.join("scrcpy")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_a_bad_serial_before_starting_anything() {
        assert!(start_scrcpy(Path::new("/nonexistent/scrcpy"), Path::new("adb"), "a;b").is_err());
    }
}
