//! What is keeping the phone awake right now (held wake locks) and which apps wake it up most
//! (alarm statistics). Both are text from `dumpsys` whose layout differs between Android versions and
//! makers, so the parsers only take lines they fully understand and an empty result means "not available".
use crate::deviceinfo::section;
use crate::{AdbError, AdbRunner};
use serde::Serialize;

pub const CMD: &str = "echo @@POWER; dumpsys power; echo @@ALARM; dumpsys alarm";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeLock {
    pub uid: u32,
    pub kind: String,
    pub tag: String,
    /// How long it has been held, when the phone says.
    pub held_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Wakeups {
    pub package: String,
    pub wakeups: u64,
    pub alarms: u64,
}

/// "-1h2m3s450ms", "+5s", "-250ms" -> whole seconds. `None` if it is not a duration.
pub fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim().trim_start_matches(['+', '-']);
    if s.is_empty() {
        return None;
    }
    let (mut total_ms, mut num, mut seen) = (0f64, String::new(), false);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let n: f64 = num.parse().ok()?;
        num.clear();
        let unit = match c {
            'd' => 86_400_000.0,
            'h' => 3_600_000.0,
            's' => 1000.0,
            'm' if chars.peek() == Some(&'s') => {
                chars.next();
                1.0
            }
            'm' => 60_000.0,
            _ => return None,
        };
        total_ms += n * unit;
        seen = true;
    }
    (seen && num.is_empty()).then(|| (total_ms / 1000.0) as u64)
}

fn uid_after(s: &str, key: &str) -> Option<u32> {
    let rest = &s[s.find(key)? + key.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Held wake locks from `dumpsys power`. A lock held on behalf of an app (its WorkSource) is credited to that app.
pub fn parse_wake_locks(text: &str) -> Vec<WakeLock> {
    let mut out = Vec::new();
    for l in text.lines() {
        let t = l.trim();
        let kind = t.split_whitespace().next().unwrap_or("");
        if !kind.ends_with("_WAKE_LOCK") && kind != "WAKE_LOCK" {
            continue;
        }
        let Some(q1) = t.find('\'') else { continue };
        let Some(q2) = t[q1 + 1..].find('\'') else { continue };
        let tag: String = t[q1 + 1..q1 + 1 + q2].chars().take(80).collect();
        let tail = &t[q1 + q2 + 2..];
        let uid = tail.find("WorkSource{").and_then(|i| uid_after(&tail[i..], "WorkSource{")).or_else(|| uid_after(tail, "uid="));
        let Some(uid) = uid else { continue };
        let held_secs = tail.find("ACQ=").and_then(|i| tail[i + 4..].split_whitespace().next()).and_then(parse_duration);
        out.push(WakeLock { uid, kind: kind.to_string(), tag, held_secs });
    }
    out
}

/// Per-app wake-up counts from `dumpsys alarm`, e.g.
/// `u0a45:com.google.android.gms +1h2m running, 0ms active, 400 wakeups, 1000 alarms:`.
pub fn parse_wakeups(text: &str) -> Vec<Wakeups> {
    let mut out: Vec<Wakeups> = Vec::new();
    for l in text.lines() {
        let t = l.trim();
        if !t.contains("wakeups") {
            continue;
        }
        let Some(first) = t.split_whitespace().next() else { continue };
        let Some((_, pkg)) = first.split_once(':') else { continue };
        if !crate::packages::valid_name(pkg) {
            continue;
        }
        let count = |word: &str| -> Option<u64> {
            t.split(',').find_map(|seg| {
                let seg = seg.trim().trim_end_matches(':');
                let (n, w) = seg.rsplit_once(' ')?;
                (w == word).then(|| n.rsplit(' ').next().and_then(|x| x.parse().ok())).flatten()
            })
        };
        let Some(wakeups) = count("wakeups") else { continue };
        if wakeups == 0 {
            continue;
        }
        match out.iter_mut().find(|w| w.package == pkg) {
            Some(w) => {
                w.wakeups += wakeups;
                w.alarms += count("alarms").unwrap_or(0);
            }
            None => out.push(Wakeups { package: pkg.to_string(), wakeups, alarms: count("alarms").unwrap_or(0) }),
        }
    }
    out.sort_by_key(|w| std::cmp::Reverse(w.wakeups));
    out.truncate(40);
    out
}

pub struct WakeData {
    pub locks: Vec<WakeLock>,
    pub wakeups: Vec<Wakeups>,
}

pub fn parse(text: &str) -> WakeData {
    WakeData { locks: parse_wake_locks(section(text, "POWER")), wakeups: parse_wakeups(section(text, "ALARM")) }
}

pub async fn read(adb: &AdbRunner, serial: &str) -> Result<WakeData, AdbError> {
    Ok(parse(&adb.shell(serial, &[CMD]).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("-1h2m3s450ms"), Some(3723));
        assert_eq!(parse_duration("+5s"), Some(5));
        assert_eq!(parse_duration("-250ms"), Some(0));
        assert_eq!(parse_duration("-2m"), Some(120));
        assert_eq!(parse_duration("+3d"), Some(259_200));
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("12"), None);
    }

    const POWER: &str = "Wake Locks: size=3\n  PARTIAL_WAKE_LOCK              'RILJ_ACK_WL' ACQ=-1m30s (uid=1001 pid=1234 ws=WorkSource{1001})\n  PARTIAL_WAKE_LOCK              'GCM_CONN_ALARM' ACQ=-5s123ms (uid=1000 pid=99 ws=WorkSource{10123 com.chat})\n  SCREEN_BRIGHT_WAKE_LOCK        'Display' ACQ=-10s (uid=10200 pid=5)\n  not a lock 'x' uid=5\n";

    #[test]
    fn held_locks_are_credited_to_the_work_source() {
        let l = parse_wake_locks(POWER);
        assert_eq!(l.len(), 3);
        assert_eq!((l[0].uid, l[0].tag.as_str(), l[0].held_secs), (1001, "RILJ_ACK_WL", Some(90)));
        assert_eq!(l[1].uid, 10123, "the app it works for, not the system uid");
        assert_eq!((l[2].uid, l[2].kind.as_str()), (10200, "SCREEN_BRIGHT_WAKE_LOCK"));
    }

    #[test]
    fn locks_with_no_owner_are_skipped() {
        assert!(parse_wake_locks("  PARTIAL_WAKE_LOCK 'x' ACQ=-1s").is_empty());
        assert!(parse_wake_locks("").is_empty());
    }

    const ALARM: &str = "  Alarm Stats:\n  u0a45:com.google.android.gms +1h2m3s456ms running, 0ms active, 400 wakeups, 1000 alarms:\n    +10s ELAPSED_WAKEUP\n  u0a99:com.chat +5s running, 0ms active, 12 wakeups, 30 alarms:\n  u0a7:com.quiet +0ms running, 0ms active, 0 wakeups, 3 alarms:\n  1000:android +1s running, 0ms active, 5 wakeups, 9 alarms:\n  weird line wakeups\n";

    #[test]
    fn wakeups_sorted_and_zero_ignored() {
        let w = parse_wakeups(ALARM);
        assert_eq!(w.iter().map(|x| x.package.as_str()).collect::<Vec<_>>(), ["com.google.android.gms", "com.chat", "android"]);
        assert_eq!((w[0].wakeups, w[0].alarms), (400, 1000));
    }

    #[test]
    fn both_sections_parse_from_one_round_trip() {
        let d = parse(&format!("@@POWER\n{POWER}@@ALARM\n{ALARM}"));
        assert_eq!((d.locks.len(), d.wakeups.len()), (3, 3));
        let empty = parse("@@POWER\nPermission denied\n@@ALARM\nnope\n");
        assert!(empty.locks.is_empty() && empty.wakeups.is_empty());
    }
}
