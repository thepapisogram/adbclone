//! Basic facts about the phone: model, Android version, storage, battery level.
use crate::{AdbError, AdbRunner};
use serde::Serialize;

pub const CMD: &str = "echo @@PROPS; getprop ro.product.manufacturer; getprop ro.product.model; \
getprop ro.build.version.release; getprop ro.build.version.sdk; getprop ro.build.version.security_patch; \
echo @@DF; df -k /data; echo @@BATT; dumpsys battery";

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub android_release: Option<String>,
    pub sdk: Option<u32>,
    pub security_patch: Option<String>,
    pub storage_total_kb: Option<u64>,
    pub storage_used_kb: Option<u64>,
    pub storage_free_kb: Option<u64>,
    pub battery_level: Option<u32>,
}

impl DeviceInfo {
    /// Lowercase manufacturer, for matching rating vendors (`samsung`, `xiaomi`, ...).
    pub fn vendor(&self) -> Option<String> {
        self.manufacturer.as_ref().map(|m| m.trim().to_lowercase())
    }
}

/// Text between `@@NAME` and the next `@@` (without the newline that follows the marker).
pub(crate) fn section<'a>(text: &'a str, name: &str) -> &'a str {
    let marker = format!("@@{name}");
    let Some(start) = text.find(&marker) else { return "" };
    let body = &text[start + marker.len()..];
    let body = body.strip_prefix("\r\n").or_else(|| body.strip_prefix('\n')).unwrap_or(body);
    &body[..body.find("@@").unwrap_or(body.len())]
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

/// First three consecutive numbers after the filesystem name: total, used, available (KiB).
fn parse_df(out: &str) -> (Option<u64>, Option<u64>, Option<u64>) {
    // Long filesystem names can wrap onto their own line, so work on the whole text after the header.
    let body: Vec<&str> = out.lines().skip(1).flat_map(|l| l.split_whitespace()).collect();
    for w in body.windows(3) {
        if let (Ok(t), Ok(u), Ok(f)) = (w[0].parse(), w[1].parse(), w[2].parse()) {
            return (Some(t), Some(u), Some(f));
        }
    }
    (None, None, None)
}

pub fn parse_info(text: &str) -> DeviceInfo {
    let props: Vec<&str> = section(text, "PROPS").lines().collect();
    let prop = |i: usize| non_empty(props.get(i).copied());
    let (total, used, free) = parse_df(section(text, "DF"));
    let battery_level = section(text, "BATT")
        .lines()
        .find_map(|l| l.trim().strip_prefix("level:")?.trim().parse::<u32>().ok())
        .filter(|l| *l <= 100);
    DeviceInfo {
        manufacturer: prop(0),
        model: prop(1),
        android_release: prop(2),
        sdk: prop(3).and_then(|s| s.parse().ok()),
        security_patch: prop(4),
        storage_total_kb: total,
        storage_used_kb: used,
        storage_free_kb: free,
        battery_level,
    }
}

pub async fn fetch(adb: &AdbRunner, serial: &str) -> Result<DeviceInfo, AdbError> {
    Ok(parse_info(&adb.shell(serial, &[CMD]).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIXEL: &str = "@@PROPS\nGoogle\nPixel 7\n14\n34\n2024-05-05\n@@DF\nFilesystem     1K-blocks     Used Available Use% Mounted on\n/dev/block/dm-9 113311596 41234567  71000000  37% /data\n@@BATT\nCurrent Battery Service state:\n  AC powered: false\n  level: 81\n  temperature: 301\n";

    #[test]
    fn parses_modern_phone() {
        let i = parse_info(PIXEL);
        assert_eq!(i.manufacturer.as_deref(), Some("Google"));
        assert_eq!(i.model.as_deref(), Some("Pixel 7"));
        assert_eq!((i.android_release.as_deref(), i.sdk), (Some("14"), Some(34)));
        assert_eq!(i.security_patch.as_deref(), Some("2024-05-05"));
        assert_eq!((i.storage_total_kb, i.storage_used_kb, i.storage_free_kb), (Some(113311596), Some(41234567), Some(71000000)));
        assert_eq!(i.battery_level, Some(81));
        assert_eq!(i.vendor().as_deref(), Some("google"));
    }

    #[test]
    fn handles_wrapped_df_and_empty_props() {
        let t = "@@PROPS\nsamsung\nSM-G973F\n9\n28\n\n@@DF\nFilesystem 1K-blocks Used Available Use% Mounted on\n/dev/block/platform/soc/1d84000.ufshc/by-name/userdata\n   52428800 20000000 32000000 39% /data\n@@BATT\n  level: 5\n";
        let i = parse_info(t);
        assert_eq!(i.security_patch, None, "empty property becomes None");
        assert_eq!(i.sdk, Some(28));
        assert_eq!((i.storage_total_kb, i.storage_free_kb), (Some(52428800), Some(32000000)));
        assert_eq!(i.battery_level, Some(5));
    }

    #[test]
    fn missing_everything_is_all_none() {
        assert_eq!(parse_info(""), DeviceInfo::default());
        let i = parse_info("@@PROPS\n\n\n\n\n\n@@DF\ndf: /data: Permission denied\n@@BATT\n  level: 400\n");
        assert_eq!(i, DeviceInfo::default(), "garbage and out-of-range values are dropped");
    }
}
