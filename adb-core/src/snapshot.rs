//! Snapshots of package states, and the plan to get a device back to one.
use crate::packages::{self, valid_name, PkgState};
use crate::AdbRunner;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageEntry {
    pub name: String,
    pub state: PkgState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub id: String,
    pub name: String,
    /// Unix seconds.
    pub created: u64,
    /// Taken automatically before a batch (as opposed to by the user).
    pub auto: bool,
    pub device_model: Option<String>,
    pub packages: Vec<PackageEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub name: String,
    pub created: u64,
    pub auto: bool,
    pub device_model: Option<String>,
    pub enabled: usize,
    pub disabled: usize,
    pub uninstalled: usize,
}

impl Snapshot {
    pub fn new(name: &str, created: u64, auto: bool, device_model: Option<String>, packages: Vec<PackageEntry>) -> Self {
        Snapshot {
            version: FORMAT_VERSION,
            id: make_id(created, name),
            name: name.trim().chars().take(80).collect(),
            created,
            auto,
            device_model,
            packages,
        }
    }

    pub fn meta(&self) -> SnapshotMeta {
        let count = |s| self.packages.iter().filter(|p| p.state == s).count();
        SnapshotMeta {
            id: self.id.clone(),
            name: self.name.clone(),
            created: self.created,
            auto: self.auto,
            device_model: self.device_model.clone(),
            enabled: count(PkgState::Enabled),
            disabled: count(PkgState::Disabled),
            uninstalled: count(PkgState::Uninstalled),
        }
    }
}

/// `<created>-<slug>`: sortable, filesystem-safe.
pub fn make_id(created: u64, name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_matches('-').chars().take(30).collect();
    if slug.is_empty() { created.to_string() } else { format!("{created}-{slug}") }
}

/// Ids become file names, so only a strict character set is accepted.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// One thing to do to a package. The names match the app's action names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    Restore,
    Enable,
    Disable,
    Uninstall,
}

impl Step {
    pub fn as_str(self) -> &'static str {
        match self {
            Step::Restore => "restore",
            Step::Enable => "enable",
            Step::Disable => "disable",
            Step::Uninstall => "uninstall",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub name: String,
    pub from: PkgState,
    pub to: PkgState,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePlan {
    pub changes: Vec<Change>,
    /// In the snapshot but not on this device (for example another phone or an older firmware).
    pub missing: Vec<String>,
    pub unchanged: usize,
}

fn steps_for(from: PkgState, to: PkgState) -> Vec<Step> {
    use PkgState::*;
    match (from, to) {
        // `install-existing` may bring the package back still disabled, so enable explicitly.
        (Uninstalled, Enabled) => vec![Step::Restore, Step::Enable],
        (Uninstalled, Disabled) => vec![Step::Restore, Step::Disable],
        (Disabled, Enabled) => vec![Step::Enable],
        (Enabled, Disabled) => vec![Step::Disable],
        (Enabled, Uninstalled) | (Disabled, Uninstalled) => vec![Step::Uninstall],
        _ => vec![],
    }
}

/// What has to change to turn the device's `current` states into the snapshot's.
/// Packages not in the snapshot (for example apps installed since) are left alone.
pub fn plan_restore(snap: &Snapshot, current: &HashMap<String, PkgState>) -> RestorePlan {
    let (mut changes, mut missing, mut unchanged) = (Vec::new(), Vec::new(), 0);
    for p in &snap.packages {
        match current.get(&p.name) {
            None => missing.push(p.name.clone()),
            Some(&now) if now == p.state => unchanged += 1,
            Some(&now) => changes.push(Change { name: p.name.clone(), from: now, to: p.state, steps: steps_for(now, p.state) }),
        }
    }
    // Bring apps back first, then enables, disables, and removals last.
    changes.sort_by_key(|c| match c.steps.first() {
        Some(Step::Restore) => 0,
        Some(Step::Enable) => 1,
        Some(Step::Disable) => 2,
        _ => 3,
    });
    RestorePlan { changes, missing, unchanged }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StepOutcome {
    pub name: String,
    pub ok: bool,
    pub message: Option<String>,
}

/// Run a plan against the device, one package at a time. If a step fails, that package's later steps
/// are skipped (they depend on it) and the failure is reported; other packages carry on.
pub async fn execute(adb: &AdbRunner, serial: &str, plan: &RestorePlan) -> Vec<StepOutcome> {
    let mut out = Vec::with_capacity(plan.changes.len());
    for change in &plan.changes {
        let mut failure = None;
        for step in &change.steps {
            let r = match step {
                Step::Restore => packages::restore(adb, serial, &change.name).await,
                Step::Enable => packages::enable(adb, serial, &change.name).await,
                Step::Disable => packages::disable(adb, serial, &change.name).await,
                Step::Uninstall => packages::uninstall_for_user(adb, serial, &change.name).await,
            };
            if let Err(e) = r {
                failure = Some(format!("{}: {e}", step.as_str()));
                break;
            }
        }
        out.push(StepOutcome { name: change.name.clone(), ok: failure.is_none(), message: failure });
    }
    out
}

// ---- storage ----

fn file_for(dir: &Path, id: &str) -> io::Result<PathBuf> {
    if !valid_id(id) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid snapshot id"));
    }
    Ok(dir.join(format!("{id}.json")))
}

pub fn save(dir: &Path, snap: &Snapshot) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = file_for(dir, &snap.id)?;
    let json = serde_json::to_string(snap).map_err(io::Error::other)?;
    // Write then rename so a crash never leaves a half-written snapshot.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(tmp, path)
}

pub fn load(dir: &Path, id: &str) -> io::Result<Snapshot> {
    let text = fs::read_to_string(file_for(dir, id)?)?;
    serde_json::from_str(&text).map_err(io::Error::other)
}

pub fn delete(dir: &Path, id: &str) -> io::Result<()> {
    fs::remove_file(file_for(dir, id)?)
}

/// All readable snapshots, newest first. Unreadable files are skipped.
pub fn list(dir: &Path) -> Vec<SnapshotMeta> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<SnapshotMeta> = rd
        .flatten()
        .filter(|f| f.path().extension().and_then(|e| e.to_str()) == Some("json"))
        .filter_map(|f| serde_json::from_str::<Snapshot>(&fs::read_to_string(f.path()).ok()?).ok())
        .map(|s| s.meta())
        .collect();
    out.sort_by(|a, b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
    out
}

/// Keep only the newest `keep` automatic snapshots; user-made ones are never removed.
pub fn prune_auto(dir: &Path, keep: usize) {
    let autos: Vec<SnapshotMeta> = list(dir).into_iter().filter(|m| m.auto).collect();
    for m in autos.into_iter().skip(keep) {
        let _ = delete(dir, &m.id);
    }
}

/// Parse and sanitise a snapshot from a file the user chose. Gets a fresh id so nothing is overwritten.
pub fn import_from_str(json: &str, created_now: u64) -> Result<Snapshot, String> {
    if json.len() > 20_000_000 {
        return Err("That file is too large to be a snapshot.".into());
    }
    let mut s: Snapshot = serde_json::from_str(json).map_err(|_| "That file is not a valid snapshot.".to_string())?;
    if s.version == 0 || s.version > FORMAT_VERSION {
        return Err(format!("This snapshot was made by a newer version of the app (format {}).", s.version));
    }
    s.packages.retain(|p| valid_name(&p.name));
    if s.packages.is_empty() {
        return Err("The snapshot contains no packages.".into());
    }
    s.auto = false;
    s.created = created_now;
    s.name = format!("{} (imported)", s.name.trim().chars().take(60).collect::<String>());
    s.id = make_id(created_now, &s.name);
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use PkgState::*;

    fn snap(pkgs: &[(&str, PkgState)]) -> Snapshot {
        Snapshot::new("Test", 1000, false, Some("Pixel".into()), pkgs.iter().map(|(n, s)| PackageEntry { name: n.to_string(), state: *s }).collect())
    }

    fn now(pkgs: &[(&str, PkgState)]) -> HashMap<String, PkgState> {
        pkgs.iter().map(|(n, s)| (n.to_string(), *s)).collect()
    }

    #[test]
    fn plans_every_transition() {
        let s = snap(&[("a", Enabled), ("b", Enabled), ("c", Disabled), ("d", Disabled), ("e", Uninstalled), ("f", Uninstalled), ("same", Enabled)]);
        let cur = now(&[("a", Disabled), ("b", Uninstalled), ("c", Enabled), ("d", Uninstalled), ("e", Enabled), ("f", Disabled), ("same", Enabled), ("extra", Disabled)]);
        let p = plan_restore(&s, &cur);
        let steps = |n: &str| p.changes.iter().find(|c| c.name == n).unwrap().steps.clone();
        assert_eq!(steps("a"), vec![Step::Enable]);
        assert_eq!(steps("b"), vec![Step::Restore, Step::Enable]);
        assert_eq!(steps("c"), vec![Step::Disable]);
        assert_eq!(steps("d"), vec![Step::Restore, Step::Disable]);
        assert_eq!(steps("e"), vec![Step::Uninstall]);
        assert_eq!(steps("f"), vec![Step::Uninstall]);
        assert_eq!((p.unchanged, p.changes.len(), p.missing.len()), (1, 6, 0));
        assert!(p.changes.iter().all(|c| c.name != "extra"), "apps not in the snapshot are left alone");
    }

    #[test]
    fn orders_restores_first_and_removals_last() {
        let s = snap(&[("rm", Uninstalled), ("dis", Disabled), ("back", Enabled), ("en", Enabled)]);
        let cur = now(&[("rm", Enabled), ("dis", Enabled), ("back", Uninstalled), ("en", Disabled)]);
        let names: Vec<_> = plan_restore(&s, &cur).changes.into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["back", "en", "dis", "rm"]);
    }

    #[test]
    fn reports_missing_and_identical() {
        let s = snap(&[("gone", Enabled), ("here", Enabled)]);
        let p = plan_restore(&s, &now(&[("here", Enabled)]));
        assert_eq!(p.missing, ["gone"]);
        assert!(p.changes.is_empty() && p.unchanged == 1);
    }

    #[test]
    fn ids_are_safe() {
        assert_eq!(make_id(5, "Before: disable 3 apps!"), "5-before-disable-3-apps");
        assert_eq!(make_id(5, "***"), "5");
        for good in ["123-abc", "1", "a-b-c"] { assert!(valid_id(good), "{good}"); }
        for bad in ["", "../x", "a/b", "a.b", "A", "x y", &"1".repeat(65)] { assert!(!valid_id(bad), "{bad}"); }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("adbm-snap-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn save_load_list_delete_roundtrip() {
        let d = tmp("roundtrip");
        let a = snap(&[("a", Enabled), ("b", Disabled), ("c", Uninstalled)]);
        save(&d, &a).unwrap();
        let loaded = load(&d, &a.id).unwrap();
        assert_eq!(loaded.packages, a.packages);
        let metas = list(&d);
        assert_eq!((metas.len(), metas[0].enabled, metas[0].disabled, metas[0].uninstalled), (1, 1, 1, 1));
        delete(&d, &a.id).unwrap();
        assert!(list(&d).is_empty());
        assert!(load(&d, "../etc/passwd").is_err(), "path traversal is rejected");
        assert!(delete(&d, "../x").is_err());
    }

    #[test]
    fn prune_keeps_newest_auto_and_all_manual() {
        let d = tmp("prune");
        for i in 0..5u64 {
            let mut s = Snapshot::new(&format!("auto {i}"), 100 + i, true, None, vec![]);
            s.auto = true;
            save(&d, &s).unwrap();
        }
        save(&d, &Snapshot::new("mine", 1, false, None, vec![])).unwrap();
        prune_auto(&d, 2);
        let m = list(&d);
        assert_eq!(m.iter().filter(|x| x.auto).count(), 2);
        assert!(m.iter().any(|x| x.name == "mine"));
        assert_eq!(m.iter().filter(|x| x.auto).map(|x| x.created).max(), Some(104));
    }

    #[test]
    fn import_sanitises_and_rejects() {
        let good = r#"{"version":1,"id":"../evil","name":"Backup","created":5,"auto":true,"device_model":null,
            "packages":[{"name":"com.ok.app","state":"disabled"},{"name":"bad; rm -rf /","state":"enabled"}]}"#;
        let s = import_from_str(good, 99).unwrap();
        assert!(valid_id(&s.id) && !s.auto && s.created == 99);
        assert_eq!(s.packages.len(), 1, "invalid package names are dropped");
        assert!(s.name.ends_with("(imported)"));
        assert!(import_from_str("not json", 1).is_err());
        assert!(import_from_str(r#"{"version":99,"id":"x","name":"n","created":1,"auto":false,"device_model":null,"packages":[]}"#, 1).unwrap_err().contains("newer"));
        assert!(import_from_str(r#"{"version":1,"id":"x","name":"n","created":1,"auto":false,"device_model":null,"packages":[]}"#, 1).is_err());
    }
}
