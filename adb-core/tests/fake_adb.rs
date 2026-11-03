//! Drives the real code paths against a fake `adb` (tests/fixtures/fake_adb.py): no device needed.
#![cfg(unix)]
use adb_core::{mirror, storage, wakelocks, backup, install, protect, bridge, log, report, tuning, deviceinfo, health, labels, packages::{self, PkgState}, processes, snapshot, wireless, AdbRunner};
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
    for f in ["literal.apk", "reference.apk", "entities.apk", "nolabel.apk", "activityref.apk", "activityliteral.apk"] {
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

// ---- device report ----

#[tokio::test]
async fn report_collects_scrubs_and_survives_failing_sections() {
    let (adb, d) = setup("report", false);
    fs::write(d.join("deviceinfo.txt"), "@@PROPS\nSamsung\nSM-G973F\n12\n31\n2023-11-01\n@@DF\nFilesystem 1K-blocks Used Available Use% Mounted on\n/dev/x 100 40 60 40% /data\n@@BATT\n  level: 73\n  serial: R58M123ABC\n").unwrap();
    fs::write(d.join("health.txt"), "@@BATT\n  level: 50\n  temperature: 300\n@@THERMAL\n@@ZONES\n@@CPU\n").unwrap();
    fs::write(d.join("ipaddr.txt"), "22: wlan0: <UP>\n    inet 192.168.1.20/24 scope global wlan0\n    link/ether 3c:5a:b4:01:02:03 owner@example.com\n").unwrap();
    write_db(&d, &[("com.sys.a", true, "enabled"), ("com.user.b", false, "enabled")]);

    let sections = report::collect(&adb, "R58M123ABC").await;
    assert_eq!(sections.len(), report::plan().len() + 2, "every planned section plus the helper check and the adb version");
    let get = |n: &str| sections.iter().find(|s| s.name == n).unwrap_or_else(|| panic!("missing {n}"));

    assert!(get("device info (parser input)").ok && get("device info (parser input)").output.contains("<serial>"));
    assert!(get("health (parser input)").ok && get("processes (parser input)").ok && get("tuning status (parser input)").ok);
    assert!(get("system packages").ok && get("system packages").output.contains("com.sys.a") && !get("system packages").output.contains("com.user.b"));
    assert!(get("wifi address").output.contains("<ip>") && get("wifi address").output.contains("<mac>") && get("wifi address").output.contains("<email>"));
    // The fake phone cannot run shell loops: those sections fail, and the report still has everything else.
    assert!(!get("build info").ok && get("build info").output.starts_with("FAILED"));
    let helper = get("on-device helper (names and icons)");
    assert!(!helper.ok, "the fake phone has no helper output in this test");
    assert!(helper.output.contains("selftest failed") || helper.output.contains("not found") || helper.output.contains("complete: false"), "{}", helper.output);
    let v = get("adb on this computer");
    assert!(v.ok && v.output.contains("Version 34") && !v.output.contains("Installed as"));

    let text = report::render(&report::Meta { app_version: "9.9.9", unix_time: 1 }, &sections);
    for secret in ["R58M123ABC", "192.168.1.20", "3c:5a:b4:01:02:03", "owner@example.com"] {
        assert!(!text.contains(secret), "{secret} leaked into the report");
    }
    assert!(text.contains("=== build info [FAILED] ===") && text.contains("=== health (parser input) [ok] ==="));
}

// ---- on-device helper (bridge) ----

const APPS_OUT: &str = "WARNING: some runtime noise\nACB1 {\"p\":\"com.google.android.gm\",\"n\":\"Gmail\",\"s\":1,\"u\":1,\"e\":1,\"in\":1,\"v\":\"1.0\",\"vc\":7,\"fi\":1,\"lu\":2,\"t\":34,\"i\":\"com.android.vending\",\"l\":1}\nACB1 {\"p\":\"com.vendor.hidden\",\"n\":\"Vendor Hidden Service\",\"s\":1,\"u\":0,\"e\":0,\"in\":0,\"v\":\"\",\"vc\":1,\"fi\":0,\"lu\":0,\"t\":30,\"i\":\"\",\"l\":0}\nACB1 {\"error\":\"com.broken: label: NullPointerException\"}\nACB1 DONE\n";

#[tokio::test]
async fn helper_push_run_and_cleanup() {
    let (adb, d) = setup("helper", false);
    fs::write(d.join("helper_apps.txt"), APPS_OUT).unwrap();
    let remote = bridge::push(&adb, "SER").await.unwrap();
    assert_eq!(fs::read(d.join("pushed.jar")).unwrap(), bridge::HELPER_JAR, "the embedded helper is what gets copied");
    let r = bridge::run(&adb, "SER", &remote, "apps", &[], std::time::Duration::from_secs(20)).await.unwrap();
    assert!(r.parsed.complete);
    let apps: Vec<_> = r.parsed.lines.iter().filter_map(|l| if let bridge::Line::App(a) = l { Some(a) } else { None }).collect();
    assert_eq!(apps.len(), 2);
    assert_eq!((apps[1].label.as_str(), apps[1].installed, apps[1].enabled), ("Vendor Hidden Service", false, false), "apps removed for the user are still named");
    assert_eq!(r.errors(), ["com.broken: label: NullPointerException"], "one broken app does not hide the rest");
    bridge::cleanup(&adb, "SER", &remote).await;
    assert!(fs::read_to_string(d.join("removed")).unwrap().contains("adbclone-helper-"));
}

#[tokio::test]
async fn helper_icons_only_for_requested_packages() {
    let (adb, d) = setup("helper-icons", false);
    fs::write(d.join("helper_icons.txt"), "ACB1 {\"p\":\"a.one\",\"png\":\"AAAA\"}\nACB1 {\"p\":\"b.two\",\"png\":\"BBBB\"}\nACB1 DONE\n").unwrap();
    let r = bridge::run(&adb, "SER", "/data/local/tmp/adbclone-helper-x.jar", "icons", &["b.two".to_string()], std::time::Duration::from_secs(20)).await.unwrap();
    let icons: Vec<_> = r.parsed.lines.iter().filter_map(|l| if let bridge::Line::Icon { package, .. } = l { Some(package.clone()) } else { None }).collect();
    assert_eq!(icons, ["b.two"]);
}

#[tokio::test]
async fn helper_failure_is_explained_not_fatal() {
    let (adb, _d) = setup("helper-fail", false);
    // No canned output: the fake phone says app_process is missing, like an unusual ROM would.
    let r = bridge::run(&adb, "SER", "/data/local/tmp/adbclone-helper-x.jar", "apps", &[], std::time::Duration::from_secs(20)).await.unwrap();
    assert!(r.parsed.lines.is_empty() && !r.parsed.complete);
    assert!(r.failure_reason().contains("not found"), "{}", r.failure_reason());
}

#[tokio::test]
async fn helper_rejects_unsafe_input() {
    let (adb, _d) = setup("helper-bad", false);
    let t = std::time::Duration::from_secs(5);
    assert!(bridge::run(&adb, "SER", "/x.jar", "apps; reboot", &[], t).await.is_err());
    assert!(bridge::run(&adb, "SER", "/x.jar", "icons", &["a.b; reboot".to_string()], t).await.is_err());
}

// ---- name resolution reasons ----

#[tokio::test]
async fn resolve_uses_launcher_name_and_explains_misses() {
    let (adb, _d) = setup("resolve", false);
    let r = labels::resolve(&adb, "SER", "/system/app/activityref.apk").await;
    assert_eq!(r.label.as_deref(), Some("Launcher Label"));
    assert_eq!(r.reason, None);

    let none = labels::resolve(&adb, "SER", "/system/app/nolabel.apk").await;
    assert_eq!(none.label, None);
    assert_eq!(none.reason.as_deref(), Some("the app declares no name"), "readable files, no name: do not waste a full pull");

    let missing = labels::resolve(&adb, "SER", "/system/app/gone.apk").await;
    assert!(missing.label.is_none() && missing.reason.as_deref().unwrap().contains("copying the file failed"), "{missing:?}");

    let odd = labels::resolve(&adb, "SER", "/system/app/we ird;name.apk").await;
    assert!(odd.reason.as_deref().unwrap().contains("unusual characters"));
}

#[tokio::test]
async fn resolve_falls_back_to_full_pull_for_launcher_names_too() {
    let (adb, _d) = setup("resolve-pull", true); // phone without unzip
    let r = labels::resolve(&adb, "SER", "/system/app/activityliteral.apk").await;
    assert_eq!(r.label.as_deref(), Some("Literal Launcher"));
}

#[tokio::test]
async fn report_describes_a_working_helper_without_app_names() {
    let (adb, d) = setup("report-helper", false);
    fs::write(d.join("helper_selftest.txt"), "ACB1 {\"sdk\":34,\"release\":\"14\",\"uid\":2000,\"context\":true,\"packages\":2,\"problems\":[]}\nACB1 DONE\n").unwrap();
    fs::write(d.join("helper_apps.txt"), format!("{}", APPS_OUT)).unwrap();
    let sections = report::collect(&adb, "R58M123ABC").await;
    let h = sections.iter().find(|s| s.name.starts_with("on-device helper")).unwrap();
    assert!(h.ok, "{}", h.output);
    assert!(h.output.contains("\"sdk\":34") && h.output.contains("apps listed: 2, with a real name: 2"), "{}", h.output);
    assert!(h.output.contains("error kinds: label: NullPointerException") || h.output.contains("NullPointerException"));
    assert!(!h.output.contains("Gmail") && !h.output.contains("com.google.android.gm") && !h.output.contains("com.broken"), "no app names or ids in the helper section: {}", h.output);
}

// ---- command log ----

#[tokio::test]
async fn every_command_is_logged_and_pairing_codes_never_are() {
    let (adb, _d) = setup("logging", false);
    let (_, start) = log::since(0);
    let _ = wireless::pair(&adb, "192.168.77.1:40000", "424242").await;
    let _ = adb.shell("LOGSER123", &["echo hello-log-test"]).await;
    let (entries, _) = log::since(start);
    assert!(entries.iter().any(|e| e.command.contains("-s LOGSER123 shell echo hello-log-test")), "{entries:?}");
    let pair = entries.iter().find(|e| e.command.contains("pair 192.168.77.1:40000")).expect("the pair command is listed");
    assert!(pair.command.ends_with("******"), "{}", pair.command);
    // No test in this file may leak a code into the shared log.
    let (all, _) = log::since(0);
    for code in ["424242", "123456", "000000", "999999"] {
        assert!(!all.iter().any(|e| e.command.contains(&format!(" {code}"))), "{code} leaked into the log");
    }
}

#[tokio::test]
async fn protections_come_from_the_phone() {
    let (adb, d) = setup("protect", false);
    fs::write(d.join("protect.txt"), "@@HOME\ncom.sec.android.app.launcher/.activities.LauncherActivity\n@@HOMEALL\ncom.sec.android.app.launcher/.activities.LauncherActivity\n@@IME\ncom.samsung.android.honeyboard/.service.HoneyBoardService\n@@ROLES\nandroid.app.role.SMS com.samsung.android.messaging\n").unwrap();
    let p = protect::detect(&adb, "SER").await.unwrap();
    let pkgs: Vec<_> = p.iter().map(|x| x.package.as_str()).collect();
    assert!(pkgs.contains(&"com.sec.android.app.launcher") && pkgs.contains(&"com.samsung.android.honeyboard") && pkgs.contains(&"com.samsung.android.messaging"), "{p:?}");
}

// ---- install tool and app backup ----

fn write_db_paths(dir: &PathBuf, json: &str) { fs::write(dir.join("pkgdb.json"), json).unwrap(); }

#[tokio::test]
async fn backup_copies_every_part_of_a_split_app_and_describes_it() {
    let (adb, d) = setup("backup", false);
    write_db_paths(&d, r#"{"com.test.split": {"system": false, "state": "enabled", "paths": ["/data/app/~~a==/com.test.split-b==/reference.apk", "/data/app/~~a==/com.test.split-b==/literal.apk"]}, "com.test.gone": {"system": false, "state": "uninstalled"}}"#);
    let dest = std::env::temp_dir().join(format!("coolant-backup-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dest);
    let info = backup::ExportInfo { label: Some("Split App".into()), version_name: Some("1.2".into()), installer: Some("com.android.vending".into()) };
    let r = backup::export(&adb, "SER", "com.test.split", &dest, &info).await.unwrap();
    assert_eq!(r.files, ["reference.apk", "literal.apk"]);
    assert!(r.bytes > 1000 && dest.join("com.test.split/reference.apk").is_file());
    let meta: serde_json::Value = serde_json::from_str(&fs::read_to_string(dest.join("com.test.split/info.json")).unwrap()).unwrap();
    assert_eq!((meta["package"].as_str(), meta["splitApp"].as_bool(), meta["label"].as_str()), (Some("com.test.split"), Some(true), Some("Split App")));
    // An app that is removed for this user cannot be copied, and says why.
    let e = backup::export(&adb, "SER", "com.test.gone", &dest, &backup::ExportInfo::default()).await.unwrap_err();
    assert!(e.contains("not installed for you"), "{e}");
    assert!(backup::export(&adb, "SER", "bad name; reboot", &dest, &backup::ExportInfo::default()).await.is_err());
    // The saved folder can be dropped straight into the install tool.
    let items = install::inspect(&[dest.join("com.test.split")]);
    assert_eq!(items[0].as_ref().unwrap().file_count, 2);
}

#[tokio::test]
async fn installing_single_and_split_apps_with_the_right_flags_and_clear_errors() {
    let (adb, d) = setup("install", false);
    let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let opts = install::Options { replace: true, allow_downgrade: false, allow_test: false };
    let single = install::inspect(&[fx.join("literal.apk")]).remove(0).unwrap();
    assert_eq!(install::install(&adb, "SER", &single, opts).await.unwrap(), "Literal App installed.");
    let multi = install::inspect(&[fx.join("reference.apk"), fx.join("entities.apk")]); // different apps: two items
    assert_eq!(multi.len(), 2);
    let log = fs::read_to_string(d.join("installed.log")).unwrap();
    assert!(log.contains("install -r :: literal.apk"), "{log}");

    // Several files of one app go through install-multiple.
    let folder = std::env::temp_dir().join(format!("coolant-multi-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder); fs::create_dir_all(&folder).unwrap();
    fs::copy(fx.join("reference.apk"), folder.join("base.apk")).unwrap();
    fs::copy(fx.join("literal.apk"), folder.join("split_config.en.apk")).unwrap();
    let item = install::inspect(&[folder]).remove(0).unwrap();
    install::install(&adb, "SER", &item, install::Options { replace: true, allow_downgrade: true, allow_test: true }).await.unwrap();
    assert!(fs::read_to_string(d.join("installed.log")).unwrap().contains("install-multiple -r -d -t :: base.apk split_config.en.apk"));

    // Without "replace", an already-installed app is refused with advice that points at the checkbox.
    let exists = folder_with(&fx, "exists.apk", "literal.apk");
    let item = install::inspect(&[exists.clone()]).remove(0).unwrap();
    let e = install::install(&adb, "SER", &item, install::Options::default()).await.unwrap_err();
    assert!(e.contains("Replace if already installed"), "{e}");
    assert!(install::install(&adb, "SER", &item, opts).await.is_ok());
    // A file built for another processor says so.
    let abi = folder_with(&fx, "abi-mismatch.apk", "literal.apk");
    let item = install::inspect(&[abi]).remove(0).unwrap();
    assert!(install::install(&adb, "SER", &item, opts).await.unwrap_err().contains("processor"));
    // Files that disappeared are reported, not installed.
    fs::remove_file(&exists).unwrap();
    assert!(install::install(&adb, "SER", &item_without_files(), opts).await.is_err());
}

fn folder_with(fx: &PathBuf, name: &str, from: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("coolant-{}-{name}", std::process::id()));
    fs::copy(fx.join(from), &p).unwrap();
    p
}
fn item_without_files() -> install::InstallItem {
    let missing = std::env::temp_dir().join("coolant-missing.apk");
    install::inspect(&[missing]).into_iter().find_map(|r| r.ok()).unwrap_or_else(|| {
        // inspect() refuses a missing file's details but still lists it: build the same shape by hand.
        let dir = std::env::temp_dir().join(format!("coolant-gone-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("gone.apk");
        fs::copy(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/literal.apk"), &f).unwrap();
        let it = install::inspect(&[f.clone()]).remove(0).unwrap();
        fs::remove_file(&f).unwrap();
        it
    })
}

#[tokio::test]
async fn storage_and_last_opened_come_from_the_phone() {
    let (adb, d) = setup("storage", false);
    fs::write(d.join("diskstats.txt"), "Package Names: [\"com.big.game\",\"com.tiny\"]\nApp Sizes: [900000000, 1000]\nApp Data Sizes: [100000000, 10]\nCache Sizes: [50000000, 1]\n").unwrap();
    fs::write(d.join("usage.txt"), "@@NOW\n2024-06-30 12:00:00\n@@USAGE\n    package=com.big.game totalTimeUsed=\"+1h\" lastTimeUsed=\"2024-03-01 10:00:00\" appLaunchCount=9\n").unwrap();
    let sizes = storage::read_sizes(&adb, "SER").await.unwrap();
    assert_eq!((sizes[0].package.as_str(), sizes[0].cache_bytes), ("com.big.game", 50_000_000));
    let usage = storage::read_usage(&adb, "SER").await.unwrap();
    assert_eq!((usage[0].package.as_str(), usage[0].days_ago, usage[0].launches), ("com.big.game", 121, Some(9)));
    // A phone that prints nothing useful gives empty lists, not an error.
    fs::write(d.join("usage.txt"), "@@NOW\n2024-06-30 12:00:00\n@@USAGE\nSecurityException: denied\n").unwrap();
    assert!(storage::read_usage(&adb, "SER").await.unwrap().is_empty());
}

#[tokio::test]
async fn wake_locks_and_wakeups_come_from_the_phone() {
    let (adb, d) = setup("wake", false);
    fs::write(d.join("wake.txt"), "@@POWER\n  PARTIAL_WAKE_LOCK 'Sync' ACQ=-2m (uid=10123 pid=1)\n@@ALARM\n  u0a1:com.chat +1s running, 0ms active, 7 wakeups, 9 alarms:\n").unwrap();
    let w = wakelocks::read(&adb, "SER").await.unwrap();
    assert_eq!((w.locks[0].uid, w.locks[0].held_secs), (10123, Some(120)));
    assert_eq!((w.wakeups[0].package.as_str(), w.wakeups[0].wakeups), ("com.chat", 7));
    fs::write(d.join("wake.txt"), "@@POWER\nSecurityException\n@@ALARM\n").unwrap();
    let w = wakelocks::read(&adb, "SER").await.unwrap();
    assert!(w.locks.is_empty() && w.wakeups.is_empty());
}

#[tokio::test]
async fn screenshot_returns_a_png_or_a_clear_error() {
    let (adb, d) = setup("shot", false);
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend(std::iter::repeat(7u8).take(300));
    fs::write(d.join("screen.png"), &png).unwrap();
    assert_eq!(mirror::screenshot(&adb, "SER").await.unwrap(), png);
    fs::remove_file(d.join("screen.png")).unwrap();
    let e = mirror::screenshot(&adb, "SER").await.unwrap_err().to_string();
    assert!(e.contains("did not return a picture"), "{e}");
}
