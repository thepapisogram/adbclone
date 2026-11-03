//! Running-process snapshot: RAM and CPU per app, from one shell round trip.
use crate::{AdbError, AdbRunner};
use serde::Serialize;
use std::collections::HashMap;

/// One shell command printing everything we need, split by `@@` markers.
const SNAPSHOT_CMD: &str = "echo @@STAT; grep '^cpu' /proc/stat; echo @@MEM; head -3 /proc/meminfo; \
echo @@PS; ps -A -o PID,UID,RSS,NAME; echo @@PROC; cat /proc/[0-9]*/stat 2>/dev/null";

#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub uid: u32,
    pub rss_kb: u64,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub total_ticks: u64,
    pub cores: usize,
    pub mem_total_kb: u64,
    pub mem_available_kb: u64,
    pub procs: Vec<Proc>,
    /// pid -> utime+stime clock ticks.
    pub proc_ticks: HashMap<u32, u64>,
}

fn section<'a>(text: &'a str, name: &str) -> &'a str {
    let marker = format!("@@{name}");
    let Some(start) = text.find(&marker) else { return "" };
    let body = &text[start + marker.len()..];
    let end = body.find("@@").unwrap_or(body.len());
    &body[..end]
}

/// Numeric uid, or an Android user name like `u0_a123`, `system`, `root`, `shell`.
pub fn parse_uid(s: &str) -> Option<u32> {
    if let Ok(n) = s.parse() {
        return Some(n);
    }
    match s {
        "root" => return Some(0),
        "system" => return Some(1000),
        "radio" => return Some(1001),
        "shell" => return Some(2000),
        _ => {}
    }
    let (user, app) = s.strip_prefix('u')?.split_once("_a")?;
    Some(user.parse::<u32>().ok()? * 100_000 + 10_000 + app.parse::<u32>().ok()?)
}

pub fn parse_ps(text: &str) -> Vec<Proc> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let pid = it.next()?.parse().ok()?; // header line fails here and is skipped
            let uid = parse_uid(it.next()?)?;
            let rss_kb = it.next()?.parse().ok()?;
            let name = it.collect::<Vec<_>>().join(" ");
            (!name.is_empty()).then_some(Proc { pid, uid, rss_kb, name })
        })
        .collect()
}

/// `pid (comm) S ppid ... utime stime ...`; comm may contain spaces and parentheses.
pub fn parse_proc_stat(text: &str) -> HashMap<u32, u64> {
    text.lines()
        .filter_map(|line| {
            let open = line.find('(')?;
            let close = line.rfind(')')?;
            let pid: u32 = line[..open].trim().parse().ok()?;
            let rest: Vec<&str> = line[close + 1..].split_whitespace().collect();
            // rest[0] is field 3 (state), so utime (14) is rest[11], stime (15) is rest[12].
            let utime: u64 = rest.get(11)?.parse().ok()?;
            let stime: u64 = rest.get(12)?.parse().ok()?;
            Some((pid, utime + stime))
        })
        .collect()
}

pub fn parse_snapshot(text: &str) -> Snapshot {
    let mut snap = Snapshot::default();
    for line in section(text, "STAT").lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("cpu") => snap.total_ticks = it.take(8).filter_map(|v| v.parse::<u64>().ok()).sum(),
            Some(c) if c.starts_with("cpu") => snap.cores += 1,
            _ => {}
        }
    }
    snap.cores = snap.cores.max(1);
    for line in section(text, "MEM").lines() {
        let mut it = line.split_whitespace();
        let (key, val) = (it.next().unwrap_or(""), it.next().and_then(|v| v.parse().ok()).unwrap_or(0));
        match key {
            "MemTotal:" => snap.mem_total_kb = val,
            "MemAvailable:" => snap.mem_available_kb = val,
            _ => {}
        }
    }
    snap.procs = parse_ps(section(text, "PS"));
    snap.proc_ticks = parse_proc_stat(section(text, "PROC"));
    snap
}

pub async fn snapshot(adb: &AdbRunner, serial: &str) -> Result<Snapshot, AdbError> {
    Ok(parse_snapshot(&adb.shell(serial, &[SNAPSHOT_CMD]).await?))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunningApp {
    /// Package name, or `None` for the "system processes" bucket.
    pub package: Option<String>,
    pub rss_kb: u64,
    /// Percent of one core (like `top`); `None` on the first sample.
    pub cpu: Option<f32>,
    pub processes: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunningReport {
    pub apps: Vec<RunningApp>,
    pub mem_total_kb: u64,
    pub mem_available_kb: u64,
    pub cores: usize,
}

/// Which package a process belongs to: name (before `:`) first, then a unique uid owner.
fn owner<'a>(p: &Proc, by_name: &'a HashMap<String, u32>, by_uid: &'a HashMap<u32, Vec<String>>) -> Option<&'a str> {
    let base = p.name.split(':').next().unwrap_or(&p.name);
    if let Some((k, _)) = by_name.get_key_value(base) {
        return Some(k);
    }
    match by_uid.get(&p.uid).map(|v| v.as_slice()) {
        Some([only]) if p.uid >= 10_000 => Some(only),
        _ => None,
    }
}

/// Group processes by package and compute CPU % from two snapshots.
/// `packages` maps package name -> uid (from `pm list packages -U`).
pub fn build_report(cur: &Snapshot, prev: Option<&Snapshot>, packages: &HashMap<String, u32>) -> RunningReport {
    let mut by_uid: HashMap<u32, Vec<String>> = HashMap::new();
    for (name, uid) in packages {
        by_uid.entry(*uid).or_default().push(name.clone());
    }
    let dtotal = prev.map(|p| cur.total_ticks.saturating_sub(p.total_ticks)).unwrap_or(0);

    let mut groups: HashMap<Option<String>, RunningApp> = HashMap::new();
    for p in &cur.procs {
        let pkg = owner(p, packages, &by_uid).map(str::to_string);
        let cpu = match (prev, dtotal) {
            (Some(prev), d) if d > 0 => {
                let now = cur.proc_ticks.get(&p.pid).copied();
                let before = prev.proc_ticks.get(&p.pid).copied();
                // A process that is new since the last sample has no baseline: count 0.
                Some(match (now, before) {
                    (Some(n), Some(b)) => 100.0 * cur.cores as f32 * n.saturating_sub(b) as f32 / d as f32,
                    _ => 0.0,
                })
            }
            _ => None,
        };
        let g = groups.entry(pkg.clone()).or_insert(RunningApp { package: pkg, rss_kb: 0, cpu: None, processes: 0 });
        g.rss_kb += p.rss_kb;
        g.processes += 1;
        if let Some(c) = cpu {
            g.cpu = Some(g.cpu.unwrap_or(0.0) + c);
        }
    }
    let mut apps: Vec<RunningApp> = groups.into_values().collect();
    apps.sort_by(|a, b| b.rss_kb.cmp(&a.rss_kb));
    RunningReport { apps, mem_total_kb: cur.mem_total_kb, mem_available_kb: cur.mem_available_kb, cores: cur.cores }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE1: &str = "@@STAT\ncpu  1000 0 500 8500 0 0 0 0 0 0\ncpu0 250 0 125 2125 0 0 0 0 0 0\ncpu1 250 0 125 2125 0 0 0 0 0 0\ncpu2 250 0 125 2125 0 0 0 0 0 0\ncpu3 250 0 125 2125 0 0 0 0 0 0\n\
@@MEM\nMemTotal:        3862908 kB\nMemFree:          200000 kB\nMemAvailable:    1500000 kB\n\
@@PS\nPID   UID   RSS NAME\n1     0     2560 init\n900   1000  180000 system_server\n2345  10123 150000 com.google.android.gms.persistent\n2400  10123 40000 com.google.android.gms\n3000  10200 90000 com.foo.app\n3001  10200 20000 com.foo.app:sync\n4000  10300 30000 some.renamed.proc\n5000  0     0 [kworker/0:1]\n\
@@PROC\n1 (init) S 0 1 1 0 -1 4194560 100 0 0 0 10 5 0 0 20 0 1 0 1 1 1\n\
900 (system_server) S 1 900 900 0 -1 4194560 100 0 0 0 100 50 0 0 20 0 1 0 1 1 1\n\
2345 (com.google.android.gm (x)) S 1 2345 2345 0 -1 4194560 100 0 0 0 200 100 0 0 20 0 1 0 1 1 1\n\
3000 (com.foo.app) S 1 1 1 0 -1 4194560 100 0 0 0 50 10 0 0 20 0 1 0 1 1 1\n";

    fn next_sample() -> Snapshot {
        let mut s = parse_snapshot(SAMPLE1);
        s.total_ticks += 4000; // 4 cores * 1000 ticks elapsed
        s.proc_ticks.insert(3000, 60 + 500); // +500 ticks of CPU time
        s.proc_ticks.insert(900, 150 + 100); // +100 ticks (system_server)
        s
    }

    #[test]
    fn parses_sections() {
        let s = parse_snapshot(SAMPLE1);
        assert_eq!(s.total_ticks, 1000 + 500 + 8500);
        assert_eq!(s.cores, 4);
        assert_eq!(s.mem_total_kb, 3862908);
        assert_eq!(s.mem_available_kb, 1500000);
        assert_eq!(s.procs.len(), 8);
        assert_eq!(s.procs[2].name, "com.google.android.gms.persistent");
        assert_eq!(s.proc_ticks[&2345], 300, "comm with spaces and parens");
        assert_eq!(s.proc_ticks[&3000], 60);
    }

    #[test]
    fn parses_android_user_names() {
        assert_eq!(parse_uid("u0_a123"), Some(10123));
        assert_eq!(parse_uid("u10_a5"), Some(1_010_005));
        assert_eq!(parse_uid("system"), Some(1000));
        assert_eq!(parse_uid("10123"), Some(10123));
        assert_eq!(parse_uid("nonsense"), None);
    }

    #[test]
    fn ps_handles_names_with_spaces_and_missing_header() {
        let p = parse_ps("12 0 0 kworker/u8:3 events\n");
        assert_eq!(p[0].name, "kworker/u8:3 events");
    }

    fn pkgs() -> HashMap<String, u32> {
        [("com.google.android.gms", 10123), ("com.google.android.gm", 10124), ("com.foo.app", 10200), ("only.owner", 10300)]
            .into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    #[test]
    fn groups_by_name_then_unique_uid() {
        let r = build_report(&parse_snapshot(SAMPLE1), None, &pkgs());
        let get = |n: &str| r.apps.iter().find(|a| a.package.as_deref() == Some(n)).unwrap();
        // ".persistent" is not a package, uid 10123 is owned only by gms, so it joins gms.
        assert_eq!(get("com.google.android.gms").processes, 2);
        assert_eq!(get("com.google.android.gms").rss_kb, 190000);
        // `pkg:sync` joins `pkg` by name.
        assert_eq!(get("com.foo.app").processes, 2);
        // unknown process name, unique uid owner.
        assert_eq!(get("only.owner").rss_kb, 30000);
        // init, system_server and the kworker land in the system bucket.
        let sys = r.apps.iter().find(|a| a.package.is_none()).unwrap();
        assert_eq!(sys.processes, 3);
        assert!(r.apps.iter().all(|a| a.cpu.is_none()), "no CPU on first sample");
        assert!(r.apps.windows(2).all(|w| w[0].rss_kb >= w[1].rss_kb), "sorted by RAM");
    }

    #[test]
    fn cpu_percent_is_of_one_core() {
        let prev = parse_snapshot(SAMPLE1);
        let cur = next_sample();
        let r = build_report(&cur, Some(&prev), &pkgs());
        // 500 ticks of 4000 total over 4 cores => 12.5% of all CPU => 50% of one core.
        let foo = r.apps.iter().find(|a| a.package.as_deref() == Some("com.foo.app")).unwrap();
        assert!((foo.cpu.unwrap() - 50.0).abs() < 0.01, "{:?}", foo.cpu);
        let sys = r.apps.iter().find(|a| a.package.is_none()).unwrap();
        // system_server: 100 ticks of 4000 => 10% of one core.
        assert!((sys.cpu.unwrap() - 10.0).abs() < 0.01, "{:?}", sys.cpu);
    }

    #[test]
    fn missing_sections_give_empty_snapshot() {
        let s = parse_snapshot("");
        assert!(s.procs.is_empty());
        assert_eq!(s.cores, 1);
    }
}
