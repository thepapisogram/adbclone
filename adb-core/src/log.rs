//! A record of every command sent to the phone, shown in the Logs tab so nothing is hidden.
//! Secrets (pairing codes) are masked, and a command repeated back to back is counted, not repeated,
//! so background polling does not push everything else out.
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_ENTRIES: usize = 500;
const MAX_COMMAND_CHARS: usize = 1200;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    /// Changes every time this entry is created or updated; ask for entries newer than the last one you saw.
    pub seq: u64,
    pub time_ms: u64,
    pub command: String,
    pub ok: bool,
    pub millis: u64,
    /// How many times in a row this exact command ran.
    pub count: u32,
    pub note: Option<String>,
}

struct Log {
    entries: VecDeque<Entry>,
    next_id: u64,
    seq: u64,
}

fn log() -> &'static Mutex<Log> {
    static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(Log { entries: VecDeque::new(), next_id: 1, seq: 0 }))
}

/// The command as the user should see it. A pairing code is a one-time password: never show it.
pub fn display_command(args: &[&str]) -> String {
    let mut parts: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    if let Some(i) = parts.iter().position(|a| a == "pair") {
        // adb pair <address> <code>
        if i + 2 < parts.len() {
            for p in parts.iter_mut().skip(i + 2) {
                *p = "******".into();
            }
        }
    }
    let mut s = format!("adb {}", parts.join(" "));
    if s.chars().count() > MAX_COMMAND_CHARS {
        s = s.chars().take(MAX_COMMAND_CHARS).collect::<String>() + " …";
    }
    s
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn record(args: &[&str], ok: bool, millis: u64, note: Option<String>) {
    let command = display_command(args);
    let mut l = log().lock().unwrap();
    l.seq += 1;
    let seq = l.seq;
    if let Some(last) = l.entries.back_mut() {
        if last.command == command {
            last.seq = seq;
            last.time_ms = now_ms();
            last.ok = ok;
            last.millis = millis;
            last.count = last.count.saturating_add(1);
            last.note = note;
            return;
        }
    }
    let id = l.next_id;
    l.next_id += 1;
    l.entries.push_back(Entry { id, seq, time_ms: now_ms(), command, ok, millis, count: 1, note });
    while l.entries.len() > MAX_ENTRIES {
        l.entries.pop_front();
    }
}

/// Entries created or updated after `seq` (0 for everything), oldest first, and the newest seq seen.
pub fn since(seq: u64) -> (Vec<Entry>, u64) {
    let l = log().lock().unwrap();
    (l.entries.iter().filter(|e| e.seq > seq).cloned().collect(), l.seq)
}

pub fn clear() {
    let mut l = log().lock().unwrap();
    l.entries.clear();
    l.seq += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_codes_are_masked() {
        assert_eq!(display_command(&["pair", "192.168.1.20:37099", "123456"]), "adb pair 192.168.1.20:37099 ******");
        assert_eq!(display_command(&["-s", "ABC", "shell", "pm list packages"]), "adb -s ABC shell pm list packages");
        // A package or setting that merely contains the word is not touched.
        assert_eq!(display_command(&["shell", "echo pair"]), "adb shell echo pair");
    }

    #[test]
    fn very_long_commands_are_cut() {
        let long = "x".repeat(5000);
        let s = display_command(&["shell", &long]);
        assert!(s.chars().count() < 1300 && s.ends_with('…'));
    }

    // The log is shared by the whole test process, so each test uses its own unique command text.
    #[test]
    fn repeats_collapse_and_updates_are_delivered_once() {
        let (_, start) = since(0);
        record(&["shell", "repeat-test-a"], true, 5, None);
        record(&["shell", "repeat-test-a"], true, 7, None);
        record(&["shell", "repeat-test-a"], false, 9, Some("boom".into()));
        let (new, seq) = since(start);
        let mine: Vec<_> = new.iter().filter(|e| e.command.contains("repeat-test-a")).collect();
        assert_eq!(mine.len(), 1);
        assert_eq!((mine[0].count, mine[0].ok, mine[0].millis, mine[0].note.as_deref()), (3, false, 9, Some("boom")));
        assert!(seq > start);
        let (again, _) = since(seq);
        assert!(!again.iter().any(|e| e.command.contains("repeat-test-a")), "nothing new since the last look");
    }

    #[test]
    fn a_different_command_in_between_breaks_the_run() {
        let (_, start) = since(0);
        record(&["shell", "alt-test-1"], true, 1, None);
        record(&["shell", "alt-test-2"], true, 1, None);
        record(&["shell", "alt-test-1"], true, 1, None);
        let (new, _) = since(start);
        assert_eq!(new.iter().filter(|e| e.command.contains("alt-test-1")).count(), 2);
    }
}
