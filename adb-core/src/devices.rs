use crate::{AdbError, AdbRunner};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceState {
    Device,
    Unauthorized,
    Offline,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Device {
    pub serial: String,
    pub state: DeviceState,
    pub model: Option<String>,
    pub product: Option<String>,
}

/// Parse the output of `adb devices -l`.
pub fn parse_devices(output: &str) -> Vec<Device> {
    output
        .lines()
        .skip_while(|l| !l.starts_with("List of devices"))
        .skip(1)
        .filter(|l| !l.trim().is_empty() && !l.starts_with('*'))
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = match parts.next()? {
                "device" => DeviceState::Device,
                "unauthorized" => DeviceState::Unauthorized,
                "offline" => DeviceState::Offline,
                other => DeviceState::Other(other.to_string()),
            };
            let (mut model, mut product) = (None, None);
            for kv in parts {
                if let Some(v) = kv.strip_prefix("model:") {
                    model = Some(v.to_string());
                } else if let Some(v) = kv.strip_prefix("product:") {
                    product = Some(v.to_string());
                }
            }
            Some(Device { serial, state, model, product })
        })
        .collect()
}

pub async fn list_devices(adb: &AdbRunner) -> Result<Vec<Device>, AdbError> {
    Ok(parse_devices(&adb.run(&["devices", "-l"]).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_states() {
        let out = "* daemon started successfully\nList of devices attached\n\
R58M123ABC device usb:1-1 product:beyond1 model:SM_G973F device:beyond1 transport_id:1\n\
192.168.1.20:5555 unauthorized transport_id:2\n\
emulator-5554 offline\n\n";
        let d = parse_devices(out);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].state, DeviceState::Device);
        assert_eq!(d[0].model.as_deref(), Some("SM_G973F"));
        assert_eq!(d[1].state, DeviceState::Unauthorized);
        assert_eq!(d[2].state, DeviceState::Offline);
    }

    #[test]
    fn empty_list() {
        assert!(parse_devices("List of devices attached\n\n").is_empty());
    }
}
