use std::time::Duration;

use anyhow::Result;
use data_center::{
    okx_api::{OkxWsEndpoint, OkxWsStream, types::Data},
    sql,
};
use futures_util::StreamExt;

static INSTRUMENTS: [&str; 1] = ["ETH-USDT-SWAP"];

#[tokio::main]
async fn main() {
    let _guard = utils::init_tracing();
    let handle = utils::spawn_with_retry(main_task, Duration::from_millis(100));
    let _ = handle.await;
}

async fn main_task() -> Result<()> {
    let mut okx_ws = OkxWsStream::connect(OkxWsEndpoint::Public).await?;
    for inst_id in INSTRUMENTS {
        okx_ws.subscribe("trades", inst_id).await?;
        okx_ws.subscribe("bbo-tbt", inst_id).await?;
    }

    while let Some((instrument_id, data)) = okx_ws.next().await {
        match data {
            Data::Trades(data) => {
                let Ok(trade) = data.try_into_trade() else {
                    tracing::error!("Failed to parse trade data");
                    continue;
                };
                sql::insert_trade(&trade).await?;
            }
            Data::BboTbt(data) => {
                let Ok(bbo) = data.try_into_bbo(instrument_id) else {
                    tracing::error!("Failed to parse bbo data");
                    continue;
                };
                sql::insert_bbo(&bbo).await?;
            }
        }
    }

    anyhow::bail!("WebSocket stream closed");
}
