//! Rolling file logging in the config folder.

use std::path::Path;

use anyhow::{Context, Result};
use kile_wheel_core::names::APP_NAME;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;

const LOG_SUFFIX: &str = "log";
const KEPT_LOG_FILES: usize = 7;
const DEFAULT_FILTER: &str = "info";

/// Raise the log level without a rebuild, e.g. `KILE_WHEEL_LOG=debug`.
const FILTER_ENV: &str = "KILE_WHEEL_LOG";

/// Writes stop when the returned guard drops, so hold it for the process lifetime.
pub fn start(logs_dir: &Path) -> Result<WorkerGuard> {
    std::fs::create_dir_all(logs_dir).with_context(|| {
        format!(
            "could not create the log folder {}; check the config folder is writable",
            logs_dir.display()
        )
    })?;

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(APP_NAME)
        .filename_suffix(LOG_SUFFIX)
        .max_log_files(KEPT_LOG_FILES)
        .build(logs_dir)
        .with_context(|| {
            format!(
                "could not open a log file in {}; check the folder is writable",
                logs_dir.display()
            )
        })?;

    let (writer, guard) = tracing_appender::non_blocking(appender);

    tracing_subscriber::fmt()
        .with_env_filter(filter())
        .with_writer(writer)
        .with_ansi(false)
        .init();

    Ok(guard)
}

/// A malformed filter falls back to the default, because logging must not be the
/// reason the app refuses to start.
fn filter() -> EnvFilter {
    EnvFilter::try_from_env(FILTER_ENV).unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
}
