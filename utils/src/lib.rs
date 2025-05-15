use std::time::Duration;

use anyhow::Result;
use tokio::time::sleep;
use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, prelude::*};

pub fn init_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    // ① 滚动文件（按天）
    let file_appender = rolling::daily("./logs", "log");

    // ② 非阻塞 writer + 后台线程
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // ③ 终端输出层
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_writer(std::io::stdout);

    // ④ 文件层（禁掉 ANSI，防止控制字符写进文件）
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking);

    // ⑤ 组合全局 Subscriber
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env()) // 支持 RUST_LOG=info,my_crate=debug
        .with(stdout_layer)
        .with(file_layer)
        .init();

    guard // 别忘了把 guard 保存在 main 里！
}

pub fn spawn_with_retry<Fut, F>(task: F, delay: Duration) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            // 执行任务
            if let Err(e) = task().await {
                tracing::error!("Task failed: {:?}", e);
            }

            // 延迟后再重试
            sleep(delay).await;
        }
    })
}
