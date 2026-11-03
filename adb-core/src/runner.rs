use crate::error::AdbError;
use std::{path::PathBuf, time::Duration};
use tokio::process::Command;

/// Windows flag: do not create a console window for the child process.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `adb.exe` is a console program. A windowless GUI app that starts one without this flag makes Windows
/// open a console window for every single call, and this app calls adb constantly. Every place that
/// starts a process must go through here.
fn quiet(cmd: &mut Command) {
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Everything a finished adb process produced.
#[derive(Debug, Clone)]
pub struct Output {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Thin async wrapper around the `adb` executable.
#[derive(Clone, Debug)]
pub struct AdbRunner {
    adb_path: PathBuf,
    timeout: Duration,
}

impl AdbRunner {
    pub fn new(adb_path: impl Into<PathBuf>) -> Self {
        Self { adb_path: adb_path.into(), timeout: Duration::from_secs(30) }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Run `adb <args...>` and return stdout.
    pub async fn run(&self, args: &[&str]) -> Result<String, AdbError> {
        Ok(String::from_utf8_lossy(&self.run_bytes(args).await?).into_owned())
    }

    /// Like `run`, but returns raw stdout (for binary data such as `exec-out`).
    pub async fn run_bytes(&self, args: &[&str]) -> Result<Vec<u8>, AdbError> {
        let out = self.run_full(args).await?;
        if out.code != Some(0) {
            return Err(AdbError::Failed { code: out.code, stderr: out.stderr.trim().to_string() });
        }
        Ok(out.stdout)
    }

    /// Run and return everything, including stderr and the exit code. Errors only if adb could not be
    /// started or took too long: a non-zero exit is reported in the result, not as an error.
    pub async fn run_full(&self, args: &[&str]) -> Result<Output, AdbError> {
        let mut attempt = 0;
        loop {
            let mut cmd = Command::new(&self.adb_path);
            cmd.args(args).kill_on_drop(true);
            quiet(&mut cmd);
            match tokio::time::timeout(self.timeout, cmd.output()).await.map_err(|_| AdbError::Timeout(self.timeout))? {
                Ok(out) => {
                    return Ok(Output { code: out.status.code(), stdout: out.stdout, stderr: String::from_utf8_lossy(&out.stderr).into_owned() })
                }
                // A just-written executable can be briefly "busy" while another thread is spawning (Linux).
                Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && attempt < 5 => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Start the adb server now (hidden), so it is not started lazily by some later call.
    pub async fn start_server(&self) -> Result<(), AdbError> {
        self.run(&["start-server"]).await.map(|_| ())
    }

    /// Run `adb -s <serial> shell <args...>`.
    pub async fn shell(&self, serial: &str, args: &[&str]) -> Result<String, AdbError> {
        let mut full = vec!["-s", serial, "shell"];
        full.extend_from_slice(args);
        self.run(&full).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_window_flag_matches_the_documented_windows_value() {
        assert_eq!(CREATE_NO_WINDOW, 134_217_728, "CREATE_NO_WINDOW is 0x08000000");
    }

    #[tokio::test]
    async fn quiet_does_not_break_spawning() {
        // Runs on every platform; on Windows this exercises the flagged spawn path.
        let echo = if cfg!(windows) { "cmd" } else { "echo" };
        let args: Vec<&str> = if cfg!(windows) { vec!["/c", "echo", "hi"] } else { vec!["hi"] };
        let out = AdbRunner::new(echo).run(&args).await.unwrap();
        assert!(out.contains("hi"));
    }
}
