//! Package ratings: how safe a package is to remove, with disable/uninstall tags.
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Safety {
    Safe,
    Caution,
    Unsafe,
}

impl Safety {
    pub fn as_str(self) -> &'static str {
        match self {
            Safety::Safe => "safe",
            Safety::Caution => "caution",
            Safety::Unsafe => "unsafe",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "safe" => Some(Safety::Safe),
            "caution" => Some(Safety::Caution),
            "unsafe" => Some(Safety::Unsafe),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Entry {
    id: String,
    risk: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    prefix: bool,
    disable: Option<String>,
    uninstall: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Rating {
    pub risk: &'static str,
    pub disable: Safety,
    pub uninstall: Safety,
    pub desc: String,
}

/// Packages that must never be rated below `unsafe`, whatever a data file says.
const CORE_BLOCKLIST: &[&str] = &[
    "android",
    "com.android.systemui",
    "com.android.settings",
    "com.android.phone",
    "com.android.providers.telephony",
    "com.android.providers.settings",
    "com.android.providers.contacts",
    "com.android.providers.media",
    "com.android.providers.media.module",
    "com.android.server.telecom",
    "com.android.bluetooth",
    "com.android.nfc",
    "com.android.shell",
    "com.android.packageinstaller",
    "com.google.android.packageinstaller",
    "com.android.permissioncontroller",
    "com.google.android.permissioncontroller",
    "com.android.inputdevices",
    "com.android.externalstorage",
    "com.android.keychain",
    "com.android.networkstack",
    "com.google.android.networkstack",
    "com.android.webview",
    "com.google.android.webview",
    "com.google.android.gms",
    "com.google.android.gsf",
    "com.android.launcher3",
];

const EMBEDDED: &[&str] = &[
    include_str!("../data/ratings/android-core.json"),
    include_str!("../data/ratings/google.json"),
    include_str!("../data/ratings/samsung.json"),
    include_str!("../data/ratings/xiaomi.json"),
    include_str!("../data/ratings/huawei-honor.json"),
    include_str!("../data/ratings/oppo-realme-oneplus.json"),
    include_str!("../data/ratings/motorola-lenovo.json"),
    include_str!("../data/ratings/sony.json"),
    include_str!("../data/ratings/social-partners.json"),
    include_str!("../data/ratings/carriers.json"),
    include_str!("../data/ratings/misc.json"),
];

#[derive(Default)]
pub struct Ratings {
    exact: HashMap<String, Rating>,
    prefixes: Vec<(String, Rating)>,
}

fn build(e: Entry) -> Option<(String, bool, Rating)> {
    let (risk, base) = match e.risk.as_str() {
        "recommended" => ("recommended", Safety::Safe),
        "advanced" => ("advanced", Safety::Caution),
        "unsafe" => ("unsafe", Safety::Unsafe),
        _ => return None,
    };
    let disable = e.disable.as_deref().and_then(Safety::parse).unwrap_or(base);
    // Uninstalling is never rated safer than disabling.
    let uninstall = e.uninstall.as_deref().and_then(Safety::parse).unwrap_or(base).max(disable);
    Some((e.id, e.prefix, Rating { risk, disable, uninstall, desc: e.desc }))
}

impl Ratings {
    fn add_json(&mut self, json: &str) -> Result<usize, serde_json::Error> {
        let entries: Vec<Entry> = serde_json::from_str(json)?;
        let mut n = 0;
        for e in entries {
            if let Some((id, prefix, r)) = build(e) {
                if prefix {
                    self.prefixes.retain(|(p, _)| p != &id);
                    self.prefixes.push((id, r));
                } else {
                    self.exact.insert(id, r);
                }
                n += 1;
            }
        }
        Ok(n)
    }

    /// Built-in lists, then optional user files from `user_dir` (which override).
    pub fn load(user_dir: Option<&Path>) -> Self {
        let mut r = Ratings::default();
        for json in EMBEDDED {
            r.add_json(json).expect("embedded ratings must be valid");
        }
        if let Some(dir) = user_dir {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for f in rd.flatten() {
                    if f.path().extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Ok(text) = std::fs::read_to_string(f.path()) {
                            let _ = r.add_json(&text); // ignore malformed user files
                        }
                    }
                }
            }
        }
        r
    }

    pub fn lookup(&self, id: &str) -> Option<Rating> {
        let mut found = self.exact.get(id).cloned().or_else(|| {
            self.prefixes
                .iter()
                .filter(|(p, _)| id.starts_with(p.as_str()))
                .max_by_key(|(p, _)| p.len())
                .map(|(_, r)| r.clone())
        });
        if CORE_BLOCKLIST.contains(&id) {
            let desc = found.map(|f| f.desc).unwrap_or_default();
            found = Some(Rating { risk: "unsafe", disable: Safety::Unsafe, uninstall: Safety::Unsafe, desc });
        }
        found
    }

    #[cfg(test)]
    fn exact_len(&self) -> usize {
        self.exact.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn embedded_lists_are_valid_and_unique() {
        let mut seen = HashSet::new();
        for json in EMBEDDED {
            let entries: Vec<Entry> = serde_json::from_str(json).unwrap();
            for e in entries {
                assert!(
                    !e.id.is_empty() && e.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_'),
                    "bad id {}",
                    e.id
                );
                assert!(["recommended", "advanced", "unsafe"].contains(&e.risk.as_str()), "bad risk {}", e.id);
                assert!(seen.insert(e.id.clone()), "duplicate {}", e.id);
            }
        }
        assert!(Ratings::load(None).exact_len() >= 300);
    }

    #[test]
    fn previous_ratings_are_kept() {
        let r = Ratings::load(None);
        for (id, risk) in [
            ("com.google.android.gms", "unsafe"),
            ("com.android.systemui", "unsafe"),
            ("com.android.vending", "advanced"),
            ("com.facebook.appmanager", "recommended"),
            ("com.samsung.android.bixby.agent", "recommended"),
            ("com.google.android.youtube", "recommended"),
        ] {
            assert_eq!(r.lookup(id).unwrap().risk, risk, "{id}");
        }
    }

    #[test]
    fn derives_tags_and_never_rates_uninstall_safer() {
        let mut r = Ratings::default();
        r.add_json(r#"[
          {"id":"a.safe","risk":"recommended"},
          {"id":"a.mid","risk":"advanced"},
          {"id":"a.odd","risk":"recommended","disable":"caution","uninstall":"safe"}
        ]"#).unwrap();
        let s = r.lookup("a.safe").unwrap();
        assert_eq!((s.disable, s.uninstall), (Safety::Safe, Safety::Safe));
        assert_eq!(r.lookup("a.mid").unwrap().disable, Safety::Caution);
        let o = r.lookup("a.odd").unwrap();
        assert_eq!((o.disable, o.uninstall), (Safety::Caution, Safety::Caution));
        assert!(r.lookup("not.rated").is_none());
    }

    #[test]
    fn core_blocklist_overrides_data_files() {
        let mut r = Ratings::default();
        r.add_json(r#"[{"id":"com.android.systemui","risk":"recommended"}]"#).unwrap();
        let x = r.lookup("com.android.systemui").unwrap();
        assert_eq!((x.risk, x.disable, x.uninstall), ("unsafe", Safety::Unsafe, Safety::Unsafe));
    }

    #[test]
    fn prefix_rules_lose_to_exact() {
        let mut r = Ratings::default();
        r.add_json(r#"[{"id":"com.v.","risk":"recommended","prefix":true},{"id":"com.v.keep","risk":"unsafe"}]"#).unwrap();
        assert_eq!(r.lookup("com.v.other").unwrap().risk, "recommended");
        assert_eq!(r.lookup("com.v.keep").unwrap().risk, "unsafe");
    }
}
