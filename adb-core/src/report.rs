//! "Save device report": the raw output of everything the app reads, so parsers can be fixed from a real phone.
//! Read-only; personal details are scrubbed before the text is shown.
use crate::{bridge, deviceinfo, health, processes, tuning, AdbRunner};
use regex::{Captures, Regex};
use serde::Serialize;
use std::{sync::OnceLock, time::Duration};

const MAX_SECTION: usize = 40_000;

#[derive(Debug, Clone, Serialize)]
pub struct Section {
    pub name: String,
    pub command: String,
    pub ok: bool,
    pub output: String,
}

/// Fixed list of (section name, shell command). Nothing here is built from user input.
pub fn plan() -> Vec<(&'static str, String)> {
    vec![
        ("build info", "for p in ro.product.brand ro.product.manufacturer ro.product.model ro.product.device ro.build.fingerprint ro.build.version.release ro.build.version.sdk ro.build.version.security_patch ro.product.cpu.abi ro.hardware ro.board.platform; do echo \"$p=$(getprop $p)\"; done".into()),
        ("device info (parser input)", deviceinfo::CMD.into()),
        ("health (parser input)", health::CMD.into()),
        ("processes (parser input)", processes::SNAPSHOT_CMD.into()),
        ("tuning status (parser input)", tuning::STATUS_CMD.into()),
        ("system packages", "pm list packages -f -U --user 0 -s | head -n 150".into()),
        ("package counts", "echo user=$(pm list packages -3 | wc -l); echo disabled=$(pm list packages -d | wc -l); echo all_including_uninstalled=$(pm list packages -u | wc -l); echo installed=$(pm list packages | wc -l)".into()),
        ("tool availability", "for t in unzip ps top stat df settings cmd dumpsys; do echo \"$t: $(command -v $t || echo missing)\"; done".into()),
        ("app ops and standby sample", "cmd appops get com.android.shell RUN_ANY_IN_BACKGROUND; am get-standby-bucket com.android.shell".into()),
        ("storage report format (keys only)", "dumpsys diskstats | cut -d: -f1 | cut -c1-60 | head -n 40".into()),
        ("last-opened format (names hidden)", "dumpsys usagestats | grep -m6 'package=' | sed 's/package=[^ ]*/package=<name>/' | cut -c1-400".into()),
        ("wifi address", "ip -f inet addr show wlan0".into()),
    ]
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

/// Replace details that identify a person or a network with placeholders.
pub fn scrub(text: &str, serial: &str) -> String {
    static EMAIL: OnceLock<Regex> = OnceLock::new();
    static MAC: OnceLock<Regex> = OnceLock::new();
    static IP: OnceLock<Regex> = OnceLock::new();
    static LONG: OnceLock<Regex> = OnceLock::new();
    let mut t = text.to_string();
    if serial.len() >= 4 {
        t = t.replace(serial, "<serial>");
    }
    t = re(&EMAIL, r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").replace_all(&t, "<email>").into_owned();
    t = re(&MAC, r"\b(?:[0-9A-Fa-f]{2}[:\-]){5}[0-9A-Fa-f]{2}\b").replace_all(&t, "<mac>").into_owned();
    t = re(&IP, r"\b(?:\d{1,3}\.){3}\d{1,3}\b")
        .replace_all(&t, |c: &Captures| if &c[0] == "127.0.0.1" || &c[0] == "0.0.0.0" { c[0].to_string() } else { "<ip>".to_string() })
        .into_owned();
    // IMEI-like identifiers.
    re(&LONG, r"\b\d{14,16}\b").replace_all(&t, "<number>").into_owned()
}

/// Cut to `max` bytes on a character boundary, saying how much was dropped.
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[... truncated {} more bytes ...]", &s[..end], s.len() - end)
}

/// Run every section. A failing section is recorded with its error; it never aborts the report.
pub async fn collect(adb: &AdbRunner, serial: &str) -> Vec<Section> {
    let adb = adb.clone().with_timeout(Duration::from_secs(20));
    let mut out = Vec::new();
    for (name, command) in plan() {
        let (ok, text) = match adb.shell(serial, &[&command]).await {
            Ok(t) => (true, t),
            Err(e) => (false, format!("FAILED: {e}")),
        };
        out.push(Section { name: name.to_string(), command, ok, output: truncate(&scrub(&text, serial), MAX_SECTION) });
    }
    out.push(helper_section(&adb, serial).await);
    // Host side: which adb the app is using (the install path line is dropped, it can contain a user name).
    let (ok, text) = match adb.run(&["version"]).await {
        Ok(t) => (true, t.lines().filter(|l| !l.starts_with("Installed as")).collect::<Vec<_>>().join("\n")),
        Err(e) => (false, format!("FAILED: {e}")),
    };
    out.push(Section { name: "adb on this computer".into(), command: "adb version".into(), ok, output: scrub(&text, serial) });
    out
}

/// Tries the on-device helper and describes what happened, without app names (counts and error kinds only).
async fn helper_section(adb: &AdbRunner, serial: &str) -> Section {
    let name = "on-device helper (names and icons)".to_string();
    let command = "push helper, run: selftest, apps".to_string();
    let remote = match bridge::push(adb, serial).await {
        Ok(r) => r,
        Err(e) => return Section { name, command, ok: false, output: scrub(&format!("could not start the helper: {e}"), serial) },
    };
    let mut text = String::new();
    let mut ok = true;
    match bridge::run(adb, serial, &remote, "selftest", &[], Duration::from_secs(30)).await {
        Ok(r) => {
            for l in &r.parsed.lines {
                match l {
                    bridge::Line::Selftest(v) => text.push_str(&format!("selftest: {v}\n")),
                    bridge::Line::Error(e) => text.push_str(&format!("selftest error: {e}\n")),
                    _ => {}
                }
            }
            text.push_str(&format!("selftest complete: {}, exit code: {:?}\n", r.parsed.complete, r.exit_code));
            if !r.stderr.trim().is_empty() {
                text.push_str(&format!("selftest stderr: {}\n", r.stderr.trim()));
            }
            ok &= r.parsed.complete;
        }
        Err(e) => {
            text.push_str(&format!("selftest failed: {e}\n"));
            ok = false;
        }
    }
    match bridge::run(adb, serial, &remote, "apps", &[], Duration::from_secs(90)).await {
        Ok(r) => {
            let apps = r.parsed.lines.iter().filter(|l| matches!(l, bridge::Line::App(_))).count();
            let named = r.parsed.lines.iter().filter(|l| matches!(l, bridge::Line::App(a) if !a.label.is_empty() && a.label != a.package)).count();
            let mut kinds: Vec<String> = r.errors().iter().map(|e| e.split_once(": ").map_or(e.clone(), |(_, rest)| rest.to_string())).collect();
            kinds.sort();
            kinds.dedup();
            text.push_str(&format!("apps listed: {apps}, with a real name: {named}, complete: {}\n", r.parsed.complete));
            if !kinds.is_empty() {
                text.push_str(&format!("error kinds: {}\n", kinds.into_iter().take(10).collect::<Vec<_>>().join(" | ")));
            }
            if !r.stderr.trim().is_empty() {
                text.push_str(&format!("apps stderr: {}\n", r.stderr.trim()));
            }
            ok &= apps > 0;
        }
        Err(e) => {
            text.push_str(&format!("apps run failed: {e}\n"));
            ok = false;
        }
    }
    bridge::cleanup(adb, serial, &remote).await;
    Section { name, command, ok, output: truncate(&scrub(&text, serial), MAX_SECTION) }
}

pub struct Meta<'a> {
    pub app_version: &'a str,
    pub unix_time: u64,
}

pub fn render(meta: &Meta, sections: &[Section]) -> String {
    let mut s = format!(
        "adbclone device report\napp version: {}\ncreated (unix time): {}\n\
This report only reads information from the phone; nothing was changed.\n\
Serial numbers, e-mail addresses, IP/MAC addresses and long ID numbers were replaced with placeholders.\n\
Read it before sharing.\n",
        meta.app_version, meta.unix_time
    );
    for sec in sections {
        s.push_str(&format!("\n=== {} [{}] ===\n$ {}\n{}\n", sec.name, if sec.ok { "ok" } else { "FAILED" }, sec.command, sec.output.trim_end()));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_identifying_details() {
        let t = "serial R58M123ABC on 192.168.1.20 mac 3C:5A:B4:01:02:03 mail me@example.com imei 356938035643809 ok 127.0.0.1";
        let s = scrub(t, "R58M123ABC");
        assert_eq!(s, "serial <serial> on <ip> mac <mac> mail <email> imei <number> ok 127.0.0.1");
    }

    #[test]
    fn leaves_ordinary_text_alone() {
        let t = "level: 81\ntemperature: 301\nro.build.fingerprint=google/panther/panther:14/UP1A.231005.007/10754064:user/release-keys\nVersion 34.0.4-10411341\n1700000000 timestamp 1234567890123";
        assert_eq!(scrub(t, "AB"), t, "short serials are not replaced, and ids under 14 digits stay");
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("short", 100), "short");
        let t = truncate("ééééé", 3); // 'é' is 2 bytes; 3 is mid-character
        assert!(t.starts_with('é') && t.contains("truncated"), "{t}");
    }

    #[test]
    fn plan_covers_the_parser_inputs_and_is_static() {
        let p = plan();
        let find = |n: &str| p.iter().find(|(name, _)| *name == n).map(|(_, c)| c.clone()).unwrap();
        assert_eq!(find("health (parser input)"), health::CMD);
        assert_eq!(find("processes (parser input)"), processes::SNAPSHOT_CMD);
        assert!(p.iter().all(|(_, c)| !c.contains("settings put") && !c.contains("pm disable") && !c.contains("pm uninstall")), "read-only commands only");
    }

    #[test]
    fn render_marks_failures() {
        let secs = vec![
            Section { name: "a".into(), command: "echo a".into(), ok: true, output: "A\n".into() },
            Section { name: "b".into(), command: "bad".into(), ok: false, output: "FAILED: nope".into() },
        ];
        let r = render(&Meta { app_version: "0.1.0", unix_time: 5 }, &secs);
        assert!(r.contains("=== a [ok] ===") && r.contains("=== b [FAILED] ===") && r.contains("app version: 0.1.0"));
    }
}
