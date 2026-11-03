use crate::error::AdbError;
use std::{path::PathBuf, time::Duration};
use tokio::process::Command;

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
        let mut cmd = Command::new(&self.adb_path);
        cmd.args(args).kill_on_drop(true);
        let out = tokio::time::timeout(self.timeout, cmd.output())
            .await
            .map_err(|_| AdbError::Timeout(self.timeout))??;
        if !out.status.success() {
            return Err(AdbError::Failed {
                code: out.status.code(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(out.stdout)
    }

    /// Run `adb -s <serial> shell <args...>`.
    pub async fn shell(&self, serial: &str, args: &[&str]) -> Result<String, AdbError> {
        let mut full = vec!["-s", serial, "shell"];
        full.extend_from_slice(args);
        self.run(&full).await
    }
}
