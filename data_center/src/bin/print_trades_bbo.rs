use std::{sync::{atomic::{AtomicU64, Ordering}, Arc}};

use anyhow::Result;
use data_center::{
    okx_api::{self, subscribe, types::Data, OkxWsEndpoint},
};
use futures_util::StreamExt;

static INSTRUMENTS: [&str; 1] = ["ETH-USDT-SWAP"];

#[tokio::main]
async fn main() -> Result<()> {
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
                    continue;
                };
                dbg!(trade);
            }
            Data::BboTbt(data) => {
                let Ok(bbo) = data.try_into_bbo(instrument_id) else {
                    continue;
                };
                dbg!(bbo);
            }
        }
    }

    Ok(())
}
