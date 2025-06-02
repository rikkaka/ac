use std::{cmp::Reverse, collections::BinaryHeap, path::Path};

use ac_core::{
    Engine,
    backtest::{SandboxBroker, TransactionCostModel},
    data::okx::get_bbo_history_provider,
    strategy::single_ticker::ofi_momentum::OfiMomentumArgs,
};
use arrayvec::ArrayString;
use chrono::Duration;
use futures::{pin_mut, Stream, StreamExt};
use ordered_float::OrderedFloat;

#[tokio::main]
async fn main() {
    let instrument_id = ArrayString::try_from("ETH-USDT-SWAP").unwrap();
    let instruments = vec![instrument_id];
    let data_provider = get_bbo_history_provider(instruments.clone(), Duration::hours(2400));
    let spread_stream = data_provider.map(|bbo| OrderedFloat(bbo.ask_price - bbo.bid_price));
    let top_spreads = top_100(spread_stream).await;
    dbg!(top_spreads);
}

async fn top_100<I>(stream: I) -> Vec<OrderedFloat<f64>>
where
    I: Stream<Item = OrderedFloat<f64>>,
{
    let mut heap = BinaryHeap::with_capacity(100);
    pin_mut!(stream);
    while let Some(val) = stream.next().await {
        if heap.len() < 100 {
            heap.push(Reverse(val));
        } else if val > heap.peek().unwrap().0 {
            heap.pop();
            heap.push(Reverse(val));
        }
    }

    let mut result: Vec<_> = heap.into_iter().map(|r| r.0).collect();
    result
}