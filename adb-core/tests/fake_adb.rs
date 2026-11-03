//! Drives the real code paths against a fake `adb` (tests/fixtures/fake_adb.py): no device needed.
use adb_core::{tuning, deviceinfo, health, labels, packages::{self, PkgState}, processes, snapshot, wireless, AdbRunner};
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

// ---- snapshot and restore against a stateful fake device ----

fn write_db(dir: &PathBuf, rows: &[(&str, bool, &str)]) {
    let body: Vec<String> = rows.iter().map(|(n, sys, st)| format!("\"{n}\": {{\"system\": {sys}, \"state\": \"{st}\"}}")).collect();
    fs::write(dir.join("pkgdb.json"), format!("{{{}}}", body.join(","))).unwrap();
}

async fn current(adb: &AdbRunner) -> std::collections::HashMap<String, PkgState> {
    packages::list_states(adb, "SER").await.unwrap().into_iter().map(|p| (p.name, p.state)).collect()
}

#[tokio::test]
async fn list_states_reads_all_four_states() {
    let (adb, d) = setup("states", false);
    write_db(&d, &[("com.a", true, "enabled"), ("com.b", true, "disabled"), ("com.c", false, "enabled"), ("com.d", true, "uninstalled")]);
    let s = packages::list_states(&adb, "SER").await.unwrap();
    let get = |n: &str| s.iter().find(|p| p.name == n).unwrap();
    assert_eq!(get("com.a").state, PkgState::Enabled);
    assert_eq!(get("com.b").state, PkgState::Disabled);
    assert_eq!(get("com.d").state, PkgState::Uninstalled);
    assert!(get("com.a").system && !get("com.c").system);
    assert_eq!(s.len(), 4);
}

#[tokio::test]
async fn snapshot_then_restore_brings_device_back() {
    let (adb, d) = setup("restore", false);
    write_db(&d, &[("com.a", true, "enabled"), ("com.b", true, "disabled"), ("com.c", false, "enabled"), ("com.d", true, "uninstalled")]);
    let entries: Vec<_> = packages::list_states(&adb, "SER").await.unwrap().into_iter()
        .map(|p| snapshot::PackageEntry { name: p.name, state: p.state }).collect();
    let snap = snapshot::Snapshot::new("before", 1, false, None, entries);
    let before = current(&adb).await;

    // The "debloat": disable a, enable b, uninstall c, restore d.
    packages::disable(&adb, "SER", "com.a").await.unwrap();
    packages::enable(&adb, "SER", "com.b").await.unwrap();
    packages::uninstall_for_user(&adb, "SER", "com.c").await.unwrap();
    packages::restore(&adb, "SER", "com.d").await.unwrap();
    assert_ne!(current(&adb).await, before);

    let plan = snapshot::plan_restore(&snap, &current(&adb).await);
    assert_eq!(plan.changes.len(), 4);
    let out = snapshot::execute(&adb, "SER", &plan).await;
    assert!(out.iter().all(|o| o.ok), "{out:?}");
    assert_eq!(current(&adb).await, before, "device matches the snapshot again");
    assert!(snapshot::plan_restore(&snap, &current(&adb).await).changes.is_empty());
}

#[tokio::test]
async fn failed_step_is_reported_and_others_continue() {
    let (adb, d) = setup("restorefail", false);
    write_db(&d, &[("com.fail.pkg", false, "enabled"), ("com.ok", false, "enabled")]);
    let entries: Vec<_> = packages::list_states(&adb, "SER").await.unwrap().into_iter()
        .map(|p| snapshot::PackageEntry { name: p.name, state: p.state }).collect();
    let snap = snapshot::Snapshot::new("s", 1, false, None, entries);
    // Put both into states that need a step to fix; the fake refuses to enable "fail" packages.
    write_db(&d, &[("com.fail.pkg", false, "disabled"), ("com.ok", false, "disabled")]);
    let plan = snapshot::plan_restore(&snap, &current(&adb).await);
    let out = snapshot::execute(&adb, "SER", &plan).await;
    let bad = out.iter().find(|o| o.name == "com.fail.pkg").unwrap();
    assert!(!bad.ok && bad.message.as_deref().unwrap().starts_with("enable:"), "{bad:?}");
    assert!(out.iter().find(|o| o.name == "com.ok").unwrap().ok);
    let left = snapshot::plan_restore(&snap, &current(&adb).await);
    assert_eq!(left.changes.len(), 1, "only the failed package is still different");
}

// ---- device info and health ----

#[tokio::test]
async fn device_info_round_trip() {
    let (adb, d) = setup("devinfo", false);
    fs::write(d.join("deviceinfo.txt"), "@@PROPS\nSamsung\nSM-G973F\n12\n31\n2023-11-01\n@@DF\nFilesystem 1K-blocks Used Available Use% Mounted on\n/dev/x 100 40 60 40% /data\n@@BATT\n  level: 73\n").unwrap();
    let i = deviceinfo::fetch(&adb, "SER").await.unwrap();
    assert_eq!((i.vendor().as_deref(), i.sdk, i.battery_level, i.storage_free_kb), (Some("samsung"), Some(31), Some(73), Some(60)));
}

#[tokio::test]
async fn health_sample_round_trip_and_blocked_sensors() {
    let (adb, d) = setup("health", false);
    fs::write(d.join("health.txt"), "@@BATT\n  status: 3\n  level: 50\n  temperature: 388\n@@THERMAL\nThermal Status: 2\nCurrent temperatures from HAL:\n\tTemperature{mValue=41.5, mType=0, mName=cpu0, mStatus=1}\n@@ZONES\nbattery 38800\n@@CPU\ncpu0 900000 2000000\n").unwrap();
    let s = health::sample(&adb, "SER").await.unwrap();
    assert_eq!(s.battery.as_ref().unwrap().temp_c, Some(38.8));
    assert_eq!((s.thermal_status, s.thermal_status_name.as_deref()), (Some(2), Some("moderate")));
    assert_eq!(s.hottest.as_ref().unwrap().name, "cpu0");
    assert!(s.missing.is_empty());

    // A phone that blocks everything still yields a sample, listing what is missing.
    fs::write(d.join("health.txt"), "@@BATT\n@@THERMAL\n@@ZONES\n@@CPU\n").unwrap();
    let blocked = health::sample(&adb, "SER").await.unwrap();
    assert_eq!(blocked.missing.len(), 4);
}

// ---- tuning: apply, verify, undo ----

fn tune_db(dir: &PathBuf, json: &str) { fs::write(dir.join("tunedb.json"), json).unwrap(); }
const EMPTY_TUNE: &str = r#"{"settings": {"global.window_animation_scale": "1.0"}, "appops": {}, "standby": {}, "fps": [60.0, 120.0]}"#;

#[tokio::test]
async fn status_reads_current_values() {
    let (adb, d) = setup("tstatus", false);
    tune_db(&d, EMPTY_TUNE);
    let s = tuning::read_status(&adb, "SER").await.unwrap();
    assert_eq!((s.animation_scale.as_deref(), s.battery_saver, s.max_fps), (Some("1.0"), None, Some(120.0)));
}

#[tokio::test]
async fn animation_change_is_verified_and_undone() {
    let (adb, d) = setup("tanim", false);
    tune_db(&d, EMPTY_TUNE);
    let mut records = Vec::new();
    for step in tuning::animation_steps() {
        records.push(tuning::apply(&adb, "SER", &step.target, step.value, step.group, step.label, 1).await.unwrap());
    }
    assert_eq!(tuning::read_status(&adb, "SER").await.unwrap().animation_scale.as_deref(), Some("0.5"));
    assert_eq!(records[0].prev.as_deref(), Some("1.0"));
    assert_eq!(records[1].prev, None, "a key that was never set remembers that");
    for r in &records { tuning::undo(&adb, "SER", r).await.unwrap(); }
    let after = fs::read_to_string(d.join("tunedb.json")).unwrap();
    assert!(after.contains("\"global.window_animation_scale\": \"1.0\"") || after.contains("\"global.window_animation_scale\":\"1.0\""), "{after}");
    assert!(!after.contains("transition_animation_scale"), "keys that did not exist are deleted again: {after}");
}

#[tokio::test]
async fn ignored_write_is_reported_and_not_left_behind() {
    let (adb, d) = setup("tignore", false);
    tune_db(&d, r#"{"settings": {}, "appops": {}, "standby": {}, "ignore": ["low_power"]}"#);
    let step = &tuning::battery_saver_steps()[0];
    let e = tuning::apply(&adb, "SER", &step.target, step.value, step.group, step.label, 1).await.unwrap_err().to_string();
    assert!(e.contains("ignored the change"), "{e}");
    assert_eq!(tuning::read_status(&adb, "SER").await.unwrap().battery_saver, None);
}

#[tokio::test]
async fn background_restriction_round_trip() {
    let (adb, d) = setup("tbg", false);
    tune_db(&d, EMPTY_TUNE);
    let mut records = Vec::new();
    for (t, v, label) in tuning::background_steps("com.noisy.app") {
        records.push(tuning::apply(&adb, "SER", &t, v, "background", label, 1).await.unwrap());
    }
    let db = fs::read_to_string(d.join("tunedb.json")).unwrap();
    assert!(db.contains("deny") && db.contains("\"45\""), "{db}");
    assert_eq!(records[0].prev.as_deref(), Some("default"));
    assert_eq!(records[1].prev.as_deref(), Some("active"));
    for r in records.iter().rev() { tuning::undo(&adb, "SER", r).await.unwrap(); }
    let db = fs::read_to_string(d.join("tunedb.json")).unwrap();
    assert!(!db.contains("deny") && db.contains("\"10\""), "{db}");
}

#[tokio::test]
async fn unsafe_names_never_reach_the_phone() {
    let (adb, d) = setup("tinject", false);
    tune_db(&d, EMPTY_TUNE);
    let bad = tuning::Target::AppOp { pkg: "com.x; reboot".into(), op: "RUN_ANY_IN_BACKGROUND".into() };
    assert!(tuning::apply(&adb, "SER", &bad, "deny", "g", "l", 1).await.is_err());
    let bad2 = tuning::Target::Setting { ns: "secure".into(), key: "adb_enabled".into() };
    assert!(tuning::apply(&adb, "SER", &bad2, "0", "g", "l", 1).await.is_err());
    tuning::trim_caches(&adb, "SER").await.unwrap();
}
