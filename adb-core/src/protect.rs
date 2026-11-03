//! Which apps the phone is using for its main jobs right now (home screen, dialer, messages, browser,
//! keyboard, assistant). Removing one of these is the classic way to break a phone, so they are never offered.
use crate::deviceinfo::section;
use crate::packages::valid_name;
use crate::{AdbError, AdbRunner};
use serde::Serialize;

pub const CMD: &str = "echo @@HOME; cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME 2>&1; \
echo @@HOMEALL; cmd package query-activities --brief -a android.intent.action.MAIN -c android.intent.category.HOME 2>&1; \
echo @@BROWSER; cmd package resolve-activity --brief -a android.intent.action.VIEW -c android.intent.category.BROWSABLE -d https://example.com 2>&1; \
echo @@DIALER; cmd package resolve-activity --brief -a android.intent.action.DIAL 2>&1; \
echo @@SMS; cmd package resolve-activity --brief -a android.intent.action.SENDTO -d smsto:123 2>&1; \
echo @@CAMERA; cmd package resolve-activity --brief -a android.media.action.IMAGE_CAPTURE 2>&1; \
echo @@IME; settings get secure default_input_method 2>&1; \
echo @@ASSIST; settings get secure assistant 2>&1; \
echo @@ROLES; for r in android.app.role.HOME android.app.role.DIALER android.app.role.SMS android.app.role.BROWSER android.app.role.ASSISTANT android.app.role.EMERGENCY; do echo \"$r $(cmd role get-role-holders --user 0 $r 2>/dev/null)\"; done";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Protection {
    pub package: String,
    /// Plain words, for example "your browser".
    pub reason: String,
}

/// The package of a component line such as `com.x.y/.MainActivity` (or `com.x.y/com.x.y.Main`).
fn component_package(line: &str) -> Option<&str> {
    let t = line.trim();
    let (pkg, class) = t.split_once('/')?;
    // Real component lines have no spaces; this skips chatter like "priority=0 preferredOrder=0 ...".
    (!t.contains(' ') && !class.is_empty() && valid_name(pkg)).then_some(pkg)
}

/// "android" is the system's chooser: it means no default has been picked, not that the chooser is an app to protect.
fn real_app(pkg: &str) -> bool {
    pkg != "android" && pkg != "system" && !pkg.is_empty()
}

fn packages_in(text: &str) -> Vec<String> {
    let mut v: Vec<String> = text.lines().filter_map(component_package).filter(|p| real_app(p)).map(str::to_string).collect();
    v.dedup();
    v
}

/// The first app named in a `resolve-activity` answer (that command names just the chosen one).
fn first_package(text: &str) -> Option<String> {
    packages_in(text).into_iter().next()
}

fn setting_package(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t == "null" || t.contains(' ') {
        return None;
    }
    let pkg = t.split('/').next()?;
    (valid_name(pkg) && real_app(pkg)).then(|| pkg.to_string())
}

pub fn parse(text: &str) -> Vec<Protection> {
    let mut out: Vec<Protection> = Vec::new();
    let mut add = |pkg: Option<String>, reason: &str| {
        let Some(pkg) = pkg else { return };
        match out.iter_mut().find(|p| p.package == pkg) {
            Some(p) => {
                if !p.reason.split(", ").any(|r| r == reason) {
                    p.reason = format!("{}, {}", p.reason, reason);
                }
            }
            None => out.push(Protection { package: pkg, reason: reason.to_string() }),
        }
    };
    add(first_package(section(text, "HOME")), "your home screen app");
    for p in packages_in(section(text, "HOMEALL")) {
        add(Some(p), "a home screen app");
    }
    add(first_package(section(text, "BROWSER")), "your browser");
    add(first_package(section(text, "DIALER")), "your phone app");
    add(first_package(section(text, "SMS")), "your messages app");
    add(first_package(section(text, "CAMERA")), "your camera app");
    add(setting_package(section(text, "IME")), "your keyboard");
    add(setting_package(section(text, "ASSIST")), "your voice assistant");
    for line in section(text, "ROLES").lines() {
        let mut it = line.split_whitespace();
        let (Some(role), Some(holder)) = (it.next(), it.next()) else { continue };
        if !valid_name(holder) || !real_app(holder) {
            continue;
        }
        let reason = match role {
            "android.app.role.HOME" => "your home screen app",
            "android.app.role.DIALER" => "your phone app",
            "android.app.role.SMS" => "your messages app",
            "android.app.role.BROWSER" => "your browser",
            "android.app.role.ASSISTANT" => "your voice assistant",
            "android.app.role.EMERGENCY" => "your emergency information app",
            _ => continue,
        };
        add(Some(holder.to_string()), reason);
    }
    // "your home screen app" already says it all; do not repeat it as "a home screen app".
    for p in &mut out {
        if p.reason.contains("your home screen app") {
            p.reason = p.reason.split(", ").filter(|r| *r != "a home screen app").collect::<Vec<_>>().join(", ");
        }
    }
    out
}

/// Ask the phone. Any part that fails simply protects less; the caller should say so when nothing came back.
pub async fn detect(adb: &AdbRunner, serial: &str) -> Result<Vec<Protection>, AdbError> {
    Ok(parse(&adb.shell(serial, &[CMD]).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIXEL: &str = "@@HOME\npriority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\ncom.google.android.apps.nexuslauncher/.NexusLauncherActivity\n\
@@HOMEALL\n1 activities found:\n  priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=false\n  com.google.android.apps.nexuslauncher/.NexusLauncherActivity\n\
@@BROWSER\npriority=0 preferredOrder=0 match=0x608000 specificIndex=-1 isDefault=true\ncom.android.chrome/com.google.android.apps.chrome.IntentDispatcher\n\
@@DIALER\npriority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\ncom.google.android.dialer/.extensions.GoogleDialtactsActivity\n\
@@SMS\npriority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\ncom.google.android.apps.messaging/.ui.conversation.ConversationActivity\n\
@@CAMERA\nNo activity found\n\
@@IME\ncom.google.android.inputmethod.latin/com.android.inputmethod.latin.LatinIME\n\
@@ASSIST\ncom.google.android.googlequicksearchbox/com.google.android.voiceinteraction.GoogleVoiceInteractionService\n\
@@ROLES\nandroid.app.role.HOME com.google.android.apps.nexuslauncher\nandroid.app.role.DIALER com.google.android.dialer\nandroid.app.role.SMS com.google.android.apps.messaging\nandroid.app.role.BROWSER com.android.chrome\nandroid.app.role.ASSISTANT com.google.android.googlequicksearchbox\nandroid.app.role.EMERGENCY \n";

    fn reason<'a>(p: &'a [Protection], pkg: &str) -> Option<&'a str> {
        p.iter().find(|x| x.package == pkg).map(|x| x.reason.as_str())
    }

    #[test]
    fn finds_the_apps_a_pixel_uses() {
        let p = parse(PIXEL);
        assert_eq!(reason(&p, "com.google.android.apps.nexuslauncher"), Some("your home screen app"));
        assert_eq!(reason(&p, "com.android.chrome"), Some("your browser"));
        assert_eq!(reason(&p, "com.google.android.dialer"), Some("your phone app"));
        assert_eq!(reason(&p, "com.google.android.apps.messaging"), Some("your messages app"));
        assert_eq!(reason(&p, "com.google.android.inputmethod.latin"), Some("your keyboard"));
        assert!(reason(&p, "com.google.android.googlequicksearchbox").unwrap().contains("voice assistant"));
        assert_eq!(p.len(), 6, "each app is listed once even when several questions name it: {p:?}");
    }

    #[test]
    fn no_default_chosen_means_protect_every_candidate_not_the_chooser() {
        let t = "@@HOME\npriority=0 preferredOrder=0 match=0x0 specificIndex=-1 isDefault=false\nandroid/com.android.internal.app.ResolverActivity\n\
@@HOMEALL\n2 activities found:\n  priority=0 isDefault=false\n  com.sec.android.app.launcher/.activities.LauncherActivity\n  priority=-1 isDefault=false\n  com.android.launcher3/.Launcher\n\
@@BROWSER\nandroid/com.android.internal.app.ResolverActivity\n";
        let p = parse(t);
        assert!(reason(&p, "com.sec.android.app.launcher").is_some() && reason(&p, "com.android.launcher3").is_some());
        assert!(reason(&p, "android").is_none(), "the system chooser is not an app to protect");
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn nothing_known_gives_nothing_and_never_panics() {
        assert!(parse("").is_empty());
        assert!(parse("@@HOME\nError: Unknown command\n@@IME\nnull\n@@ASSIST\n\n@@ROLES\ncmd: Can't find service: role\n").is_empty());
        // Role lines with an empty holder and rubbish are skipped.
        assert!(parse("@@ROLES\nandroid.app.role.SMS \nandroid.app.role.X weird.app\nnot a line at all\n").is_empty());
        // A hostile component name is not accepted as a package.
        assert!(parse("@@BROWSER\nbad name; rm -rf//x\nevil$(id)/x\n").is_empty());
    }

    #[test]
    fn roles_alone_are_enough_on_phones_where_resolve_fails() {
        let p = parse("@@ROLES\nandroid.app.role.HOME com.sec.android.app.launcher\nandroid.app.role.SMS com.samsung.android.messaging\n");
        assert_eq!(reason(&p, "com.sec.android.app.launcher"), Some("your home screen app"));
        assert_eq!(reason(&p, "com.samsung.android.messaging"), Some("your messages app"));
    }

    #[test]
    fn the_command_only_reads() {
        for bad in ["set-", "disable", "uninstall", "put ", "delete", "rm "] {
            assert!(!CMD.contains(bad), "{bad}");
        }
    }
}
