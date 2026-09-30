//! 日志初始化：文件全量 + 屏幕仅 INFO

use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

pub fn init_logging() -> WorkerGuard {
    let file_appender = tracing_appender::rolling::daily("logs", "crazy_ap.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_target(false)
        .with_ansi(false);

    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(stdout_layer)
        .init();

    info!("Logging initialized: screen=INFO+, file=TRACE+ at logs/crazy_ap.log");
    guard
}
