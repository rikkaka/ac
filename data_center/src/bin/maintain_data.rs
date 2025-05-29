use std::{sync::{atomic::{AtomicU64, Ordering}, Arc}, time::Duration};

use anyhow::Result;
use data_center::{
    okx_api::{self, subscribe, types::Data, OkxWsEndpoint},
    sql, 
};
use futures_util::StreamExt;

static INSTRUMENTS: [&str; 1] = ["ETH-USDT-SWAP"];

#[tokio::main]
async fn main() {
    let _guard = utils::init_tracing();
    let handle = utils::spawn_with_retry(main_task, Duration::from_millis(0));
    let _ = handle.await;
}

async fn main_task() -> Result<()> {
    let mut okx_ws = okx_api::connect(OkxWsEndpoint::Public).await?;
    for inst_id in INSTRUMENTS {
        subscribe(&mut okx_ws, "trades", inst_id).await?;
        subscribe(&mut okx_ws, "bbo-tbt", inst_id).await?;
    }
    let last_data_ts = Arc::new(AtomicU64::new(0));

    while let Some((instrument_id, data)) = okx_ws.next().await {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        last_data_ts.store(now, Ordering::Relaxed);

        match data {
            Data::Trades(data) => {
                let Ok(trade) = data.try_into_trade() else {
                    tracing::error!("Failed to parse trade data");
                    continue;
                };
                if let Err(e) = sql::insert_trade(&trade).await {
                    tracing::error!("Failed to insert trade data: {e}");
                }
            }
            Data::BboTbt(data) => {
                let Ok(bbo) = data.try_into_bbo(instrument_id) else {
                    tracing::error!("Failed to parse bbo data");
                    continue;
                };
                if let Err(e) = sql::insert_bbo(&bbo).await {
                    tracing::error!("Failed to insert bbo data: {e}");
                }
            }
        }
    }

    let last = last_data_ts.load(Ordering::Relaxed);
    let now = chrono::Utc::now().timestamp_millis() as u64;
    anyhow::bail!("WebSocket stream closed. Last data timestamp: {last}, now: {now}");
}
