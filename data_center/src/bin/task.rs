use std::{panic::AssertUnwindSafe, sync::Arc, time::Duration};

use anyhow::{Error, Ok, Result};
use data_center::{okx_api::{subscribe_trades}, sql::{insert_trade, Trade}};
use futures_util::{FutureExt, StreamExt};
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    let task = || store_trades("ETH-USDT-SWAP");
    let handle = spawn_with_retry(task, Duration::from_millis(100));
    let _ = handle.await;
}

pub fn spawn_with_retry<Fut, F>(task: F, delay: Duration) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    return tokio::spawn(async move {
        loop {
            // 执行任务（无论成功失败，都会继续）
            // task().await;
            let fut = task();
            let _ = AssertUnwindSafe(fut).catch_unwind().await;


            // 延迟后再重试
            sleep(delay).await;
        }
    });
}

async fn store_trades(instrument_id: &str) -> () {
    let mut ws_stream = subscribe_trades(instrument_id).await.unwrap();
    while let Some(data) = ws_stream.next().await {
        dbg!(&data);
        let trade: Trade = data.into();
        insert_trade(&trade).await.unwrap();
    }
}
