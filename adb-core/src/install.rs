//! Installing apps from files on the computer (.apk, split sets, .apks, .xapk, .apkm, or a folder of them).
//! Archives are unpacked into a private temporary folder with strict limits, and Android's install
//! errors are turned into plain advice.
use crate::{labels, AdbRunner};
use serde::Serialize;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_ARCHIVE_ENTRIES: usize = 600;
const MAX_UNPACKED_BYTES: u64 = 3 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallItem {
    pub id: String,
    /// What the person dropped (file or folder name).
    pub source: String,
    /// `apk`, `splits` (several apk files of one app), `apks`, `xapk`, `apkm` or `folder`.
    pub kind: String,
    #[serde(skip)]
    pub files: Vec<PathBuf>,
    pub file_count: usize,
    pub package: Option<String>,
    pub version: Option<String>,
    pub label: Option<String>,
    pub permissions: usize,
    pub size_bytes: u64,
    pub warnings: Vec<String>,
    /// Folder holding unpacked files; removed by `discard`.
    #[serde(skip)]
    pub temp_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Replace the app if it is already installed, keeping its data (`-r`).
    pub replace: bool,
    /// Allow installing an older version over a newer one (`-d`).
    pub allow_downgrade: bool,
    /// Allow apps built for testing (`-t`).
    pub allow_test: bool,
}

fn ext(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// Package name, version, name and permission count of one apk, read on this computer.
pub fn summarize(apk: &Path) -> (Option<String>, Option<String>, Option<String>, usize) {
    let Ok(a) = apk_info::Apk::new(apk) else { return (None, None, None, 0) };
    (a.get_package_name(), a.get_version_name(), labels::label_from_apk_file(apk), a.get_permissions().count())
}

fn total_size(files: &[PathBuf]) -> u64 {
    files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum()
}

fn temp_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("coolant-install-{}-{}", std::process::id(), crate::wireless::random_token(8)));
    let _ = std::fs::create_dir_all(&d);
    d
}

/// A file name that is safe to create: only the last path part, no odd characters.
pub(crate) fn safe_name(n: &str) -> String {
    let base = n.rsplit(['/', '\\']).next().unwrap_or(n);
    let cleaned: String = base.chars().map(|c| if c.is_ascii_alphanumeric() || "._-+".contains(c) { c } else { '_' }).collect();
    if cleaned.is_empty() || cleaned.starts_with('.') { format!("f{cleaned}") } else { cleaned }
}

/// Unpack only the `.apk` files (and `manifest.json`) of an archive into `dest`. Returns the entry names kept.
fn unpack(archive: &Path, dest: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let f = std::fs::File::open(archive).map_err(|e| format!("could not open it: {e}"))?;
    let mut zip = zip::ZipArchive::new(f).map_err(|_| "this is not a valid app bundle (zip) file".to_string())?;
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        return Err("this bundle contains far too many files".into());
    }
    let (mut total, mut out, mut used) = (0u64, Vec::new(), std::collections::HashSet::new());
    for i in 0..zip.len() {
        let mut e = zip.by_index(i).map_err(|e| format!("could not read the bundle: {e}"))?;
        if e.is_dir() {
            continue;
        }
        let Some(inner) = e.enclosed_name() else { return Err("the bundle contains an unsafe path".into()) };
        let name = inner.to_string_lossy().replace('\\', "/");
        let wanted = name.to_ascii_lowercase().ends_with(".apk") || name.rsplit('/').next() == Some("manifest.json");
        if !wanted {
            continue;
        }
        total += e.size();
        if total > MAX_UNPACKED_BYTES {
            return Err("the bundle is larger than expected".into());
        }
        let mut file_name = safe_name(&name);
        while !used.insert(file_name.clone()) {
            file_name = format!("{}_{}", crate::wireless::random_token(3), file_name);
        }
        let path = dest.join(&file_name);
        let mut w = std::fs::File::create(&path).map_err(|e| format!("could not write a file: {e}"))?;
        let mut buf = [0u8; 256 * 1024];
        loop {
            let n = e.read(&mut buf).map_err(|e| format!("could not unpack: {e}"))?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
        out.push((name, path));
    }
    Ok(out)
}

/// From the apk entries of a bundle, the ones to install together.
fn choose_bundle_apks(kind: &str, entries: &[(String, PathBuf)]) -> Vec<PathBuf> {
    let apks: Vec<&(String, PathBuf)> = entries.iter().filter(|(n, _)| n.to_ascii_lowercase().ends_with(".apk")).collect();
    let lower = |n: &String| n.to_ascii_lowercase();
    if kind == "apks" {
        // A universal apk stands on its own.
        if let Some((_, p)) = apks.iter().find(|(n, _)| lower(n) == "universal.apk") {
            return vec![p.clone()];
        }
        let splits: Vec<PathBuf> = apks.iter().filter(|(n, _)| lower(n).starts_with("splits/")).map(|(_, p)| p.clone()).collect();
        if !splits.is_empty() {
            return splits;
        }
        // Standalone builds are alternatives for different phones: take just one.
        return apks.iter().find(|(n, _)| lower(n).starts_with("standalones/")).map(|(_, p)| vec![p.clone()]).unwrap_or_default();
    }
    apks.iter().map(|(_, p)| p.clone()).collect()
}

/// XAPK: `manifest.json` lists the apks (`split_apks`), and may list game data (`expansions`) we do not copy.
fn xapk_files(entries: &[(String, PathBuf)]) -> (Vec<PathBuf>, Vec<String>) {
    let mut warnings = Vec::new();
    let manifest = entries.iter().find(|(n, _)| n.rsplit('/').next() == Some("manifest.json"));
    let json: Option<serde_json::Value> = manifest.and_then(|(_, p)| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok());
    if let Some(v) = &json {
        if v.get("expansions").and_then(|e| e.as_array()).is_some_and(|a| !a.is_empty()) {
            warnings.push("This file also contains game data (OBB files) that is not copied. The app may need it to run.".to_string());
        }
        if let Some(list) = v.get("split_apks").and_then(|s| s.as_array()) {
            let files: Vec<PathBuf> = list
                .iter()
                .filter_map(|s| s.get("file").and_then(|f| f.as_str()))
                .filter_map(|f| entries.iter().find(|(n, _)| n == f || n.rsplit('/').next() == f.rsplit('/').next()).map(|(_, p)| p.clone()))
                .collect();
            if !files.is_empty() {
                return (files, warnings);
            }
        }
    }
    (choose_bundle_apks("xapk", entries), warnings)
}

fn new_item(source: &str, kind: &str, files: Vec<PathBuf>, temp: Option<PathBuf>, mut warnings: Vec<String>) -> InstallItem {
    // The base apk (or the first one) tells us what this is.
    let probe = files.iter().find(|f| f.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("base") || n.contains("master"))).or(files.first());
    let (package, version, label, permissions) = probe.map(|p| summarize(p)).unwrap_or((None, None, None, 0));
    if package.is_none() {
        warnings.push("Could not read this app's details; it may be damaged.".into());
    }
    InstallItem {
        id: crate::wireless::random_token(10),
        source: source.to_string(),
        kind: kind.to_string(),
        file_count: files.len(),
        size_bytes: total_size(&files),
        files,
        package,
        version,
        label,
        permissions,
        warnings,
        temp_dir: temp,
    }
}

fn collect_apks(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && ext(p) == "apk")
        .collect();
    out.sort();
    out
}

/// Turn what the person dropped into things that can be installed. One result per dropped bundle,
/// folder, or group of apk files that belong to the same app.
pub fn inspect(paths: &[PathBuf]) -> Vec<Result<InstallItem, String>> {
    let mut out = Vec::new();
    let mut loose: Vec<PathBuf> = Vec::new();
    for p in paths {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
        if p.is_dir() {
            let apks = collect_apks(p);
            if apks.is_empty() {
                out.push(Err(format!("{name}: there are no .apk files in this folder")));
            } else {
                out.push(Ok(new_item(&name, "folder", apks, None, vec![])));
            }
            continue;
        }
        match ext(p).as_str() {
            "apk" => loose.push(p.clone()),
            k @ ("apks" | "xapk" | "apkm") => {
                let tmp = temp_dir();
                match unpack(p, &tmp) {
                    Err(e) => {
                        let _ = std::fs::remove_dir_all(&tmp);
                        out.push(Err(format!("{name}: {e}")));
                    }
                    Ok(entries) => {
                        let (files, warnings) = if k == "xapk" { xapk_files(&entries) } else { (choose_bundle_apks(k, &entries), vec![]) };
                        if files.is_empty() {
                            let _ = std::fs::remove_dir_all(&tmp);
                            out.push(Err(format!("{name}: no app files were found inside")));
                        } else {
                            out.push(Ok(new_item(&name, k, files, Some(tmp), warnings)));
                        }
                    }
                }
            }
            _ => out.push(Err(format!("{name}: only .apk, .apks, .xapk and .apkm files, or folders of .apk files, can be installed"))),
        }
    }
    // Loose apk files that belong to the same app (a base and its splits) are installed together.
    let mut groups: Vec<(Option<String>, Vec<PathBuf>)> = Vec::new();
    for f in loose {
        let pkg = summarize(&f).0;
        match groups.iter_mut().find(|(p, _)| p.is_some() && *p == pkg) {
            Some((_, files)) => files.push(f),
            None => groups.push((pkg, vec![f])),
        }
    }
    for (_, files) in groups {
        let source = if files.len() == 1 { files[0].file_name().and_then(|n| n.to_str()).unwrap_or("app.apk").to_string() } else { format!("{} files", files.len()) };
        let kind = if files.len() == 1 { "apk" } else { "splits" };
        out.push(Ok(new_item(&source, kind, files, None, vec![])));
    }
    out
}

pub fn discard(item: &InstallItem) {
    if let Some(t) = &item.temp_dir {
        let _ = std::fs::remove_dir_all(t);
    }
}

// ---- running the install ----

/// Android's install error codes in plain words.
pub fn friendly_install_error(raw: &str) -> String {
    let code = raw.split("INSTALL_").nth(1).map(|r| format!("INSTALL_{}", r.split([':', ']', ' ', '\n']).next().unwrap_or(""))).unwrap_or_default();
    let advice = match code.as_str() {
        "INSTALL_FAILED_ALREADY_EXISTS" => "This app is already installed. Tick \"Replace if already installed\".",
        "INSTALL_FAILED_UPDATE_INCOMPATIBLE" | "INSTALL_FAILED_SHARED_USER_INCOMPATIBLE" => "The installed copy was signed by someone else, so this one cannot replace it. You would have to uninstall the installed copy first, which deletes its data.",
        "INSTALL_FAILED_VERSION_DOWNGRADE" => "This is an older version than the one installed. Tick \"Allow an older version\" (not every phone allows it).",
        "INSTALL_FAILED_INSUFFICIENT_STORAGE" => "There is not enough free space on the phone.",
        "INSTALL_FAILED_NO_MATCHING_ABIS" => "This app is not built for your phone's processor.",
        "INSTALL_FAILED_OLDER_SDK" => "This app needs a newer version of Android than your phone has.",
        "INSTALL_FAILED_NEWER_SDK" => "This app was made for an older Android and cannot run on this phone.",
        "INSTALL_FAILED_TEST_ONLY" => "This is a test build. Tick \"Allow test builds\".",
        "INSTALL_FAILED_USER_RESTRICTED" => "The phone blocks installs from a computer. On Xiaomi/Redmi/POCO turn on \"Install via USB\" in Developer options; on others check for a security option next to USB debugging.",
        "INSTALL_FAILED_VERIFICATION_FAILURE" | "INSTALL_FAILED_VERIFICATION_TIMEOUT" => "The phone's app check (Play Protect) stopped the install. Look at the phone's screen for a prompt.",
        "INSTALL_FAILED_MISSING_SHARED_LIBRARY" => "This app needs a system library this phone does not have.",
        "INSTALL_FAILED_INVALID_APK" | "INSTALL_PARSE_FAILED_NOT_APK" | "INSTALL_PARSE_FAILED_BAD_MANIFEST" | "INSTALL_PARSE_FAILED_UNEXPECTED_EXCEPTION" => "The file is damaged or is not a complete app. For apps in several parts, install all the parts together.",
        "INSTALL_PARSE_FAILED_NO_CERTIFICATES" | "INSTALL_PARSE_FAILED_INCONSISTENT_CERTIFICATES" => "The app is not signed correctly, or its parts are signed differently.",
        "INSTALL_FAILED_DUPLICATE_PERMISSION" => "Another app already defines a permission this app also defines.",
        "INSTALL_FAILED_CONFLICTING_PROVIDER" => "Another installed app uses the same content provider name.",
        "INSTALL_FAILED_DEXOPT" => "The phone could not prepare the app to run. Free some space and try again.",
        "INSTALL_FAILED_ABORTED" => "The install was cancelled, on the phone or by a security app.",
        _ => "",
    };
    let first = raw.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("The install failed.");
    match (advice.is_empty(), code.is_empty()) {
        (false, _) => format!("{advice} ({code})"),
        (true, false) => format!("The phone refused the install ({code})."),
        (true, true) if raw.contains("device offline") || raw.contains("no devices") => "The phone is not connected any more.".to_string(),
        (true, true) => first.chars().take(300).collect(),
    }
}

/// Install one item on the phone. Returns Ok(short message) or Err(plain advice).
pub async fn install(adb: &AdbRunner, serial: &str, item: &InstallItem, o: Options) -> Result<String, String> {
    if item.files.is_empty() || item.files.iter().any(|f| !f.is_file()) {
        return Err("The file is no longer there. Add it again.".into());
    }
    let mut args: Vec<String> = vec!["-s".into(), serial.into(), if item.files.len() > 1 { "install-multiple".into() } else { "install".into() }];
    if o.replace { args.push("-r".into()); }
    if o.allow_downgrade { args.push("-d".into()); }
    if o.allow_test { args.push("-t".into()); }
    args.extend(item.files.iter().map(|f| f.to_string_lossy().into_owned()));
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    // Big apps over Wi-Fi take a while.
    let out = adb.clone().with_timeout(Duration::from_secs(600)).run_full(&refs).await.map_err(|e| e.to_string())?;
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), out.stderr);
    if out.code == Some(0) && text.contains("Success") {
        Ok(match &item.label {
            Some(l) => format!("{l} installed."),
            None => "Installed.".to_string(),
        })
    } else {
        Err(friendly_install_error(&text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn fx(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("coolant-inst-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn zip_of(path: &Path, entries: &[(&str, Vec<u8>)]) {
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (n, d) in entries {
            w.start_file(*n, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)).unwrap();
            w.write_all(d).unwrap();
        }
        w.finish().unwrap();
    }
    fn bytes(name: &str) -> Vec<u8> { std::fs::read(fx(name)).unwrap() }

    #[test]
    fn reads_an_apk_on_this_computer() {
        let (pkg, _v, label, _p) = summarize(&fx("reference.apk"));
        assert_eq!(pkg.as_deref(), Some("com.test.reference"));
        assert_eq!(label.as_deref(), Some("Referenced App"));
        assert_eq!(summarize(&fx("does-not-exist.apk")).0, None);
    }

    #[test]
    fn a_single_apk_becomes_one_item() {
        let r = inspect(&[fx("literal.apk")]);
        assert_eq!(r.len(), 1);
        let i = r[0].as_ref().unwrap();
        assert_eq!((i.kind.as_str(), i.file_count, i.package.as_deref(), i.label.as_deref()), ("apk", 1, Some("com.test.literal"), Some("Literal App")));
    }

    #[test]
    fn apk_files_of_different_apps_stay_separate() {
        let r = inspect(&[fx("literal.apk"), fx("reference.apk")]);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn an_apks_bundle_installs_its_splits_together() {
        let d = tmp("apks");
        let f = d.join("app.apks");
        zip_of(&f, &[("toc.pb", vec![1, 2]), ("splits/base-master.apk", bytes("reference.apk")), ("splits/base-xxhdpi.apk", bytes("literal.apk")), ("standalones/standalone-xxhdpi.apk", bytes("entities.apk"))]);
        let items = inspect(&[f]);
        let i = items[0].as_ref().unwrap();
        assert_eq!((i.kind.as_str(), i.file_count), ("apks", 2), "splits only, never the standalone alternative");
        assert_eq!(i.package.as_deref(), Some("com.test.reference"), "the base decides what the app is");
        discard(i);
        assert!(!i.temp_dir.as_ref().unwrap().exists(), "unpacked files are cleaned up");
    }

    #[test]
    fn a_universal_apk_stands_alone() {
        let d = tmp("universal");
        let f = d.join("u.apks");
        zip_of(&f, &[("universal.apk", bytes("literal.apk")), ("splits/base-master.apk", bytes("reference.apk"))]);
        assert_eq!(inspect(&[f])[0].as_ref().unwrap().file_count, 1);
    }

    #[test]
    fn xapk_follows_its_manifest_and_warns_about_game_data() {
        let d = tmp("xapk");
        let f = d.join("g.xapk");
        let manifest = br#"{"package_name":"com.test.reference","split_apks":[{"file":"base.apk","id":"base"},{"file":"config.en.apk","id":"config.en"}],"expansions":[{"file":"Android/obb/x.obb"}]}"#.to_vec();
        zip_of(&f, &[("manifest.json", manifest), ("base.apk", bytes("reference.apk")), ("config.en.apk", bytes("literal.apk")), ("unrelated.apk", bytes("entities.apk")), ("Android/obb/x.obb", vec![0; 10])]);
        let items = inspect(&[f]);
        let i = items[0].as_ref().unwrap();
        assert_eq!((i.kind.as_str(), i.file_count), ("xapk", 2));
        assert!(i.warnings.iter().any(|w| w.contains("game data")), "{:?}", i.warnings);
    }

    #[test]
    fn a_folder_of_apks_is_one_item_and_an_empty_folder_is_an_error() {
        let d = tmp("folder");
        std::fs::copy(fx("reference.apk"), d.join("base.apk")).unwrap();
        std::fs::copy(fx("literal.apk"), d.join("split_config.en.apk")).unwrap();
        assert_eq!(inspect(&[d.clone()])[0].as_ref().unwrap().file_count, 2);
        let empty = tmp("empty-folder");
        assert!(inspect(&[empty])[0].is_err());
    }

    #[test]
    fn bad_input_is_explained_not_crashed() {
        let d = tmp("bad");
        let not_zip = d.join("x.apks");
        std::fs::write(&not_zip, b"nope").unwrap();
        let txt = d.join("notes.txt");
        std::fs::write(&txt, b"hi").unwrap();
        let empty_bundle = d.join("e.apks");
        zip_of(&empty_bundle, &[("readme.txt", b"x".to_vec())]);
        let r = inspect(&[not_zip, txt, empty_bundle]);
        assert!(r.iter().all(|x| x.is_err()), "{r:?}");
        assert!(r[0].as_ref().unwrap_err().contains("not a valid app bundle"));
        assert!(r[1].as_ref().unwrap_err().contains("only .apk"));
    }

    #[test]
    fn zip_slip_names_never_escape_the_temporary_folder() {
        let d = tmp("slip");
        let f = d.join("evil.apks");
        zip_of(&f, &[("splits/base-master.apk", bytes("literal.apk")), ("../../escape.apk", bytes("literal.apk"))]);
        let r = inspect(&[f]);
        assert!(r[0].is_err(), "an unsafe path rejects the whole bundle");
        assert!(!d.parent().unwrap().join("escape.apk").exists());
    }

    #[test]
    fn safe_names_drop_directories_and_odd_characters() {
        assert_eq!(safe_name("splits/base-master.apk"), "base-master.apk");
        assert_eq!(safe_name("..\\x y;z.apk"), "x_y_z.apk");
        assert!(safe_name(".hidden").starts_with('f'));
        assert!(!safe_name("").is_empty());
    }

    #[test]
    fn install_errors_come_with_advice() {
        let e = friendly_install_error("Performing Streamed Install\nadb: failed to install x.apk: Failure [INSTALL_FAILED_ALREADY_EXISTS: Attempt to re-install x without first uninstalling.]");
        assert!(e.contains("Replace if already installed") && e.contains("INSTALL_FAILED_ALREADY_EXISTS"), "{e}");
        assert!(friendly_install_error("Failure [INSTALL_FAILED_USER_RESTRICTED: Install canceled by user]").contains("Install via USB"));
        assert!(friendly_install_error("Failure [INSTALL_FAILED_NO_MATCHING_ABIS]").contains("processor"));
        assert!(friendly_install_error("Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match]").contains("signed by someone else"));
        assert!(friendly_install_error("Failure [INSTALL_SOMETHING_NEW]").contains("INSTALL_SOMETHING_NEW"));
        assert!(friendly_install_error("error: device offline").contains("not connected"));
        assert!(friendly_install_error("").len() > 5);
    }
}
