use std::time::Duration;

use anyhow::Result;
use data_center::{
    utils::Heartbeat, 
};
use futures_util::StreamExt;
use tokio_tungstenite::connect_async;

#[tokio::main]
async fn main() {
    let _guard = utils::init_tracing();
    let handle = utils::spawn_with_retry(main_task, Duration::from_millis(100));
    let _ = handle.await;
}

async fn main_task() -> Result<()> {
    let (okx_ws, _) = connect_async("wss://ws.okx.com:8443/ws/v5/public").await?;
    let mut okx_ws = Heartbeat::new(okx_ws, Duration::from_secs(1), Duration::from_millis(100));

    while let Some(msg) = okx_ws.next().await {
        
        let _ = dbg!(msg);
    }

    anyhow::bail!("WebSocket stream closed");
}