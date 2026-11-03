//! Drives the real code paths against a fake `adb` (tests/fixtures/fake_adb.py): no device needed.
use adb_core::{labels, processes, wireless, AdbRunner};
use std::{collections::HashMap, fs, os::unix::fs::PermissionsExt, path::PathBuf};

const SNAP1: &str = "@@STAT\ncpu  1000 0 500 8500 0 0 0 0\ncpu0 250 0 125 2125 0 0 0 0\ncpu1 250 0 125 2125 0 0 0 0\n\
@@MEM\nMemTotal: 2000000 kB\nMemFree: 100000 kB\nMemAvailable: 900000 kB\n\
@@PS\nPID UID RSS NAME\n10 10200 90000 com.foo.app\n11 10200 20000 com.foo.app:sync\n1 0 2000 init\n\
@@PROC\n10 (com.foo.app) S 1 1 1 0 -1 0 0 0 0 0 100 20 0 0 20 0 1 0 1 1 1\n11 (com.foo.app:sync) S 1 1 1 0 -1 0 0 0 0 0 10 0 0 0 20 0 1 0 1 1 1\n1 (init) S 0 1 1 0 -1 0 0 0 0 0 5 5 0 0 20 0 1 0 1 1 1\n";

/// Sets up a temp dir with fixtures and a wrapper script; returns (runner, dir).
fn setup(name: &str, no_unzip: bool) -> (AdbRunner, PathBuf) {
    let dir = std::env::temp_dir().join(format!("adbm-fake-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for f in ["literal.apk", "reference.apk", "entities.apk", "nolabel.apk"] {
        fs::copy(fx.join(f), dir.join(f)).unwrap();
    }
    fs::write(dir.join("snap1.txt"), SNAP1).unwrap();
    // Second sample: 2 cores * 1000 elapsed ticks; com.foo.app used +200 ticks, init +0.
    fs::write(
        dir.join("snap2.txt"),
        SNAP1.replace("cpu  1000 0 500 8500", "cpu  1500 0 800 9700").replace(") S 1 1 1 0 -1 0 0 0 0 0 100 20", ") S 1 1 1 0 -1 0 0 0 0 0 250 70"),
    )
    .unwrap();
    let wrapper = dir.join("adb");
    let flag = if no_unzip { "--no-unzip" } else { "" };
    fs::write(&wrapper, format!("#!/bin/sh\nexec python3 '{}' '{}' {flag} \"$@\"\n", fx.join("fake_adb.py").display(), dir.display())).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    (AdbRunner::new(wrapper), dir)
}

#[tokio::test]
async fn label_via_unzip_fast_path() {
    let (adb, _d) = setup("unzip", false);
    let l = labels::fetch_label(&adb, "SER", "/data/app/~~x==/com.test.reference-y==/reference.apk").await.unwrap();
    assert_eq!(l.as_deref(), Some("Referenced App"));
    let l = labels::fetch_label(&adb, "SER", "/system/app/literal.apk").await.unwrap();
    assert_eq!(l.as_deref(), Some("Literal App"));
}

#[tokio::test]
async fn label_falls_back_to_pull_when_unzip_missing() {
    let (adb, _d) = setup("pull", true);
    let l = labels::fetch_label(&adb, "SER", "/system/app/entities.apk").await.unwrap();
    assert_eq!(l.as_deref(), Some("Fish & Chips"));
}

#[tokio::test]
async fn app_without_label_or_missing_apk_gives_none_not_error() {
    let (adb, _d) = setup("none", false);
    assert_eq!(labels::fetch_label(&adb, "SER", "/system/app/nolabel.apk").await.unwrap(), None);
    assert_eq!(labels::fetch_label(&adb, "SER", "/system/app/missing.apk").await.unwrap(), None);
}

#[tokio::test]
async fn unsafe_paths_are_rejected_before_reaching_adb() {
    let (adb, _d) = setup("unsafe", false);
    assert!(labels::fetch_label(&adb, "SER", "/x; rm -rf /").await.is_err());
}

#[tokio::test]
async fn stat_gives_cache_key_parts() {
    let (adb, _d) = setup("stat", false);
    let m = labels::stat_files(&adb, "SER", &["/system/app/literal.apk", "/system/app/nope.apk"]).await.unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m["/system/app/literal.apk"].1, 1700000000);
}

#[tokio::test]
async fn running_report_over_two_samples() {
    let (adb, _d) = setup("proc", false);
    let pkgs: HashMap<String, u32> = [("com.foo.app".to_string(), 10200)].into();
    let s1 = processes::snapshot(&adb, "SER").await.unwrap();
    let r1 = processes::build_report(&s1, None, &pkgs);
    let foo = r1.apps.iter().find(|a| a.package.as_deref() == Some("com.foo.app")).unwrap();
    assert_eq!((foo.rss_kb, foo.processes, foo.cpu), (110000, 2, None));

    let s2 = processes::snapshot(&adb, "SER").await.unwrap();
    let r2 = processes::build_report(&s2, Some(&s1), &pkgs);
    let foo = r2.apps.iter().find(|a| a.package.as_deref() == Some("com.foo.app")).unwrap();
    // total ticks grew by 2000 over 2 cores; foo used (250+70)-(100+20)=200 ticks => 100*2*200/2000 = 20% of one core.
    assert!((foo.cpu.unwrap() - 20.0).abs() < 0.01, "{:?}", foo.cpu);
    assert_eq!(r2.mem_total_kb, 2_000_000);
    assert_eq!(r2.cores, 2);
}

// ---- wireless ----

#[tokio::test]
async fn pair_success_wrong_code_and_protocol_fault() {
    let (adb, _d) = setup("pair", false);
    let ok = wireless::pair(&adb, "192.168.1.20:37099", "123456").await.unwrap();
    assert!(ok.starts_with("Successfully paired"));
    let wrong = wireless::pair(&adb, "192.168.1.20:37099", "000000").await.unwrap_err().to_string();
    assert!(wrong.contains("Wrong pairing code"), "{wrong}");
    // non-zero exit with stderr is also turned into a message
    let fault = wireless::pair(&adb, "192.168.1.20:37099", "999999").await.unwrap_err().to_string();
    assert!(fault.contains("protocol fault"), "{fault}");
}

#[tokio::test]
async fn bad_input_never_reaches_adb() {
    let (adb, d) = setup("badinput", false);
    assert!(wireless::pair(&adb, "1.2.3.4; reboot:99", "123456").await.is_err());
    assert!(wireless::pair(&adb, "192.168.1.20:37099", "12345").await.is_err());
    assert!(wireless::connect(&adb, "nonsense").await.is_err());
    assert!(!d.join("connected").exists());
}

#[tokio::test]
async fn connect_variants() {
    let (adb, _d) = setup("connect", false);
    assert!(wireless::connect(&adb, "192.168.1.20:41231").await.unwrap().starts_with("connected to"));
    assert!(wireless::connect(&adb, "192.168.1.20:41231").await.unwrap().starts_with("already connected"));
    let refused = wireless::connect(&adb, "192.168.1.20:1").await.unwrap_err().to_string();
    assert!(refused.contains("Wireless debugging"), "{refused}");
    let unreachable = wireless::connect(&adb, "10.9.9.9:5555").await.unwrap_err().to_string();
    assert!(unreachable.contains("same Wi-Fi"), "{unreachable}");
    wireless::disconnect(&adb, "192.168.1.20:41231").await.unwrap();
}

#[tokio::test]
async fn discovery_and_qr_pairing_flow() {
    let (adb, d) = setup("mdns", false);
    assert!(wireless::mdns_services(&adb).await.unwrap().is_empty());
    let session = wireless::new_qr_session().unwrap();
    // The phone scans the QR and starts advertising a pairing service named after the session.
    fs::write(
        d.join("mdns.txt"),
        format!("{}\t_adb-tls-pairing._tcp.\t192.168.1.20:37099\nadb-R58M-xyz\t_adb-tls-connect._tcp.\t192.168.1.20:41231\n", session.name),
    )
    .unwrap();
    let services = wireless::mdns_services(&adb).await.unwrap();
    let pairing = services.iter().find(|s| s.name == session.name && s.kind == wireless::MdnsKind::Pairing).unwrap();
    assert_eq!(pairing.address, "192.168.1.20:37099");
    assert_eq!(wireless::adb_major_version(&adb).await.unwrap(), 34);
}

#[tokio::test]
async fn usb_to_wireless_switch() {
    let (adb, d) = setup("tcpip", false);
    fs::write(d.join("ipaddr.txt"), "22: wlan0: <UP>\n    inet 192.168.1.20/24 brd 192.168.1.255 scope global wlan0\n").unwrap();
    let addr = wireless::usb_to_wireless(&adb, "R58M123ABC", 5555).await.unwrap();
    assert_eq!(addr, "192.168.1.20:5555");
    assert!(fs::read_to_string(d.join("connected")).unwrap().contains("192.168.1.20:5555"));
}

#[tokio::test]
async fn usb_to_wireless_without_wifi_explains() {
    let (adb, _d) = setup("nowifi", false);
    let e = wireless::usb_to_wireless(&adb, "R58M123ABC", 5555).await.unwrap_err().to_string();
    assert!(e.contains("Wi-Fi"), "{e}");
}
