//! Which apps use the most space (app, data, cache), and when each app was last opened.
//! Both come from text the phone prints, in formats that vary by Android version, so the parsers are
//! forgiving and report "not available" rather than guess.
use crate::deviceinfo::section;
use crate::{AdbError, AdbRunner};
use serde::Serialize;

// ---------------------------------------------------------------- sizes

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStorage {
    pub package: String,
    pub app_bytes: u64,
    pub data_bytes: u64,
    pub cache_bytes: u64,
}

/// A JSON-like list on one line, e.g. `["a","b"]` or `[1, 2, 3]`.
fn list_after<'a>(text: &'a str, keys: &[&str]) -> Option<&'a str> {
    text.lines().find_map(|l| {
        let t = l.trim();
        let lower = t.to_ascii_lowercase();
        keys.iter().find(|k| lower.starts_with(*k)).map(|k| t[k.len()..].trim())
    })
}

fn numbers(s: &str) -> Vec<u64> {
    serde_json::from_str::<Vec<serde_json::Value>>(s).map(|v| v.iter().map(|x| x.as_u64().unwrap_or(0)).collect()).unwrap_or_default()
}

/// `dumpsys diskstats` prints `Package Names: [...]` and parallel lists of app, data and cache sizes in bytes.
pub fn parse_diskstats(text: &str) -> Vec<AppStorage> {
    let names: Vec<String> = list_after(text, &["package names:"]).and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
    let app = numbers(list_after(text, &["app sizes:", "package app sizes:"]).unwrap_or(""));
    let data = numbers(list_after(text, &["app data sizes:", "package app data sizes:"]).unwrap_or(""));
    let cache = numbers(list_after(text, &["cache sizes:", "package cache sizes:", "app cache sizes:"]).unwrap_or(""));
    let mut out: Vec<AppStorage> = names
        .into_iter()
        .enumerate()
        .filter(|(_, n)| crate::packages::valid_name(n))
        .map(|(i, package)| AppStorage {
            package,
            app_bytes: app.get(i).copied().unwrap_or(0),
            data_bytes: data.get(i).copied().unwrap_or(0),
            cache_bytes: cache.get(i).copied().unwrap_or(0),
        })
        .filter(|a| a.app_bytes + a.data_bytes + a.cache_bytes > 0)
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.app_bytes + a.data_bytes + a.cache_bytes));
    out
}

pub async fn read_sizes(adb: &AdbRunner, serial: &str) -> Result<Vec<AppStorage>, AdbError> {
    Ok(parse_diskstats(&adb.shell(serial, &["dumpsys diskstats"]).await?))
}

// ---------------------------------------------------------------- last opened

pub const USAGE_CMD: &str = "echo @@NOW; date \"+%Y-%m-%d %H:%M:%S\"; echo @@USAGE; dumpsys usagestats";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub package: String,
    /// `YYYY-MM-DD HH:MM:SS` in the phone's own clock.
    pub last_used: String,
    pub days_ago: i64,
    pub launches: Option<u32>,
}

/// Days from 1970-01-01 for a civil date (Howard Hinnant's algorithm), valid for any date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Parse `YYYY-MM-DD HH:MM:SS` into seconds since the epoch of the same clock (no time zone).
fn naive_seconds(s: &str) -> Option<i64> {
    let (date, time) = s.trim().split_once(' ')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, mo, da) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|x| x.parse::<i64>().ok());
    let (h, mi, se) = (t.next()??, t.next()??, t.next()??);
    ((1970..=2200).contains(&y) && (1..=12).contains(&mo) && (1..=31).contains(&da) && h < 24 && mi < 60 && se < 61)
        .then(|| days_from_civil(y, mo, da) * 86400 + h * 3600 + mi * 60 + se)
}

/// Whole days between two phone-clock timestamps, never negative.
pub fn days_between(earlier: &str, later: &str) -> Option<i64> {
    Some(((naive_seconds(later)? - naive_seconds(earlier)?) / 86400).max(0))
}

/// The value of `key=` on a line: quoted ("...") or bare until a space.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let i = line.find(&format!("{key}="))? + key.len() + 1;
    let rest = &line[i..];
    if let Some(r) = rest.strip_prefix('"') { r.split('"').next() } else { rest.split_whitespace().next() }
}

fn epoch_to_naive(ms: i64) -> String {
    let secs = ms / 1000;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil_from_days
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// The newest "last used" per package from `dumpsys usagestats`. Handles the date-text format of Android 9 to 14
/// and the epoch-millisecond variant. `now` is the phone's clock from the same moment.
pub fn parse_usage(text: &str) -> Vec<Usage> {
    let now = section(text, "NOW").trim().to_string();
    let body = section(text, "USAGE");
    let mut best: std::collections::HashMap<String, (String, Option<u32>)> = std::collections::HashMap::new();
    for line in body.lines() {
        let l = line.trim();
        if !l.starts_with("package=") {
            continue;
        }
        let Some(pkg) = field(l, "package") else { continue };
        if !crate::packages::valid_name(pkg) {
            continue;
        }
        // Several fields can say "last used"; the latest one counts.
        let mut latest: Option<String> = None;
        for key in ["lastTimeUsed", "lastTimeVisible", "lastTimeForegroundServiceUsed", "lastTime"] {
            let Some(v) = field(l, key) else { continue };
            let when = if v.chars().all(|c| c.is_ascii_digit()) && v.len() >= 12 {
                v.parse::<i64>().ok().filter(|ms| *ms > 0).map(epoch_to_naive)
            } else {
                // Date text is split into two words by the space; re-read the quoted value in full.
                let full = l.find(&format!("{key}=\"")).map(|i| &l[i + key.len() + 2..]).and_then(|r| r.split('"').next()).unwrap_or(v);
                naive_seconds(full).map(|_| full.to_string())
            };
            if let Some(w) = when {
                if latest.as_ref().map_or(true, |cur| w.as_str() > cur.as_str()) {
                    latest = Some(w);
                }
            }
        }
        let launches = field(l, "appLaunchCount").and_then(|v| v.parse::<u32>().ok());
        if let Some(w) = latest {
            let e = best.entry(pkg.to_string()).or_insert((w.clone(), launches));
            if w.as_str() > e.0.as_str() {
                e.0 = w;
            }
            e.1 = e.1.max(launches);
        }
    }
    let mut out: Vec<Usage> = best
        .into_iter()
        .filter_map(|(package, (last_used, launches))| {
            // Dates in the far future or before 2000 mean the phone had no real record.
            let days_ago = days_between(&last_used, &now)?;
            (days_ago < 366 * 30 && last_used.as_str() > "2000").then_some(Usage { package, last_used, days_ago, launches })
        })
        .collect();
    out.sort_by(|a, b| a.days_ago.cmp(&b.days_ago).then(a.package.cmp(&b.package)));
    out
}

pub async fn read_usage(adb: &AdbRunner, serial: &str) -> Result<Vec<Usage>, AdbError> {
    Ok(parse_usage(&adb.shell(serial, &[USAGE_CMD]).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_sizes_are_matched_by_position_and_ordered_by_total() {
        let t = "Latency: 5ms\nApp Size: 123\nPackage Names: [\"com.big\",\"com.small\",\"com.zero\",\"bad name\"]\nApp Sizes: [1000, 10, 0, 5]\nApp Data Sizes: [4000, 20, 0, 5]\nCache Sizes: [500, 1, 0, 5]\n";
        let v = parse_diskstats(t);
        assert_eq!(v.iter().map(|a| a.package.as_str()).collect::<Vec<_>>(), ["com.big", "com.small"], "empty and invalid names are dropped");
        assert_eq!((v[0].app_bytes, v[0].data_bytes, v[0].cache_bytes), (1000, 4000, 500));
    }

    #[test]
    fn disk_sizes_cope_with_short_lists_and_other_wordings() {
        let v = parse_diskstats("Package Names: [\"a.b\",\"c.d\"]\nPackage App Sizes: [7]\nPackage Cache Sizes: [1, 2]\n");
        assert_eq!(v.len(), 2);
        assert_eq!(v.iter().find(|a| a.package == "c.d").unwrap().cache_bytes, 2);
        assert!(parse_diskstats("").is_empty());
        assert!(parse_diskstats("Package Names: not json\nApp Sizes: [1]").is_empty());
    }

    #[test]
    fn day_arithmetic_is_right_across_months_and_leap_years() {
        assert_eq!(days_between("2024-02-28 10:00:00", "2024-03-01 10:00:00"), Some(2), "2024 is a leap year");
        assert_eq!(days_between("2023-02-28 10:00:00", "2023-03-01 10:00:00"), Some(1));
        assert_eq!(days_between("2023-12-31 23:00:00", "2024-01-01 01:00:00"), Some(0));
        assert_eq!(days_between("2023-01-01 00:00:00", "2024-01-01 00:00:00"), Some(365));
        assert_eq!(days_between("2024-05-05 00:00:00", "2024-05-01 00:00:00"), Some(0), "never negative");
        assert_eq!(days_between("garbage", "2024-01-01 00:00:00"), None);
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(epoch_to_naive(1_714_910_000_000), "2024-05-05 11:53:20");
        assert_eq!(epoch_to_naive(0), "1970-01-01 00:00:00");
    }

    const ANDROID12: &str = "@@NOW\n2024-06-30 12:00:00\n@@USAGE\nIn-memory yearly stats\n  packages\n    package=com.android.chrome totalTimeUsed=\"+2h3m\" lastTimeUsed=\"2024-06-29 08:15:00\" totalTimeVisible=\"+2h\" lastTimeVisible=\"2024-06-29 08:20:00\" appLaunchCount=40\n\
    package=com.old.app totalTimeUsed=\"+5s\" lastTimeUsed=\"2024-03-01 09:00:00\" appLaunchCount=2\n  daily\n    package=com.android.chrome totalTimeUsed=\"+1m\" lastTimeUsed=\"2024-06-30 07:00:00\" appLaunchCount=3\n    package=never.opened totalTimeUsed=\"+0s\" lastTimeUsed=\"1970-01-01 00:00:00\"\n";

    #[test]
    fn usage_takes_the_newest_time_across_sections_and_ignores_empty_records() {
        let u = parse_usage(ANDROID12);
        let get = |p: &str| u.iter().find(|x| x.package == p);
        assert_eq!((get("com.android.chrome").unwrap().days_ago, get("com.android.chrome").unwrap().last_used.as_str()), (0, "2024-06-30 07:00:00"));
        assert_eq!(get("com.android.chrome").unwrap().launches, Some(40));
        assert_eq!(get("com.old.app").unwrap().days_ago, 121);
        assert!(get("never.opened").is_none(), "a 1970 date means no record");
    }

    #[test]
    fn usage_understands_the_older_and_the_epoch_formats() {
        let old = "@@NOW\n2024-06-30 12:00:00\n@@USAGE\n    package=com.a.b totalTime=\"+1m\" lastTime=\"2024-06-20 10:00:00\"\n";
        assert_eq!(parse_usage(old)[0].days_ago, 10);
        let epoch = "@@NOW\n2024-05-10 11:53:20\n@@USAGE\n    package=com.c.d lastTimeUsed=1714910000000 appLaunchCount=1\n";
        let u = parse_usage(epoch);
        assert_eq!((u[0].last_used.as_str(), u[0].days_ago), ("2024-05-05 11:53:20", 5));
    }

    #[test]
    fn usage_never_panics_on_junk_or_a_permission_error() {
        assert!(parse_usage("").is_empty());
        assert!(parse_usage("@@NOW\nnot a date\n@@USAGE\npackage=a.b lastTimeUsed=\"2024-06-20 10:00:00\"\n").is_empty());
        assert!(parse_usage("@@NOW\n2024-06-30 12:00:00\n@@USAGE\nSecurityException: Permission Denial: can't dump\n").is_empty());
        assert!(parse_usage("@@NOW\n2024-06-30 12:00:00\n@@USAGE\npackage=bad;name lastTimeUsed=\"2024-06-20 10:00:00\"\n").is_empty());
    }
}
