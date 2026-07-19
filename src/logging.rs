use std::time::{Duration, SystemTime};
use std::{env, path::Path};
use tracing_appender::non_blocking::WorkerGuard;

/// Must be kept alive for the whole program - dropping it stops file writes.
pub struct LoggingGuard{
    _guard: Option<WorkerGuard>
}

fn cleanup_old_logs(dir: &str, max_age_days: u64) {
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);
    let now = SystemTime::now();

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let modified = match entry.metadata().and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if let Ok(age) = now.duration_since(modified) {
            if age > max_age {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

pub fn init(verbose: bool) -> LoggingGuard {
    let level = if verbose { "debug" } else { "info" };

    if cfg!(debug_assertions) {
        tracing_subscriber::fmt().with_env_filter(level).init();

        LoggingGuard{ _guard: None }
    } else {
        let logs_dir: String = env::var("LOGGING_DIR").ok().unwrap_or("logs".to_string());
        let days_to_keep: u64 = env::var("LOGGING_DAYS_TO_KEEP")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        cleanup_old_logs(&logs_dir, days_to_keep);

        let exe = env::args().next().unwrap();
        let executable_name = Path::new(&exe)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let file_appender = tracing_appender::rolling::daily(logs_dir, executable_name);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        tracing_subscriber::fmt()
            .with_env_filter(level)
            .with_writer(non_blocking)
            .with_ansi(false)
            .init();

        LoggingGuard{ _guard: Some(guard) }
    }
}
