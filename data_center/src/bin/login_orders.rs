use std::vec;

use data_center::{
    okx_api::{self, OkxWsEndpoint},
    types::{Action, InstId, Side},
};
use futures::{SinkExt, StreamExt};

#[tokio::main]
async fn main() {
    let _guard = utils::init_tracing();
    let subscribe_actions = vec![Action::SubscribeOrders(InstId::EthUsdtSwap)];
    let mut okx_ws = okx_api::connect(OkxWsEndpoint::PrivateSimu, subscribe_actions)
        .await
        .unwrap();

    okx_ws
        .send(Action::LimitOrder {
            request_id: "123".into(),
            side: Side::Buy,
            inst_id: InstId::EthUsdtSwap,
            client_order_id: "123".into(),
            size: "0.1".into(),
            price: "100".into(),
        })
        .await
        .unwrap();

    okx_ws
        .send(Action::CancelOrder {
            request_id: "123".into(),
            inst_id: InstId::EthUsdtSwap,
            client_order_id: "123".into(),
        })
        .await
        .unwrap();

    while let Some(data) = okx_ws.next().await {
        dbg!(data);
    }
}
