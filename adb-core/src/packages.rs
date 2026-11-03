use crate::{AdbError, AdbRunner};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Package {
    pub name: String,
    pub apk_path: Option<String>,
    pub uid: Option<u32>,
}

/// Parse `pm list packages -f -U` lines:
/// `package:/data/app/~~x/base.apk=com.foo uid:10123`
pub fn parse_packages(output: &str) -> Vec<Package> {
    output
        .lines()
        .filter_map(|l| l.trim().strip_prefix("package:"))
        .map(|rest| {
            let (main, uid) = match rest.split_once(" uid:") {
                Some((m, u)) => (m, u.split_whitespace().next().and_then(|s| s.parse().ok())),
                None => (rest, None),
            };
            // The apk path may contain '=', the package name never does: split on the last one.
            match main.rsplit_once('=') {
                Some((path, name)) => Package {
                    name: name.to_string(),
                    apk_path: Some(path.to_string()),
                    uid,
                },
                None => Package { name: main.to_string(), apk_path: None, uid },
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub enum Filter {
    All,
    System,
    ThirdParty,
    Disabled,
    Enabled,
}

pub async fn list_packages(
    adb: &AdbRunner,
    serial: &str,
    filter: Filter,
    include_uninstalled: bool,
) -> Result<Vec<Package>, AdbError> {
    let mut args = vec!["pm", "list", "packages", "-f", "-U", "--user", "0"];
    match filter {
        Filter::All => {}
        Filter::System => args.push("-s"),
        Filter::ThirdParty => args.push("-3"),
        Filter::Disabled => args.push("-d"),
        Filter::Enabled => args.push("-e"),
    }
    if include_uninstalled {
        args.push("-u");
    }
    Ok(parse_packages(&adb.shell(serial, &args).await?))
}

/// Validate a package name before interpolating it into a shell command.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
}

async fn pm_action(adb: &AdbRunner, serial: &str, args: &[&str], pkg: &str) -> Result<(), AdbError> {
    if !valid_name(pkg) {
        return Err(AdbError::Device(format!("invalid package name: {pkg}")));
    }
    let mut full = args.to_vec();
    full.push(pkg);
    let out = adb.shell(serial, &full).await?;
    // `pm` often exits 0 but prints Failure / Exception on stdout.
    if out.contains("Failure") || out.contains("Exception") || out.contains("Error") {
        return Err(AdbError::Device(out.trim().to_string()));
    }
    Ok(())
}

pub async fn disable(adb: &AdbRunner, s: &str, pkg: &str) -> Result<(), AdbError> {
    pm_action(adb, s, &["pm", "disable-user", "--user", "0"], pkg).await
}
pub async fn enable(adb: &AdbRunner, s: &str, pkg: &str) -> Result<(), AdbError> {
    pm_action(adb, s, &["pm", "enable", "--user", "0"], pkg).await
}
pub async fn uninstall_for_user(adb: &AdbRunner, s: &str, pkg: &str) -> Result<(), AdbError> {
    pm_action(adb, s, &["pm", "uninstall", "-k", "--user", "0"], pkg).await
}
pub async fn force_stop(adb: &AdbRunner, s: &str, pkg: &str) -> Result<(), AdbError> {
    pm_action(adb, s, &["am", "force-stop"], pkg).await
}
pub async fn restore(adb: &AdbRunner, s: &str, pkg: &str) -> Result<(), AdbError> {
    pm_action(adb, s, &["cmd", "package", "install-existing", "--user", "0"], pkg).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_with_uid() {
        let out = "package:/system/app/Foo/Foo.apk=com.vendor.foo uid:10050\n\
package:/data/app/~~abc==/com.bar-xyz==/base.apk=com.bar uid:10123\n";
        let p = parse_packages(out);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].name, "com.vendor.foo");
        assert_eq!(p[0].uid, Some(10050));
        assert_eq!(p[1].name, "com.bar");
        assert_eq!(p[1].apk_path.as_deref(), Some("/data/app/~~abc==/com.bar-xyz==/base.apk"));
    }

    #[test]
    fn parses_without_path() {
        let p = parse_packages("package:com.plain\n");
        assert_eq!(p[0].name, "com.plain");
        assert!(p[0].apk_path.is_none());
    }

    #[test]
    fn rejects_shell_injection() {
        assert!(!valid_name("com.a; rm -rf /"));
        assert!(valid_name("com.google.android_gms"));
    }
}
