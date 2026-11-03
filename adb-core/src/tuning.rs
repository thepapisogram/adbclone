//! Reversible performance and heat tweaks that need no root. Every change reads the previous value
//! first, checks afterwards that the phone really accepted it, and can be undone from the saved record.
use crate::deviceinfo::section;
use crate::packages::valid_name;
use crate::{AdbError, AdbRunner};
use serde::{Deserialize, Serialize};

const ALLOWED_OPS: &[&str] = &["RUN_ANY_IN_BACKGROUND", "RUN_IN_BACKGROUND"];
const APPOP_MODES: &[&str] = &["allow", "deny", "ignore", "default", "foreground"];
const BUCKETS: &[&str] = &["active", "working_set", "frequent", "rare", "restricted"];

/// Something on the phone we can read, change and restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    Setting { ns: String, key: String },
    AppOp { pkg: String, op: String },
    Standby { pkg: String },
}

/// What was changed, and the value to put back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub serial: String,
    /// Which feature made the change (`animations`, `battery_saver`, `refresh`, `background`).
    pub group: String,
    pub label: String,
    pub target: Target,
    /// `None` means "was not set": undo deletes the setting or resets the app op to default.
    pub prev: Option<String>,
    pub applied: String,
    pub time: u64,
}

fn valid_key(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
fn valid_value(s: &str) -> bool {
    !s.is_empty() && s.len() <= 16 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn check_target(t: &Target) -> Result<(), AdbError> {
    let ok = match t {
        Target::Setting { ns, key } => (ns == "global" || ns == "system") && valid_key(key),
        Target::AppOp { pkg, op } => valid_name(pkg) && ALLOWED_OPS.contains(&op.as_str()),
        Target::Standby { pkg } => valid_name(pkg),
    };
    if ok { Ok(()) } else { Err(AdbError::Device("Refusing an unexpected setting name.".into())) }
}

// ---- parsing ----

/// `settings get` prints `null` for an unset key.
pub fn parse_setting_value(out: &str) -> Option<String> {
    let v = out.trim();
    (!v.is_empty() && v != "null").then(|| v.to_string())
}

/// `cmd appops get <pkg> <op>`: "RUN_ANY_IN_BACKGROUND: deny", "...: allow; time=+3d", or "No operations."
pub fn parse_appop(out: &str, op: &str) -> String {
    for l in out.lines() {
        if let Some(rest) = l.trim().strip_prefix(op).and_then(|r| r.strip_prefix(':')) {
            let mode = rest.split([';', '(']).next().unwrap_or("").trim().to_lowercase();
            if rest.contains("(default)") {
                return "default".into();
            }
            if APPOP_MODES.contains(&mode.as_str()) {
                return mode;
            }
        }
    }
    "default".into()
}

/// `am get-standby-bucket` prints a number (10 active .. 45 restricted) or, on some builds, the name.
pub fn parse_standby(out: &str) -> Option<String> {
    let t = out.trim().to_lowercase();
    let name = match t.as_str() {
        "10" => "active",
        "20" => "working_set",
        "30" => "frequent",
        "40" => "rare",
        "45" => "restricted",
        other => other,
    };
    BUCKETS.contains(&name).then(|| name.to_string())
}

// ---- reading and writing ----

pub async fn read_target(adb: &AdbRunner, serial: &str, t: &Target) -> Result<Option<String>, AdbError> {
    check_target(t)?;
    match t {
        Target::Setting { ns, key } => Ok(parse_setting_value(&adb.shell(serial, &[&format!("settings get {ns} {key}")]).await?)),
        Target::AppOp { pkg, op } => Ok(Some(parse_appop(&adb.shell(serial, &[&format!("cmd appops get {pkg} {op}")]).await?, op))),
        Target::Standby { pkg } => Ok(parse_standby(&adb.shell(serial, &[&format!("am get-standby-bucket {pkg}")]).await?)),
    }
}

/// Write a value. `None` removes a setting, or resets an app op to `default`.
pub async fn write_target(adb: &AdbRunner, serial: &str, t: &Target, value: Option<&str>) -> Result<(), AdbError> {
    check_target(t)?;
    if let Some(v) = value {
        if !valid_value(v) {
            return Err(AdbError::Device("Refusing an unexpected value.".into()));
        }
    }
    let cmd = match (t, value) {
        (Target::Setting { ns, key }, Some(v)) => format!("settings put {ns} {key} {v}"),
        (Target::Setting { ns, key }, None) => format!("settings delete {ns} {key}"),
        (Target::AppOp { pkg, op }, v) => {
            let mode = v.unwrap_or("default");
            if !APPOP_MODES.contains(&mode) {
                return Err(AdbError::Device("Refusing an unexpected app-op mode.".into()));
            }
            format!("cmd appops set {pkg} {op} {mode}")
        }
        (Target::Standby { pkg }, Some(b)) if BUCKETS.contains(&b) => format!("am set-standby-bucket {pkg} {b}"),
        (Target::Standby { .. }, _) => return Err(AdbError::Device("Refusing an unexpected standby bucket.".into())),
    };
    let out = adb.shell(serial, &[&cmd]).await?;
    let l = out.to_lowercase();
    if l.contains("exception") || l.contains("error") || l.contains("not allowed") || l.contains("permission denial") {
        return Err(AdbError::Device(out.trim().to_string()));
    }
    Ok(())
}

fn same(read: &Option<String>, want: Option<&str>, t: &Target) -> bool {
    match (read, want) {
        (None, None) => true,
        (Some(r), Some(w)) => {
            if matches!(t, Target::Setting { .. }) {
                // "0.5" vs "0.50": compare numbers when both are numeric.
                match (r.parse::<f64>(), w.parse::<f64>()) {
                    (Ok(a), Ok(b)) => (a - b).abs() < 1e-6,
                    _ => r == w,
                }
            } else {
                r == w
            }
        }
        (Some(r), None) => matches!(t, Target::AppOp { .. }) && r == "default",
        (None, Some(_)) => false,
    }
}

/// Change something, keeping the original value. If the phone does not end up with the new value
/// (some makers silently ignore these writes), the old value is put back and an error is returned.
pub async fn apply(
    adb: &AdbRunner,
    serial: &str,
    t: &Target,
    value: &str,
    group: &str,
    label: &str,
    now: u64,
) -> Result<Record, AdbError> {
    let prev = read_target(adb, serial, t).await?;
    write_target(adb, serial, t, Some(value)).await?;
    let after = read_target(adb, serial, t).await?;
    if !same(&after, Some(value), t) {
        let _ = write_target(adb, serial, t, prev.as_deref()).await;
        return Err(AdbError::Device(format!("{label}: the phone ignored the change (it may block this on your model).")));
    }
    Ok(Record {
        id: crate::wireless::random_token(10),
        serial: serial.to_string(),
        group: group.to_string(),
        label: label.to_string(),
        target: t.clone(),
        prev,
        applied: value.to_string(),
        time: now,
    })
}

/// Put the saved value back.
pub async fn undo(adb: &AdbRunner, serial: &str, r: &Record) -> Result<(), AdbError> {
    write_target(adb, serial, &r.target, r.prev.as_deref()).await?;
    let now = read_target(adb, serial, &r.target).await?;
    if same(&now, r.prev.as_deref(), &r.target) {
        Ok(())
    } else {
        Err(AdbError::Device(format!("{}: could not be put back; check it in the phone's settings.", r.label)))
    }
}

// ---- what to change ----

pub struct Step {
    pub target: Target,
    pub value: &'static str,
    pub group: &'static str,
    pub label: &'static str,
}

fn setting(ns: &str, key: &str) -> Target {
    Target::Setting { ns: ns.into(), key: key.into() }
}

pub fn animation_steps() -> Vec<Step> {
    [("window_animation_scale", "Window animation 0.5x"), ("transition_animation_scale", "Transition animation 0.5x"), ("animator_duration_scale", "Animator duration 0.5x")]
        .into_iter()
        .map(|(k, label)| Step { target: setting("global", k), value: "0.5", group: "animations", label })
        .collect()
}

pub fn battery_saver_steps() -> Vec<Step> {
    vec![Step { target: setting("global", "low_power"), value: "1", group: "battery_saver", label: "Battery saver on" }]
}

pub fn refresh_steps() -> Vec<Step> {
    vec![
        Step { target: setting("system", "peak_refresh_rate"), value: "60.0", group: "refresh", label: "Peak refresh rate 60 Hz" },
        Step { target: setting("system", "min_refresh_rate"), value: "60.0", group: "refresh", label: "Minimum refresh rate 60 Hz" },
    ]
}

/// Stop an app waking up in the background: block "run any in background" and move it to the restricted bucket.
pub fn background_steps(pkg: &str) -> Vec<(Target, &'static str, &'static str)> {
    vec![
        (Target::AppOp { pkg: pkg.into(), op: "RUN_ANY_IN_BACKGROUND".into() }, "deny", "Background activity blocked"),
        (Target::Standby { pkg: pkg.into() }, "restricted", "Moved to the restricted standby bucket"),
    ]
}

// ---- status ----

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub animation_scale: Option<String>,
    pub battery_saver: Option<String>,
    pub peak_refresh: Option<String>,
    /// Highest refresh rate the screen offers; the 60 Hz cap only makes sense above 60.
    pub max_fps: Option<f32>,
}

pub const STATUS_CMD: &str = "echo @@TUNE; for k in window_animation_scale low_power; do echo \"global.$k $(settings get global $k)\"; done; \
echo \"system.peak_refresh_rate $(settings get system peak_refresh_rate)\"; echo @@FPS; dumpsys display | grep -o 'fps=[0-9.]*'";

pub fn parse_status(text: &str) -> Status {
    let mut st = Status::default();
    for l in section(text, "TUNE").lines() {
        let mut it = l.split_whitespace();
        let (k, v) = (it.next().unwrap_or(""), parse_setting_value(it.next().unwrap_or("")));
        match k {
            "global.window_animation_scale" => st.animation_scale = v,
            "global.low_power" => st.battery_saver = v,
            "system.peak_refresh_rate" => st.peak_refresh = v,
            _ => {}
        }
    }
    st.max_fps = section(text, "FPS")
        .lines()
        .filter_map(|l| l.trim().strip_prefix("fps=")?.parse::<f32>().ok())
        .filter(|f| (1.0..=500.0).contains(f))
        .fold(None, |m: Option<f32>, f| Some(m.map_or(f, |x| x.max(f))));
    st
}

pub async fn read_status(adb: &AdbRunner, serial: &str) -> Result<Status, AdbError> {
    Ok(parse_status(&adb.shell(serial, &[STATUS_CMD]).await?))
}

/// Free every app's cache. Harmless, but apps start a little slower the first time afterwards.
pub async fn trim_caches(adb: &AdbRunner, serial: &str) -> Result<(), AdbError> {
    // A free-space target far above any phone's storage makes Android clear all caches.
    let out = adb.shell(serial, &["pm trim-caches 1099511627776"]).await?;
    let l = out.to_lowercase();
    if l.contains("exception") || l.contains("error") || l.contains("unknown") {
        return Err(AdbError::Device(out.trim().to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_values() {
        assert_eq!(parse_setting_value("1.0\n"), Some("1.0".into()));
        assert_eq!(parse_setting_value("null\n"), None);
        assert_eq!(parse_setting_value(""), None);
    }

    #[test]
    fn appop_output_variants() {
        let op = "RUN_ANY_IN_BACKGROUND";
        assert_eq!(parse_appop("RUN_ANY_IN_BACKGROUND: deny", op), "deny");
        assert_eq!(parse_appop("RUN_ANY_IN_BACKGROUND: allow; time=+3d2h ago", op), "allow");
        assert_eq!(parse_appop("RUN_ANY_IN_BACKGROUND: allow (default)", op), "default");
        assert_eq!(parse_appop("No operations.", op), "default");
        assert_eq!(parse_appop("Error: Unknown package", op), "default");
        assert_eq!(parse_appop("  RUN_ANY_IN_BACKGROUND: ignore", op), "ignore");
    }

    #[test]
    fn standby_numbers_and_names() {
        assert_eq!(parse_standby("10\n"), Some("active".into()));
        assert_eq!(parse_standby("45"), Some("restricted".into()));
        assert_eq!(parse_standby("working_set"), Some("working_set".into()));
        assert_eq!(parse_standby("Unknown command: get-standby-bucket"), None);
        assert_eq!(parse_standby("50"), None);
    }

    #[test]
    fn status_parsing() {
        let t = "@@TUNE\nglobal.window_animation_scale 1.0\nglobal.low_power null\nsystem.peak_refresh_rate 90.0\n@@FPS\nfps=60.0\nfps=90.000\nfps=120.0\nfps=abc\n";
        let s = parse_status(t);
        assert_eq!(s.animation_scale.as_deref(), Some("1.0"));
        assert_eq!(s.battery_saver, None);
        assert_eq!(s.peak_refresh.as_deref(), Some("90.0"));
        assert_eq!(s.max_fps, Some(120.0));
        assert_eq!(parse_status(""), Status::default());
    }

    #[test]
    fn targets_and_values_are_validated() {
        assert!(check_target(&setting("global", "low_power")).is_ok());
        assert!(check_target(&setting("secure", "low_power")).is_err(), "only global and system namespaces");
        assert!(check_target(&setting("global", "x; reboot")).is_err());
        assert!(check_target(&Target::AppOp { pkg: "com.a".into(), op: "SYSTEM_ALERT_WINDOW".into() }).is_err());
        assert!(check_target(&Target::Standby { pkg: "a b".into() }).is_err());
        assert!(valid_value("60.0") && !valid_value("1; id") && !valid_value(""));
    }

    #[test]
    fn same_compares_numbers_and_defaults() {
        let s = setting("global", "x");
        assert!(same(&Some("0.5".into()), Some("0.50"), &s));
        assert!(same(&None, None, &s));
        assert!(!same(&None, Some("1"), &s));
        let op = Target::AppOp { pkg: "a.b".into(), op: "RUN_ANY_IN_BACKGROUND".into() };
        assert!(same(&Some("default".into()), None, &op));
    }

    #[test]
    fn step_lists() {
        assert_eq!(animation_steps().len(), 3);
        assert!(animation_steps().iter().all(|s| s.value == "0.5"));
        assert_eq!(background_steps("com.a").len(), 2);
        assert_eq!(refresh_steps().len(), 2);
    }
}
