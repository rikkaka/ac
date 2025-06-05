use std::time::Duration;

use anyhow::Result;
use futures::{Sink, Stream};
use pin_project::pin_project;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
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

pub trait Duplex<I, IE, O>: Sink<I, Error = IE> + Stream<Item = O> + Unpin {}
impl<T, I, IE, O> Duplex<I, IE, O> for T where T: Sink<I, Error = IE> + Stream<Item = O> + Unpin {}

pub trait Timestamped {
    fn get_ts(&self) -> i64;
}

///合并两个实现Timestamped的Stream并按ts顺序发出。ts相等时优先stream1。
#[pin_project]
pub struct TsStreamMerger<S1, S2, T1, T2, T> {
    #[pin]
    stream1: S1,
    #[pin]
    stream2: S2,
    buffer1: Option<T1>,
    buffer2: Option<T2>,
    stream1_ended: bool,
    stream2_ended: bool,
    _marker: PhantomData<T>,
}

impl<S1, S2, T1, T2, T> TsStreamMerger<S1, S2, T1, T2, T>
where
    S1: Stream<Item = T1>,
    S2: Stream<Item = T2>,
    T1: Timestamped,
    T2: Timestamped,
    T: From<T1> + From<T2>,
{
    pub fn new(stream1: S1, stream2: S2) -> Self {
        Self {
            stream1,
            stream2,
            buffer1: None,
            buffer2: None,
            stream1_ended: false,
            stream2_ended: false,
            _marker: PhantomData,
        }
    }
}

impl<S1, S2, T1, T2, T> Stream for TsStreamMerger<S1, S2, T1, T2, T>
where
    S1: Stream<Item = T1>,
    S2: Stream<Item = T2>,
    T1: Timestamped,
    T2: Timestamped,
    T: From<T1> + From<T2>,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        // Try to get an item from stream1 if we don't have one cached and stream hasn't ended
        if this.buffer1.is_none() && !*this.stream1_ended {
            match this.stream1.poll_next(cx) {
                Poll::Ready(Some(item)) => *this.buffer1 = Some(item),
                Poll::Ready(None) => *this.stream1_ended = true,
                Poll::Pending => return Poll::Pending,
            }
        }

        // Try to get an item from stream2 if we don't have one cached and stream hasn't ended
        if this.buffer2.is_none() && !*this.stream2_ended {
            match this.stream2.poll_next(cx) {
                Poll::Ready(Some(item)) => *this.buffer2 = Some(item),
                Poll::Ready(None) => *this.stream2_ended = true,
                Poll::Pending => return Poll::Pending,
            }
        }

        // Compare timestamps and return items in order
        match (&this.buffer1, &this.buffer2) {
            (Some(item1), Some(item2)) => {
                if item1.get_ts() <= item2.get_ts() {
                    Poll::Ready(Some(T::from(this.buffer1.take().unwrap())))
                } else {
                    Poll::Ready(Some(T::from(this.buffer2.take().unwrap())))
                }
            }
            (Some(_), None) => Poll::Ready(Some(T::from(this.buffer1.take().unwrap()))),
            (None, Some(_)) => Poll::Ready(Some(T::from(this.buffer2.take().unwrap()))),
            (None, None) => {
                if *this.stream1_ended && *this.stream2_ended {
                    Poll::Ready(None)
                } else {
                    Poll::Pending
                }
            }
        }
    }
}
