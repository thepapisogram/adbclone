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

/// Base name on the phone for this helper version.
pub fn remote_path() -> String {
    format!("/data/local/tmp/adbclone-helper-{}.jar", &fnv_hex(HELPER_JAR)[..12])
}

/// A name no other push can share: two things (say, loading names and saving a report) may use the helper
/// at the same moment, and one must not delete or overwrite the other's copy.
fn unique_remote_path() -> String {
    format!("/data/local/tmp/adbclone-helper-{}-{}.jar", &fnv_hex(HELPER_JAR)[..12], crate::wireless::random_token(8))
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
    Details(AppExtra),
    Error(String),
    Done,
}

/// Things only the phone knows about one app: what it asks permission for, and its own description.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppExtra {
    pub package: String,
    /// (permission name, currently granted)
    pub permissions: Vec<(String, bool)>,
    /// The `android:description` the developer wrote (rare; empty when there is none).
    pub description: String,
    /// `ApplicationInfo.category` (Android 8+), -1 when unknown.
    pub category: i64,
    pub source_dir: String,
}

/// Permission names are dotted identifiers; anything else is dropped rather than shown.
pub fn permission_ok(n: &str) -> bool {
    !n.is_empty() && n.len() <= 200 && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '$'))
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
    if let Some(perms) = v.get("perms").and_then(Value::as_array) {
        let package = s("p");
        if !valid_name(&package) {
            return None;
        }
        let permissions = perms
            .iter()
            .filter_map(|p| Some((p.get("n")?.as_str()?.to_string(), p.get("g").and_then(Value::as_i64).unwrap_or(0) != 0)))
            .filter(|(n, _)| permission_ok(n))
            .collect();
        return Some(Line::Details(AppExtra { package, permissions, description: s("d").chars().take(500).collect(), category: v.get("c").and_then(Value::as_i64).unwrap_or(-1), source_dir: s("src") }));
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
    let remote = unique_remote_path();
    let local = std::env::temp_dir().join(format!("adbclone-helper-{}-{}.jar", std::process::id(), crate::wireless::random_token(8)));
    std::fs::write(&local, HELPER_JAR)?;
    let local_s = local.to_string_lossy().into_owned();
    let out = adb.clone().with_timeout(Duration::from_secs(60)).run_full(&["-s", serial, "push", &local_s, &remote]).await;
    let _ = std::fs::remove_file(&local);
    let out = out?;
    if out.code != Some(0) {
        return Err(AdbError::Device(format!("could not copy the helper to the phone: {}", out.stderr.trim())));
    }
    // Newer Android versions refuse to load code from a file that anyone can still write to.
    let _ = adb.shell(serial, &[&format!("chmod 444 {remote}")]).await;
    Ok(remote)
}

/// Run the helper in one mode. `args` are package names (validated).
pub async fn run(adb: &AdbRunner, serial: &str, remote: &str, mode: &str, args: &[String], timeout: Duration) -> Result<RunResult, AdbError> {
    if !["selftest", "apps", "icons", "details"].contains(&mode) {
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

/// Permissions from `dumpsys package <pkg>`: the "requested permissions" list, with granted state taken
/// from any `name: granted=true|false` line (install and runtime permissions).
pub fn parse_dumpsys_permissions(text: &str) -> Vec<(String, bool)> {
    let mut requested: Vec<String> = Vec::new();
    let mut granted: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut in_requested = false;
    let mut base_indent = 0;
    for l in text.lines() {
        let indent = l.len() - l.trim_start().len();
        let t = l.trim();
        if t == "requested permissions:" {
            in_requested = true;
            base_indent = indent;
            continue;
        }
        if in_requested {
            if indent > base_indent && permission_ok(t.trim_end_matches(':')) && !t.contains(' ') {
                requested.push(t.trim_end_matches(':').to_string());
                continue;
            }
            in_requested = false;
        }
        if let Some((name, rest)) = t.split_once(": granted=") {
            if permission_ok(name) {
                granted.insert(name.to_string(), rest.starts_with("true"));
            }
        }
    }
    requested.dedup();
    requested.into_iter().map(|n| { let g = granted.get(&n).copied().unwrap_or(false); (n, g) }).collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckStep {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    pub steps: Vec<CheckStep>,
    pub verdict: String,
    pub works: bool,
}

/// A plain-words hint for the most common reasons the helper cannot run.
pub fn hint_for(detail: &str) -> &'static str {
    let d = detail.to_ascii_lowercase();
    if d.contains("unauthorized") {
        "Accept the USB debugging prompt on the phone, then try again."
    } else if d.contains("device offline") || d.contains("no devices") || d.contains("not found") && d.contains("device") {
        "The phone is not connected right now. Reconnect it and try again."
    } else if d.contains("permission denied") || d.contains("securityexception") || d.contains("not allowed") {
        "The phone's security settings blocked it. On Xiaomi/Redmi/POCO turn on \"USB debugging (Security settings)\"; on Oppo/Realme/Vivo allow installing/debugging over USB in Developer options."
    } else if d.contains("inaccessible or not found") || d.contains("app_process") {
        "This phone could not start the helper program (app_process). Names will come from reading app files instead."
    } else if d.contains("no space") {
        "The phone has no free space for the 5 KB helper file."
    } else if d.contains("timed out") || d.contains("timeout") {
        "The phone took too long to answer. Try again, and keep the screen on."
    } else {
        "Send this result with the device report so it can be fixed for your phone."
    }
}

pub fn summarize(steps: &[CheckStep]) -> (String, bool) {
    match steps.iter().find(|s| !s.ok) {
        None => ("Works: Coolant can read names, icons and details straight from this phone.".into(), true),
        Some(f) => (format!("Not working at the step \"{}\": {} {}", f.name, f.detail, hint_for(&f.detail)), false),
    }
}

/// Try every part of the helper path on this phone and say, step by step, what worked. Changes nothing
/// on the phone except a tiny temporary file that is deleted at the end.
pub async fn check(adb: &AdbRunner, serial: &str) -> CheckReport {
    let mut steps: Vec<CheckStep> = Vec::new();
    let step = |name: &str, ok: bool, detail: String| CheckStep { name: name.into(), ok, detail };
    let remote = match push(adb, serial).await {
        Ok(r) => {
            steps.push(step("Copy the helper to the phone", true, "copied".into()));
            r
        }
        Err(e) => {
            steps.push(step("Copy the helper to the phone", false, e.to_string()));
            let (verdict, works) = summarize(&steps);
            return CheckReport { steps, verdict, works };
        }
    };
    // Start it.
    match run(adb, serial, &remote, "selftest", &[], Duration::from_secs(40)).await {
        Err(e) => steps.push(step("Start the helper", false, e.to_string())),
        Ok(res) => match res.parsed.lines.iter().find_map(|l| if let Line::Selftest(v) = l { Some(v.clone()) } else { None }) {
            None => steps.push(step("Start the helper", false, res.failure_reason())),
            Some(v) => {
                let problems: Vec<String> = v.get("problems").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                let g = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(-1);
                let detail = format!(
                    "Android {} (API {}), {} apps visible{}{}",
                    v.get("release").and_then(Value::as_str).unwrap_or("?"),
                    g("sdk"),
                    g("packages").max(0),
                    if v.get("iconEncoded").and_then(Value::as_bool) == Some(true) { ", icon drawing works" } else { "" },
                    if problems.is_empty() { String::new() } else { format!("; problems: {}", problems.join(" | ")) }
                );
                let ok = v.get("context").and_then(Value::as_bool) == Some(true) && g("packages") > 0 && problems.is_empty();
                steps.push(step("Start the helper", ok, detail));
            }
        },
    }
    // Names.
    let mut first_pkgs: Vec<String> = Vec::new();
    if steps.iter().all(|s| s.ok) {
        match run(adb, serial, &remote, "apps", &[], Duration::from_secs(90)).await {
            Err(e) => steps.push(step("Read app names", false, e.to_string())),
            Ok(res) => {
                let apps: Vec<&BridgeApp> = res.parsed.lines.iter().filter_map(|l| if let Line::App(a) = l { Some(a) } else { None }).collect();
                let named = apps.iter().filter(|a| !a.label.trim().is_empty() && a.label != a.package).count();
                first_pkgs = apps.iter().filter(|a| !a.system && a.launchable).map(|a| a.package.clone()).chain(apps.iter().filter(|a| a.launchable).map(|a| a.package.clone())).take(3).collect();
                let errs = res.errors().len();
                let ok = !apps.is_empty() && named * 10 >= apps.len() * 9;
                let detail = if apps.is_empty() { res.failure_reason() } else { format!("{} apps, {} with a real name{}{}", apps.len(), named, if errs > 0 { format!(", {errs} could not be read") } else { String::new() }, if res.parsed.complete { "" } else { "; the list was cut short" }) };
                steps.push(step("Read app names", ok, detail));
            }
        }
    }
    // Icons.
    if steps.iter().all(|s| s.ok) && !first_pkgs.is_empty() {
        match run(adb, serial, &remote, "icons", &first_pkgs, Duration::from_secs(60)).await {
            Err(e) => steps.push(step("Read icons", false, e.to_string())),
            Ok(res) => {
                let got = res.parsed.lines.iter().filter(|l| matches!(l, Line::Icon { .. })).count();
                let detail = if got > 0 { format!("{} of {} sample icons read", got, first_pkgs.len()) } else { format!("no icons came back: {}", res.errors().first().cloned().unwrap_or_else(|| res.failure_reason())) };
                steps.push(step("Read icons", got > 0, detail));
            }
        }
    }
    cleanup(adb, serial, &remote).await;
    let (verdict, works) = summarize(&steps);
    CheckReport { steps, verdict, works }
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
    fn every_push_gets_its_own_files() {
        let (a, b) = (unique_remote_path(), unique_remote_path());
        assert_ne!(a, b);
        for p in [&a, &b] {
            assert!(p.starts_with("/data/local/tmp/adbclone-helper-") && p.ends_with(".jar") && !p.contains(' '), "{p}");
        }
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
    fn details_lines_carry_permissions_and_the_apps_own_description() {
        let l = r#"ACB1 {"p":"com.chat","perms":[{"n":"android.permission.CAMERA","g":1},{"n":"android.permission.INTERNET","g":1},{"n":"bad name!","g":1},{"n":"com.chat.permission.C2D_MESSAGE","g":0}],"d":"Chat with friends","c":4,"src":"/data/app/x/base.apk"}"#;
        let Some(Line::Details(d)) = parse_line(l) else { panic!("not details") };
        assert_eq!(d.permissions.len(), 3, "the badly named permission is dropped");
        assert!(d.permissions[0].1 && !d.permissions[2].1);
        assert_eq!((d.description.as_str(), d.category), ("Chat with friends", 4));
        assert_eq!(parse_line(r#"ACB1 {"p":"bad name","perms":[]}"#), None);
    }

    #[test]
    fn permissions_from_dumpsys_when_the_helper_is_unavailable() {
        let t = "Packages:\n  Package [com.chat] (abc):\n    requested permissions:\n      android.permission.CAMERA\n      android.permission.INTERNET\n      android.permission.RECORD_AUDIO\n    install permissions:\n      android.permission.INTERNET: granted=true\n    User 0: ceDataInode=5\n      runtime permissions:\n        android.permission.CAMERA: granted=true, flags=[ USER_SET]\n        android.permission.RECORD_AUDIO: granted=false, flags=[ ]\n";
        let p = parse_dumpsys_permissions(t);
        assert_eq!(p, vec![("android.permission.CAMERA".to_string(), true), ("android.permission.INTERNET".to_string(), true), ("android.permission.RECORD_AUDIO".to_string(), false)]);
        assert!(parse_dumpsys_permissions("nothing useful").is_empty());
    }

    #[test]
    fn verdicts_name_the_failing_step_and_give_a_hint() {
        let ok = |n: &str| CheckStep { name: n.into(), ok: true, detail: "fine".into() };
        let (v, works) = summarize(&[ok("a"), ok("b")]);
        assert!(works && v.starts_with("Works"));
        let bad = CheckStep { name: "Start the helper".into(), ok: false, detail: "java.lang.SecurityException: nope".into() };
        let (v, works) = summarize(&[ok("a"), bad]);
        assert!(!works && v.contains("Start the helper") && v.contains("USB debugging (Security settings)"), "{v}");
        assert!(hint_for("error: device unauthorized").contains("Accept the USB debugging prompt"));
        assert!(hint_for("something odd").contains("device report"));
    }

    #[test]
    fn hash_changes_with_content() {
        assert_ne!(fnv_hex(b"a"), fnv_hex(b"b"));
        assert_eq!(fnv_hex(b"abc").len(), 16);
    }
}
