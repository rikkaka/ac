use std::path::Path;

use ac_core::{
    Engine,
    backtest::{SandboxBroker, TransactionCostModel},
    data::okx::get_bbo_history_provider,
    strategy::single_ticker::ofi_momentum::OfiMomentumArgs,
};
use arrayvec::ArrayString;
use chrono::Duration;

#[tokio::main]
async fn main() {
    let instrument_id = ArrayString::try_from("ETH-USDT-SWAP").unwrap();
    let instruments = vec![instrument_id];

    let data_provider = get_bbo_history_provider(instruments.clone(), Duration::hours(12));

    let strategy_args = OfiMomentumArgs {
        instrument_id,
        window_ofi: Duration::minutes(1),
        window_ema: Duration::minutes(10),
        holding_duration: Duration::seconds(30),
        theta: 3.,
        notional: 10_000.,
        size_digits: 2,
        order_id_offset: 0,
    };
    let strategy = strategy_args.into_strategy();

    let transaction_cost_model = TransactionCostModel::new_okx(0.);
    let broker = SandboxBroker::new(
        instruments,
        data_provider,
        100_000.,
        transaction_cost_model,
        Duration::minutes(1),
    )
    .await;

    let mut engine = Engine::new(broker, strategy);
    engine.run().await;

    let broker = engine.broker();
    let reporter = broker.reporter();
    reporter.to_csv(Path::new("./report.csv")).unwrap();
}
