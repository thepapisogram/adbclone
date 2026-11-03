use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdbError {
    #[error("adb binary not found or failed to start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("adb command timed out after {0:?}")]
    Timeout(std::time::Duration),
    #[error("adb exited with status {code:?}: {stderr}")]
    Failed { code: Option<i32>, stderr: String },
    #[error("device command failed: {0}")]
    Device(String),
}
