//! What goes into the log file. The level comes from Settings > Logs and can change while the
//! app runs: the logger is built to accept everything, and [`allows`] filters by the current
//! level on each record.

use std::sync::atomic::{AtomicU8, Ordering};

use log::{LevelFilter, Metadata};
use quay_config::LogLevel;

static LEVEL: AtomicU8 = AtomicU8::new(0);

const LEVELS: [LogLevel; 4] = [
    LogLevel::Error,
    LogLevel::Info,
    LogLevel::Debug,
    LogLevel::Trace,
];

fn current() -> LogLevel {
    LEVELS[LEVEL.load(Ordering::Relaxed) as usize % LEVELS.len()]
}

/// The most verbose level let through for (Quay Term's own crates, the SSH library, everything
/// else). Libraries stay one step quieter than the app so the log is not drowned out.
fn limits(level: LogLevel) -> [LevelFilter; 3] {
    use LevelFilter::*;
    match level {
        LogLevel::Error => [Error, Error, Error],
        LogLevel::Info => [Info, Warn, Warn],
        LogLevel::Debug => [Debug, Info, Warn],
        // Not `Trace` for russh: at that level it dumps packet buffers.
        LogLevel::Trace => [Trace, Debug, Info],
    }
}

fn limit_for(target: &str, level: LogLevel) -> LevelFilter {
    let [ours, ssh, rest] = limits(level);
    if target.starts_with("quay") {
        ours
    } else if target.starts_with("russh") {
        ssh
    } else {
        rest
    }
}

/// Use `level` from now on.
pub fn set(level: LogLevel) {
    let i = LEVELS.iter().position(|l| *l == level).unwrap_or(0);
    LEVEL.store(i as u8, Ordering::Relaxed);
    // Lets the `log!` macros skip records nobody will write, before formatting them.
    let max = limits(level).into_iter().max().unwrap_or(LevelFilter::Error);
    log::set_max_level(max);
}

/// The logger's filter: whether this record is written at the current level.
pub fn allows(m: &Metadata) -> bool {
    m.level() <= limit_for(m.target(), current())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libraries_stay_quieter_than_the_app() {
        assert_eq!(limit_for("quay_app", LogLevel::Error), LevelFilter::Error);
        assert_eq!(limit_for("russh::client", LogLevel::Error), LevelFilter::Error);
        assert_eq!(limit_for("quay_ssh::client", LogLevel::Debug), LevelFilter::Debug);
        assert_eq!(limit_for("russh::client", LogLevel::Debug), LevelFilter::Info);
        assert_eq!(limit_for("tao::window", LogLevel::Debug), LevelFilter::Warn);
        assert_eq!(limit_for("russh::client", LogLevel::Trace), LevelFilter::Debug);
    }
}
