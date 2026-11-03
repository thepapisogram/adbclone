//! The temporary on-device helper: pushed to /data/local/tmp, run once with `app_process` (as the shell
//! user, like scrcpy's server), asked for app names/details/icons, then deleted. Nothing is installed.
use crate::packages::valid_name;
use crate::{AdbError, AdbRunner};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

/// Prebuilt by `helper/build.sh` from `helper/src`. Rebuild it to check it matches the source.
pub const HELPER_JAR: &[u8] = include_bytes!("../../helper/dist/helper.jar");
const MAIN_CLASS: &str = "dev.adbclone.helper.Main";

/// Stable short hash (FNV-1a) so a new helper version gets a new file name on the phone.
pub fn fnv_hex(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub fn remote_path() -> String {
    format!("/data/local/tmp/adbclone-helper-{}.jar", &fnv_hex(HELPER_JAR)[..12])
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeApp {
    pub package: String,
    /// The name the phone itself shows (already localized). Empty if it could not be read.
    pub label: String,
    pub system: bool,
    pub updated_system: bool,
    pub enabled: bool,
    /// False for apps removed for the current user (still in the ROM).
    pub installed: bool,
    pub version_name: String,
    pub version_code: i64,
    pub installer: String,
    pub first_install: i64,
    pub last_update: i64,
    /// Has a launcher icon, i.e. something people open.
    pub launchable: bool,
    pub target_sdk: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    App(BridgeApp),
    Icon { package: String, png_base64: String },
    Selftest(Value),
    Error(String),
    Done,
}

/// Parse one output line. Anything that does not start with the protocol marker is ignored (noise from
/// the shell, warnings from the runtime).
pub fn parse_line(raw: &str) -> Option<Line> {
    let payload = raw.trim_end_matches(['\r', '\n']).strip_prefix("ACB1 ")?;
    if payload == "DONE" {
        return Some(Line::Done);
    }
    let v: Value = serde_json::from_str(payload).ok()?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let n = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
    let b = |k: &str| n(k) != 0;
    if let Some(e) = v.get("error").and_then(Value::as_str) {
        return Some(Line::Error(e.to_string()));
    }
    if v.get("png").is_some() {
        let package = s("p");
        return valid_name(&package).then(|| Line::Icon { package, png_base64: s("png") });
    }
    if v.get("sdk").is_some() && v.get("problems").is_some() {
        return Some(Line::Selftest(v));
    }
    let package = s("p");
    if !valid_name(&package) || v.get("n").is_none() {
        return None;
    }
    Some(Line::App(BridgeApp {
        label: s("n"),
        system: b("s"),
        updated_system: b("u"),
        enabled: b("e"),
        installed: v.get("in").map_or(true, |_| b("in")),
        version_name: s("v"),
        version_code: n("vc"),
        installer: s("i"),
        first_install: n("fi"),
        last_update: n("lu"),
        launchable: b("l"),
        target_sdk: n("t"),
        package,
    }))
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub lines: Vec<Line>,
    /// The helper printed its final DONE line, so the list is complete.
    pub complete: bool,
}

pub fn parse_output(text: &str) -> Parsed {
    let mut p = Parsed::default();
    for l in text.lines() {
        match parse_line(l) {
            Some(Line::Done) => p.complete = true,
            Some(other) => p.lines.push(other),
            None => {}
        }
    }
    p
}

/// What a helper run gave back, plus stderr for diagnosis.
#[derive(Debug)]
pub struct RunResult {
    pub parsed: Parsed,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

impl RunResult {
    pub fn errors(&self) -> Vec<String> {
        self.parsed.lines.iter().filter_map(|l| if let Line::Error(e) = l { Some(e.clone()) } else { None }).collect()
    }

    /// One line for the user when the run did not work.
    pub fn failure_reason(&self) -> String {
        if let Some(e) = self.errors().first() {
            return e.clone();
        }
        let err = self.stderr.trim();
        if !err.is_empty() {
            return err.lines().take(3).collect::<Vec<_>>().join(" | ");
        }
        match self.exit_code {
            Some(0) | None => "the helper printed nothing".to_string(),
            Some(c) => format!("the helper exited with code {c}"),
        }
    }
}

/// Copy the helper to the phone. Returns the path on the phone.
pub async fn push(adb: &AdbRunner, serial: &str) -> Result<String, AdbError> {
    let remote = remote_path();
    let local = std::env::temp_dir().join(format!("adbclone-helper-{}-{}.jar", std::process::id(), fnv_hex(HELPER_JAR)));
    std::fs::write(&local, HELPER_JAR)?;
    let local_s = local.to_string_lossy().into_owned();
    let out = adb.clone().with_timeout(Duration::from_secs(60)).run_full(&["-s", serial, "push", &local_s, &remote]).await;
    let _ = std::fs::remove_file(&local);
    let out = out?;
    if out.code != Some(0) {
        return Err(AdbError::Device(format!("could not copy the helper to the phone: {}", out.stderr.trim())));
    }
    Ok(remote)
}

/// Run the helper in one mode. `args` are package names (validated).
pub async fn run(adb: &AdbRunner, serial: &str, remote: &str, mode: &str, args: &[String], timeout: Duration) -> Result<RunResult, AdbError> {
    if !["selftest", "apps", "icons"].contains(&mode) {
        return Err(AdbError::Device("unknown helper mode".into()));
    }
    if args.iter().any(|a| !valid_name(a)) {
        return Err(AdbError::Device("invalid package name".into()));
    }
    let mut cmd = format!("CLASSPATH={remote} app_process /system/bin {MAIN_CLASS} {mode}");
    for a in args {
        cmd.push(' ');
        cmd.push_str(a);
    }
    // exec-out gives a clean byte stream (no terminal newline translation).
    let out = adb.clone().with_timeout(timeout).run_full(&["-s", serial, "exec-out", &cmd]).await?;
    Ok(RunResult { parsed: parse_output(&String::from_utf8_lossy(&out.stdout)), stderr: out.stderr, exit_code: out.code })
}

/// Delete the helper from the phone.
pub async fn cleanup(adb: &AdbRunner, serial: &str, remote: &str) {
    if remote.starts_with("/data/local/tmp/adbclone-helper-") && remote.ends_with(".jar") && !remote.contains(' ') {
        let _ = adb.shell(serial, &[&format!("rm -f {remote}")]).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn embedded_jar_is_a_real_dex_jar() {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(HELPER_JAR)).expect("helper.jar is a zip");
        let mut dex = Vec::new();
        z.by_name("classes.dex").expect("contains classes.dex").read_to_end(&mut dex).unwrap();
        assert!(dex.starts_with(b"dex\n0"), "dex magic");
        assert!(dex.windows(MAIN_CLASS.len()).any(|w| w == MAIN_CLASS.replace('.', "/").as_bytes() || w == MAIN_CLASS.as_bytes()) || String::from_utf8_lossy(&dex).contains("dev/adbclone/helper/Main"));
        assert!(remote_path().starts_with("/data/local/tmp/adbclone-helper-") && remote_path().ends_with(".jar"));
    }

    #[test]
    fn parses_an_app_line() {
        let l = r#"ACB1 {"p":"com.google.android.gm","n":"Gmail","s":1,"u":1,"e":1,"in":1,"v":"2024.05","vc":123456,"fi":1700000000000,"lu":1710000000000,"t":34,"i":"com.android.vending","l":1}"#;
        let Some(Line::App(a)) = parse_line(l) else { panic!("not an app") };
        assert_eq!((a.package.as_str(), a.label.as_str()), ("com.google.android.gm", "Gmail"));
        assert!(a.system && a.updated_system && a.enabled && a.installed && a.launchable);
        assert_eq!((a.version_code, a.target_sdk, a.installer.as_str()), (123456, 34, "com.android.vending"));
    }

    #[test]
    fn removed_and_disabled_apps_are_flagged() {
        let l = r#"ACB1 {"p":"com.vendor.x","n":"X","s":1,"u":0,"e":0,"in":0,"v":"","vc":1,"fi":0,"lu":0,"t":30,"i":"","l":0}"#;
        let Some(Line::App(a)) = parse_line(l) else { panic!() };
        assert!(!a.enabled && !a.installed && !a.launchable);
    }

    #[test]
    fn parses_other_line_kinds() {
        assert_eq!(parse_line("ACB1 DONE\r\n"), Some(Line::Done));
        assert_eq!(parse_line(r#"ACB1 {"error":"boom"}"#), Some(Line::Error("boom".into())));
        assert!(matches!(parse_line(r#"ACB1 {"p":"a.b","png":"iVBOR"}"#), Some(Line::Icon { .. })));
        assert!(matches!(parse_line(r#"ACB1 {"sdk":34,"release":"14","problems":[]}"#), Some(Line::Selftest(_))));
    }

    #[test]
    fn ignores_noise_and_garbage() {
        for bad in ["", "WARNING: linker: something", "ACB1 not json", "ACB1 {\"p\":\"bad name!\",\"n\":\"x\"}", "ACB1 {\"n\":\"no package\"}", "ACB1 {\"p\":\"a.b\",\"png\":\"x\"} trailing? no"] {
            let r = parse_line(bad);
            assert!(!matches!(r, Some(Line::App(_))), "{bad}");
        }
        assert_eq!(parse_line("ACB1 {\"p\":\"bad name!\",\"png\":\"x\"}"), None);
    }

    #[test]
    fn output_tracks_completeness() {
        let full = "noise\nACB1 {\"p\":\"a.b\",\"n\":\"A\"}\nACB1 DONE\n";
        let cut = "ACB1 {\"p\":\"a.b\",\"n\":\"A\"}\nACB1 {\"p\":\"c.d\",\"n\"";
        assert!(parse_output(full).complete && parse_output(full).lines.len() == 1);
        assert!(!parse_output(cut).complete && parse_output(cut).lines.len() == 1, "a cut-off run keeps what it got but is not complete");
    }

    #[test]
    fn failure_reasons_are_readable() {
        let r = |lines: Vec<Line>, stderr: &str, code| RunResult { parsed: Parsed { lines, complete: false }, stderr: stderr.into(), exit_code: code };
        assert_eq!(r(vec![Line::Error("java.lang.SecurityException: x".into())], "", Some(0)).failure_reason(), "java.lang.SecurityException: x");
        assert!(r(vec![], "app_process: not found\nsecond", Some(127)).failure_reason().contains("not found"));
        assert_eq!(r(vec![], "", Some(0)).failure_reason(), "the helper printed nothing");
        assert_eq!(r(vec![], "", Some(2)).failure_reason(), "the helper exited with code 2");
    }

    #[test]
    fn hash_changes_with_content() {
        assert_ne!(fnv_hex(b"a"), fnv_hex(b"b"));
        assert_eq!(fnv_hex(b"abc").len(), 16);
    }
}
