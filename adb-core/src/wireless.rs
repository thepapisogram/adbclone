//! Wireless ADB: pairing (Android 11+), connecting, USB-to-Wi-Fi switching and mDNS discovery.
use crate::{AdbError, AdbRunner};
use serde::Serialize;

/// `host:port` with an IPv4 address or hostname and a port in 1..=65535.
pub fn valid_address(s: &str) -> bool {
    let Some((host, port)) = s.rsplit_once(':') else { return false };
    let port_ok = port.parse::<u32>().map_or(false, |p| (1..=65535).contains(&p));
    let host_ok = !host.is_empty()
        && host.len() <= 253
        && !host.starts_with(['-', '.'])
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    port_ok && host_ok
}

pub fn valid_pairing_code(s: &str) -> bool {
    s.len() == 6 && s.chars().all(|c| c.is_ascii_digit())
}

/// Turn raw adb output into advice a person can act on.
pub fn friendly_error(raw: &str) -> String {
    let l = raw.to_lowercase();
    let msg = if l.contains("wrong password") || l.contains("pairing code") {
        "Wrong pairing code, or the code expired. Reopen \"Pair device with pairing code\" on the phone and try again."
    } else if l.contains("refused") {
        "Connection refused. Is Wireless debugging turned on? The port changes every time you toggle it: use the port shown on the phone (or the discovered one)."
    } else if l.contains("timed out") || l.contains("10060") || l.contains("no route") || l.contains("unreachable") || l.contains("10061") {
        "Can't reach the phone. Check that the phone and this computer are on the same Wi-Fi network, and that the router does not isolate wireless clients."
    } else if l.contains("unauthorized") {
        "The phone has not authorised this computer. Pair it first (Wireless debugging > Pair device)."
    } else if l.contains("unknown host service") || l.contains("unknown command") {
        "This adb is too old for wireless pairing. Update Android platform-tools to version 30 or newer."
    } else {
        return raw.trim().to_string();
    };
    msg.to_string()
}

/// `adb pair` prints "Successfully paired to HOST:PORT [guid=...]" or "Failed: ...".
pub fn parse_pair_result(out: &str) -> Result<String, String> {
    let t = out.trim();
    if t.starts_with("Successfully paired") {
        Ok(t.to_string())
    } else {
        Err(friendly_error(t.strip_prefix("Failed:").unwrap_or(t).trim()))
    }
}

/// `adb connect` prints "connected to X", "already connected to X", or "failed/cannot connect to X: why".
pub fn parse_connect_result(out: &str) -> Result<String, String> {
    let t = out.trim();
    if t.starts_with("connected to") || t.starts_with("already connected to") {
        Ok(t.to_string())
    } else {
        Err(friendly_error(t))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MdnsKind {
    Pairing,
    Connect,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MdnsService {
    pub name: String,
    pub kind: MdnsKind,
    pub address: String,
}

/// Parse `adb mdns services` (tab- or space-separated: name, type, address).
pub fn parse_mdns(out: &str) -> Vec<MdnsService> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (name, ty, addr) = (it.next()?, it.next()?, it.next()?);
            if !ty.starts_with('_') || !valid_address(addr) {
                return None; // header line or noise
            }
            let kind = if ty.starts_with("_adb-tls-pairing") {
                MdnsKind::Pairing
            } else if ty.starts_with("_adb-tls-connect") {
                MdnsKind::Connect
            } else {
                MdnsKind::Other
            };
            Some(MdnsService { name: name.to_string(), kind, address: addr.to_string() })
        })
        .collect()
}

/// First IPv4 address in `ip -f inet addr show` output, skipping loopback.
pub fn parse_ip(out: &str) -> Option<String> {
    out.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("inet ")?;
        let ip = rest.split(['/', ' ']).next()?;
        (!ip.starts_with("127.") && ip.split('.').count() == 4).then(|| ip.to_string())
    })
}

/// Major version of `adb version` ("Version 34.0.4-..." line), if present.
pub fn parse_adb_major(out: &str) -> Option<u32> {
    out.lines().find_map(|l| l.trim().strip_prefix("Version ")?.split('.').next()?.parse().ok())
}

/// What a phone scans to pair without typing a code.
pub fn qr_payload(name: &str, password: &str) -> String {
    format!("WIFI:T:ADB;S:{name};P:{password};;")
}

pub fn qr_svg(payload: &str) -> Option<String> {
    let code = qrcode::QrCode::new(payload.as_bytes()).ok()?;
    Some(code.render::<qrcode::render::svg::Color>().min_dimensions(220, 220).quiet_zone(true).build())
}

/// Random lowercase-alphanumeric string from the OS random source.
pub fn random_token(len: usize) -> String {
    const CH: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut buf = vec![0u8; len];
    getrandom::fill(&mut buf).expect("system random source unavailable");
    buf.iter().map(|b| CH[*b as usize % CH.len()] as char).collect()
}

pub struct QrSession {
    pub name: String,
    pub password: String,
    pub svg: String,
}

pub fn new_qr_session() -> Option<QrSession> {
    let name = format!("adbclone-{}", random_token(6));
    let password = random_token(10);
    let svg = qr_svg(&qr_payload(&name, &password))?;
    Some(QrSession { name, password, svg })
}

pub async fn adb_major_version(adb: &AdbRunner) -> Result<u32, AdbError> {
    let out = adb.run(&["version"]).await?;
    parse_adb_major(&out).ok_or_else(|| AdbError::Device(format!("could not read adb version: {}", out.trim())))
}

fn describe(e: AdbError) -> AdbError {
    match e {
        AdbError::Failed { stderr, .. } => AdbError::Device(friendly_error(&stderr)),
        other => other,
    }
}

pub async fn mdns_services(adb: &AdbRunner) -> Result<Vec<MdnsService>, AdbError> {
    Ok(parse_mdns(&adb.run(&["mdns", "services"]).await.map_err(describe)?))
}

/// Pair with the phone. The code is passed to adb but never logged or echoed in errors.
pub async fn pair(adb: &AdbRunner, address: &str, code: &str) -> Result<String, AdbError> {
    if !valid_address(address) {
        return Err(AdbError::Device("Enter the pairing address as IP:port, for example 192.168.1.20:37099.".into()));
    }
    if !valid_pairing_code(code) {
        return Err(AdbError::Device("The pairing code is 6 digits.".into()));
    }
    // Some adb versions exit non-zero and print to stderr, others print "Failed:" on stdout.
    let out = adb.run(&["pair", address, code]).await.map_err(describe)?;
    parse_pair_result(&out).map_err(AdbError::Device)
}

pub async fn connect(adb: &AdbRunner, address: &str) -> Result<String, AdbError> {
    if !valid_address(address) {
        return Err(AdbError::Device("Enter the address as IP:port, for example 192.168.1.20:41231.".into()));
    }
    let out = adb.run(&["connect", address]).await.map_err(describe)?;
    parse_connect_result(&out).map_err(AdbError::Device)
}

pub async fn disconnect(adb: &AdbRunner, address: &str) -> Result<(), AdbError> {
    if !valid_address(address) {
        return Err(AdbError::Device("Not a valid address.".into()));
    }
    adb.run(&["disconnect", address]).await.map_err(describe)?;
    Ok(())
}

/// Switch a USB-connected phone to TCP/IP and connect to it. Returns the new `ip:port`.
pub async fn usb_to_wireless(adb: &AdbRunner, serial: &str, port: u16) -> Result<String, AdbError> {
    let ip_out = adb.shell(serial, &["ip -f inet addr show wlan0"]).await.map_err(describe)?;
    let ip = parse_ip(&ip_out)
        .ok_or_else(|| AdbError::Device("The phone has no Wi-Fi address. Connect it to Wi-Fi first.".into()))?;
    let port_s = port.to_string();
    adb.run(&["-s", serial, "tcpip", &port_s]).await.map_err(describe)?;
    // adbd restarts in TCP mode; give it a moment before connecting.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let address = format!("{ip}:{port}");
    connect(adb, &address).await?;
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_addresses_and_codes() {
        assert!(valid_address("192.168.1.20:37099"));
        assert!(valid_address("phone.local:5555"));
        for bad in ["", "192.168.1.20", "192.168.1.20:0", "192.168.1.20:70000", ":5555", "a b:5555", "-x:5555", "1.2.3.4:5555; rm -rf /", "host:12ab"] {
            assert!(!valid_address(bad), "{bad}");
        }
        assert!(valid_pairing_code("012345"));
        for bad in ["12345", "1234567", "12345a", "", "12 456"] {
            assert!(!valid_pairing_code(bad), "{bad}");
        }
    }

    #[test]
    fn pair_results() {
        assert!(parse_pair_result("Successfully paired to 192.168.1.20:37099 [guid=adb-ABC-xyz]\n").is_ok());
        let e = parse_pair_result("Failed: Wrong password or connection was dropped. Check the pairing code and try again.").unwrap_err();
        assert!(e.contains("Wrong pairing code"), "{e}");
        let e = parse_pair_result("error: protocol fault (couldn't read status): connection reset").unwrap_err();
        assert!(e.contains("protocol fault"), "{e}");
    }

    #[test]
    fn connect_results() {
        assert!(parse_connect_result("connected to 192.168.1.20:41231\n").is_ok());
        assert!(parse_connect_result("already connected to 192.168.1.20:41231").is_ok());
        let e = parse_connect_result("failed to connect to '192.168.1.20:41231': Connection refused").unwrap_err();
        assert!(e.contains("Wireless debugging"), "{e}");
        let e = parse_connect_result("cannot connect to 192.168.1.99:5555: A connection attempt failed (10060)").unwrap_err();
        assert!(e.contains("same Wi-Fi"), "{e}");
    }

    #[test]
    fn mdns_listing() {
        let out = "List of discovered mdns services\nadb-R58M123ABC-AbCdEf\t_adb-tls-connect._tcp.\t192.168.1.20:41231\n\
adb-R58M123ABC-AbCdEf\t_adb-tls-pairing._tcp.\t192.168.1.20:37099\nsomething\t_other._tcp.\t10.0.0.5:80\n";
        let s = parse_mdns(out);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].kind, MdnsKind::Connect);
        assert_eq!(s[1].kind, MdnsKind::Pairing);
        assert_eq!(s[1].address, "192.168.1.20:37099");
        assert_eq!(s[2].kind, MdnsKind::Other);
        assert!(parse_mdns("List of discovered mdns services\n").is_empty());
    }

    #[test]
    fn ip_and_version() {
        let out = "22: wlan0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500\n    inet 192.168.1.20/24 brd 192.168.1.255 scope global wlan0\n       valid_lft forever\n";
        assert_eq!(parse_ip(out).as_deref(), Some("192.168.1.20"));
        assert_eq!(parse_ip("    inet 127.0.0.1/8 scope host lo\n"), None);
        assert_eq!(parse_ip("Device \"wlan0\" does not exist.\n"), None);
        assert_eq!(parse_adb_major("Android Debug Bridge version 1.0.41\nVersion 34.0.4-10411341\nInstalled as /usr/bin/adb\n"), Some(34));
        assert_eq!(parse_adb_major("garbage"), None);
    }

    #[test]
    fn qr_payload_and_svg() {
        assert_eq!(qr_payload("adbclone-abc123", "secret1234"), "WIFI:T:ADB;S:adbclone-abc123;P:secret1234;;");
        let s = new_qr_session().unwrap();
        assert!(s.svg.contains("<svg"));
        assert!(s.name.starts_with("adbclone-") && s.password.len() == 10);
        assert_ne!(new_qr_session().unwrap().password, s.password, "tokens must differ");
    }
}
