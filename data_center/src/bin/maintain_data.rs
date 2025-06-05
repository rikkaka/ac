use std::time::Duration;

use anyhow::Result;
use data_center::{
    okx_api::{
        self, OkxWsEndpoint,
        types::{Data, SubscribeArg},
    },
    sql,
    types::Trade,
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
    let mut subscribe_args = vec![];
    for inst_id in INSTRUMENTS {
        subscribe_args.push(SubscribeArg::new_trades(inst_id));
        subscribe_args.push(SubscribeArg::new_bbo_tbt(inst_id))
    }
    let mut okx_ws = okx_api::connect(OkxWsEndpoint::Public, subscribe_args).await?;

    while let Some(data) = okx_ws.next().await {
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
            Data::BboTbt(inst_id, data) => {
                let Ok(bbo) = data.try_into_bbo(inst_id) else {
                    tracing::error!("Failed to parse bbo data");
                    continue;
                };
                if let Err(e) = sql::insert_bbo(&bbo).await {
                    tracing::error!("Failed to insert bbo data: {e}");
                }
            }
            _ => unreachable!(),
        }
    }

    Ok(())
}
