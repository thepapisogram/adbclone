//! Temperature, battery and throttling readings from sources that need no root.
//! Every source is optional: phones block different ones, so a missing source is reported, not an error.
use crate::deviceinfo::section;
use crate::{AdbError, AdbRunner};
use serde::Serialize;

pub const CMD: &str = "echo @@BATT; dumpsys battery; echo @@THERMAL; dumpsys thermalservice 2>/dev/null; \
echo @@ZONES; for z in /sys/class/thermal/thermal_zone*; do echo \"$(cat $z/type 2>/dev/null) $(cat $z/temp 2>/dev/null)\"; done; \
echo @@CPU; for c in /sys/devices/system/cpu/cpu[0-9]*; do echo \"${c##*/} $(cat $c/cpufreq/scaling_cur_freq 2>/dev/null) $(cat $c/cpufreq/cpuinfo_max_freq 2>/dev/null)\"; done";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub level: Option<u32>,
    pub temp_c: Option<f32>,
    pub status: String,
    pub health: String,
    pub charging: bool,
    /// `ac`, `usb`, `wireless` or `none`.
    pub power_source: String,
    pub voltage_mv: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sensor {
    pub name: String,
    pub temp_c: f32,
    /// `hal` (thermal service) or `zone` (/sys/class/thermal).
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuFreq {
    pub core: String,
    pub cur_khz: u64,
    pub max_khz: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthSample {
    pub battery: Option<Battery>,
    /// 0 none .. 6 shutdown (Android's thermal status scale).
    pub thermal_status: Option<u8>,
    pub thermal_status_name: Option<String>,
    pub sensors: Vec<Sensor>,
    pub hottest: Option<Sensor>,
    pub cpu: Vec<CpuFreq>,
    /// Sources that returned nothing on this phone.
    pub missing: Vec<String>,
}

/// Kernel values come as degrees, tenths or millidegrees depending on the driver.
pub fn normalize_temp(raw: f64) -> Option<f32> {
    let c = if raw.abs() > 1000.0 { raw / 1000.0 } else if raw.abs() > 200.0 { raw / 10.0 } else { raw };
    // 0 is how many drivers say "no reading"; nothing real in a phone is outside this range.
    (c != 0.0 && (-30.0..=150.0).contains(&c)).then_some(c as f32)
}

pub fn thermal_status_name(n: u8) -> &'static str {
    ["none", "light", "moderate", "severe", "critical", "emergency", "shutdown"].get(n as usize).copied().unwrap_or("unknown")
}

fn field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| Some(l.trim().strip_prefix(key)?.strip_prefix(':')?.trim().to_string()))
}

pub fn parse_battery(text: &str) -> Option<Battery> {
    if !text.contains("level") && !text.contains("temperature") {
        return None;
    }
    let num = |k: &str| field(text, k).and_then(|v| v.parse::<i64>().ok());
    let flag = |k: &str| field(text, k).map_or(false, |v| v == "true");
    let status = match num("status") {
        Some(2) => "charging",
        Some(3) => "discharging",
        Some(4) => "not charging",
        Some(5) => "full",
        _ => "unknown",
    };
    let health = match num("health") {
        Some(2) => "good",
        Some(3) => "overheat",
        Some(4) => "dead",
        Some(5) => "over voltage",
        Some(7) => "cold",
        Some(6) => "failure",
        _ => "unknown",
    };
    let power_source = if flag("AC powered") { "ac" } else if flag("USB powered") { "usb" } else if flag("Wireless powered") || flag("Dock powered") { "wireless" } else { "none" };
    Some(Battery {
        level: num("level").filter(|l| (0..=100).contains(l)).map(|l| l as u32),
        temp_c: num("temperature").and_then(|t| normalize_temp(t as f64)),
        status: status.into(),
        health: health.into(),
        charging: num("status") == Some(2),
        power_source: power_source.into(),
        voltage_mv: num("voltage").filter(|v| *v > 0).map(|v| v as u32),
    })
}

/// `Thermal Status: N` and HAL temperatures `Temperature{mValue=36.5, mType=3, mName=battery, ...}`.
pub fn parse_thermalservice(text: &str) -> (Option<u8>, Vec<Sensor>) {
    let status = field(text, "Thermal Status").and_then(|v| v.parse::<u8>().ok()).filter(|n| *n <= 6);
    // Prefer the "current" block; an older block would show stale values.
    let block: Vec<&str> = match text.find("Current temperatures from HAL") {
        Some(i) => text[i..].lines().skip(1).skip_while(|l| l.trim().is_empty()).take_while(|l| l.contains("mValue=")).collect(),
        None => text.lines().filter(|l| l.contains("mValue=")).collect(),
    };
    let mut sensors: Vec<Sensor> = Vec::new();
    for l in block {
        let val = l.split("mValue=").nth(1).and_then(|r| r.split([',', '}']).next()).and_then(|v| v.trim().parse::<f64>().ok());
        let name = l.split("mName=").nth(1).and_then(|r| r.split([',', '}']).next()).map(str::trim);
        if let (Some(v), Some(n)) = (val.and_then(normalize_temp), name) {
            if !sensors.iter().any(|s| s.name == n) {
                sensors.push(Sensor { name: n.to_string(), temp_c: v, source: "hal".into() });
            }
        }
    }
    (status, sensors)
}

pub fn parse_zones(text: &str) -> Vec<Sensor> {
    text.lines()
        .filter_map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            let (raw, name) = t.split_last()?;
            let temp = normalize_temp(raw.parse::<f64>().ok()?)?;
            (!name.is_empty()).then(|| Sensor { name: name.join(" "), temp_c: temp, source: "zone".into() })
        })
        .collect()
}

pub fn parse_cpu(text: &str) -> Vec<CpuFreq> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (core, cur, max) = (it.next()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
            (max > 0 && it.next().is_none()).then(|| CpuFreq { core: core.to_string(), cur_khz: cur, max_khz: max })
        })
        .collect()
}

pub fn parse_sample(text: &str) -> HealthSample {
    let battery = parse_battery(section(text, "BATT"));
    let (thermal_status, hal) = parse_thermalservice(section(text, "THERMAL"));
    let zones = parse_zones(section(text, "ZONES"));
    let cpu = parse_cpu(section(text, "CPU"));
    let mut sensors = hal;
    sensors.extend(zones);
    // The hottest reading is the most useful single number, but a zero-ish unused sensor must not win.
    let hottest = sensors.iter().cloned().max_by(|a, b| a.temp_c.total_cmp(&b.temp_c));
    let mut missing = Vec::new();
    if battery.is_none() { missing.push("battery".to_string()); }
    if thermal_status.is_none() { missing.push("thermal status".to_string()); }
    if sensors.is_empty() { missing.push("temperature sensors".to_string()); }
    if cpu.is_empty() { missing.push("cpu frequencies".to_string()); }
    HealthSample {
        battery,
        thermal_status,
        thermal_status_name: thermal_status.map(|n| thermal_status_name(n).to_string()),
        sensors,
        hottest,
        cpu,
        missing,
    }
}

pub async fn sample(adb: &AdbRunner, serial: &str) -> Result<HealthSample, AdbError> {
    Ok(parse_sample(&adb.shell(serial, &[CMD]).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BATT_14: &str = "Current Battery Service state:\n  AC powered: false\n  USB powered: true\n  Wireless powered: false\n  Max charging current: 500000\n  status: 2\n  health: 2\n  present: true\n  level: 67\n  scale: 100\n  voltage: 4012\n  temperature: 342\n  technology: Li-ion\n";

    #[test]
    fn battery_fields() {
        let b = parse_battery(BATT_14).unwrap();
        assert_eq!(b.level, Some(67));
        assert_eq!(b.temp_c, Some(34.2));
        assert!(b.charging);
        assert_eq!((b.power_source.as_str(), b.status.as_str(), b.health.as_str()), ("usb", "charging", "good"));
        assert_eq!(b.voltage_mv, Some(4012));
        assert!(parse_battery("").is_none());
        let hot = parse_battery("  status: 3\n  health: 3\n  level: 40\n  temperature: 468\n").unwrap();
        assert_eq!((hot.health.as_str(), hot.charging, hot.power_source.as_str()), ("overheat", false, "none"));
    }

    #[test]
    fn temperature_units_are_normalised() {
        assert_eq!(normalize_temp(36.5), Some(36.5));
        assert_eq!(normalize_temp(342.0), Some(34.2));
        assert_eq!(normalize_temp(42000.0), Some(42.0));
        for bad in [0.0, -274000.0, 999999.0 * 1000.0, f64::NAN] {
            assert_eq!(normalize_temp(bad), None, "{bad}");
        }
    }

    const THERMAL_14: &str = "IsStatusOverride: false\nThermal Status: 1\nCached temperatures:\n\tTemperature{mValue=30.0, mType=3, mName=battery, mStatus=0}\nHAL Ready: true\nCurrent temperatures from HAL:\n\tTemperature{mValue=36.5, mType=3, mName=battery, mStatus=0}\n\tTemperature{mValue=47.25, mType=0, mName=cpu0, mStatus=1}\n\tTemperature{mValue=NaN, mType=1, mName=gpu, mStatus=0}\nCurrent cooling devices from HAL:\n\tCoolingDevice{mValue=0, mType=1, mName=thermal-cpufreq-0}\n";

    #[test]
    fn thermalservice_current_block_only() {
        let (status, sensors) = parse_thermalservice(THERMAL_14);
        assert_eq!(status, Some(1));
        let names: Vec<_> = sensors.iter().map(|s| (s.name.as_str(), s.temp_c)).collect();
        assert_eq!(names, [("battery", 36.5), ("cpu0", 47.25)], "cached block, NaN and cooling devices are ignored");
        assert_eq!(parse_thermalservice("Unknown service").0, None);
        assert_eq!(parse_thermalservice("Thermal Status: 9").0, None);
    }

    #[test]
    fn zones_and_cpu() {
        let z = parse_zones("battery 31500\ncpu-1-0-usr 52000\nsoc thermal 48100\nbroken\nunused 0\ntype-only \n -274000 \n");
        let n: Vec<_> = z.iter().map(|s| (s.name.as_str(), s.temp_c)).collect();
        assert_eq!(n, [("battery", 31.5), ("cpu-1-0-usr", 52.0), ("soc thermal", 48.1)]);
        let c = parse_cpu("cpu0 1804800 1804800\ncpu4 1209600 2400000\ncpu5  \ncpu6 abc 100\ncpu7 500000 0\n");
        assert_eq!(c.len(), 2);
        assert_eq!(c[1], CpuFreq { core: "cpu4".into(), cur_khz: 1209600, max_khz: 2400000 });
    }

    #[test]
    fn full_sample_reports_hottest_and_missing() {
        let t = format!("@@BATT\n{BATT_14}@@THERMAL\n{THERMAL_14}@@ZONES\nbattery 31500\ncpu-1-0-usr 62000\n@@CPU\ncpu0 1000000 2000000\n");
        let s = parse_sample(&t);
        assert_eq!(s.thermal_status, Some(1));
        assert_eq!(s.thermal_status_name.as_deref(), Some("light"));
        assert_eq!(s.sensors.len(), 4);
        assert_eq!(s.hottest.as_ref().map(|h| (h.name.as_str(), h.temp_c)), Some(("cpu-1-0-usr", 62.0)));
        assert!(s.missing.is_empty());
    }

    #[test]
    fn old_android_without_thermalservice() {
        // Android 9: no thermalservice, zones in millidegrees, battery only.
        let t = format!("@@BATT\n{BATT_14}@@THERMAL\nCan't find service: thermalservice\n@@ZONES\nbattery 29000\n@@CPU\n");
        let s = parse_sample(&t);
        assert_eq!(s.thermal_status, None);
        assert_eq!(s.sensors.len(), 1);
        assert_eq!(s.missing, ["thermal status", "cpu frequencies"]);
        let empty = parse_sample("");
        assert_eq!(empty.missing.len(), 4);
        assert!(empty.battery.is_none() && empty.hottest.is_none());
    }
}
