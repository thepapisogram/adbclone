//! Tauri-independent ADB layer: spawns the `adb` binary and parses its output.
pub mod backup;
pub mod bridge;
pub mod deviceinfo;
pub mod devices;
pub mod error;
pub mod health;
pub mod icons_apk;
pub mod install;
pub mod labels;
pub mod log;
pub mod mirror;
pub mod packages;
pub mod platform_tools;
pub mod processes;
pub mod protect;
pub mod report;
pub mod runner;
pub mod snapshot;
pub mod storage;
pub mod wakelocks;
pub mod tuning;
pub mod wireless;

pub use error::AdbError;
pub use runner::{AdbRunner, Output};
