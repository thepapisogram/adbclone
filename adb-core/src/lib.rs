//! Tauri-independent ADB layer: spawns the `adb` binary and parses its output.
pub mod devices;
pub mod error;
pub mod labels;
pub mod packages;
pub mod processes;
pub mod runner;

pub use error::AdbError;
pub use runner::AdbRunner;
