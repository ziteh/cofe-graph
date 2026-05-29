use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;

/// Returns a WorkerGuard that MUST be kept alive for the program's lifetime
/// dropping it early flushes and closes the non-blocking writer
pub fn init(log_dir: &Path, quiet: bool) -> WorkerGuard {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

    std::fs::create_dir_all(log_dir).expect("cannot create log dir");

    let file_appender = tracing_appender::rolling::daily(log_dir, "log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false);

    let stderr_layer =
        (!quiet).then(|| fmt::layer().with_writer(std::io::stderr).with_target(false));

    tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .init();

    guard
}

pub use tracing::{debug, error, info, trace, warn};
