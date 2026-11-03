//! Tauri-independent ADB layer: spawns the `adb` binary and parses its output.
pub mod bridge;
pub mod deviceinfo;
pub mod devices;
pub mod error;
pub mod health;
pub mod labels;
pub mod packages;
pub mod processes;
pub mod report;
pub mod runner;
pub mod snapshot;
pub mod tuning;
pub mod wireless;

pub use error::AdbError;
pub use runner::{AdbRunner, Output};
