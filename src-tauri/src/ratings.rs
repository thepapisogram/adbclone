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
    // v2 fields; all optional so older and user-written files keep working.
    tier: Option<String>,
    category: Option<String>,
    impact: Option<String>,
    confidence: Option<String>,
    reinstall: Option<String>,
    duplicate_of: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Rating {
    pub risk: &'static str,
    pub disable: Safety,
    pub uninstall: Safety,
    pub desc: String,
    /// How much you would miss it: `safe`, `balanced`, `aggressive`, or `never` (not offered at all).
    pub tier: &'static str,
    pub category: Option<String>,
    /// One line: what stops working if it is removed.
    pub impact: String,
    /// How sure the rating is: `high`, `medium` or `low`. Only high-confidence safe entries are pre-ticked.
    pub confidence: &'static str,
    /// `store` (reinstall from an app store) or `rom-only` (restore with a snapshot).
    pub reinstall: Option<String>,
    pub duplicate_of: Option<String>,
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
    // Without an explicit tier the old two-level rating decides; unsafe is never offered.
    let tier = match (risk, e.tier.as_deref()) {
        ("unsafe", _) => "never",
        (_, Some("safe")) => "safe",
        (_, Some("balanced")) => "balanced",
        (_, Some("aggressive")) => "aggressive",
        ("recommended", _) => "safe",
        _ => "balanced",
    };
    let confidence = match e.confidence.as_deref() {
        Some("high") => "high",
        Some("low") => "low",
        _ => "medium",
    };
    Some((
        e.id,
        e.prefix,
        Rating {
            risk,
            disable,
            uninstall,
            desc: e.desc,
            tier,
            category: e.category,
            impact: e.impact.unwrap_or_default(),
            confidence,
            reinstall: e.reinstall,
            duplicate_of: e.duplicate_of,
        },
    ))
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
            let desc = found.as_ref().map(|f| f.desc.clone()).unwrap_or_default();
            let (category, impact) = found.map(|f| (f.category, f.impact)).unwrap_or((None, String::new()));
            found = Some(Rating {
                risk: "unsafe",
                disable: Safety::Unsafe,
                uninstall: Safety::Unsafe,
                desc,
                tier: "never",
                category: category.or_else(|| Some("core-system".into())),
                impact: if impact.is_empty() { "Can stop the phone from working properly. Never remove.".into() } else { impact },
                confidence: "high",
                reinstall: None,
                duplicate_of: None,
            });
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

    const CATEGORIES: &[&str] = &[
        "ads-telemetry", "partner-stub", "help-promo", "discontinued", "aosp-extra", "game-hub", "assistant", "vendor-app", "google-app",
        "preinstalled-app", "store", "carrier", "cloud-sync", "payments", "system-service", "core-app", "core-system", "background-service",
        "duplicate", "other",
    ];

    #[test]
    fn every_entry_follows_the_v2_rules() {
        let mut safe_high = 0;
        for json in EMBEDDED {
            let entries: Vec<Entry> = serde_json::from_str(json).unwrap();
            for e in entries {
                let id = &e.id;
                let cat = e.category.as_deref().unwrap_or_else(|| panic!("{id}: missing category"));
                assert!(CATEGORIES.contains(&cat), "{id}: unknown category {cat}");
                let conf = e.confidence.as_deref().unwrap_or_else(|| panic!("{id}: missing confidence"));
                assert!(["high", "medium", "low"].contains(&conf), "{id}: bad confidence");
                assert!(e.impact.as_deref().map_or(false, |i| i.trim().len() >= 15), "{id}: needs an impact line");
                match e.risk.as_str() {
                    "unsafe" => assert!(e.tier.is_none() && cat == "core-system", "{id}: unsafe entries are core-system and have no tier"),
                    _ => {
                        let tier = e.tier.as_deref().unwrap_or_else(|| panic!("{id}: missing tier"));
                        assert!(["safe", "balanced", "aggressive"].contains(&tier), "{id}: bad tier");
                        // A removal that could matter is never rated as plainly recommended, and the reverse.
                        if tier == "safe" { assert_eq!(e.risk, "recommended", "{id}: safe tier needs a recommended rating"); }
                        if tier == "aggressive" { assert_eq!(e.risk, "advanced", "{id}: aggressive tier needs an advanced rating"); }
                        if tier == "safe" && conf == "high" { safe_high += 1; }
                        // Things other features depend on can never be in the safe tier.
                        if tier == "safe" {
                            assert!(!["system-service", "core-app", "cloud-sync", "payments", "store", "assistant", "carrier"].contains(&cat), "{id}: {cat} cannot be in the safe tier");
                        }
                    }
                }
            }
        }
        assert!(safe_high >= 25, "the Safe preset needs real content (found {safe_high})");
    }

    #[test]
    fn tiers_follow_the_rules_when_loaded() {
        let r = Ratings::load(None);
        let fb = r.lookup("com.facebook.appmanager").unwrap();
        assert_eq!((fb.tier, fb.confidence, fb.category.as_deref()), ("safe", "high", Some("partner-stub")));
        assert!(!fb.impact.is_empty());
        let play = r.lookup("com.android.vending").unwrap();
        assert_eq!(play.tier, "aggressive");
        let gms = r.lookup("com.google.android.gms").unwrap();
        assert_eq!((gms.tier, gms.confidence), ("never", "high"));
        assert!(!gms.impact.is_empty());
    }

    #[test]
    fn older_and_user_files_without_v2_fields_still_work() {
        let mut r = Ratings::default();
        r.add_json(r#"[{"id":"a.rec","risk":"recommended"},{"id":"a.adv","risk":"advanced"},{"id":"a.bad","risk":"unsafe","tier":"safe"}]"#).unwrap();
        assert_eq!((r.lookup("a.rec").unwrap().tier, r.lookup("a.rec").unwrap().confidence), ("safe", "medium"));
        assert_eq!(r.lookup("a.adv").unwrap().tier, "balanced");
        assert_eq!(r.lookup("a.bad").unwrap().tier, "never", "an unsafe app can never be put in a tier by a file");
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
