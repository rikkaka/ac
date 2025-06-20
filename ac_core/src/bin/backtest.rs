use std::path::Path;

use ac_core::InstId;
use ac_core::{
    Engine,
    backtest::{SandboxBroker, TransactionCostModel},
    data::okx::get_bbo_history_provider,
    strategy::single_ticker::ofi_momentum::OfiMomentumArgs,
};
use chrono::Duration;

#[tokio::main]
async fn main() {
    let _guard = utils::init_tracing();

    let instrument_id = InstId::EthUsdtSwap;
    let instruments = vec![instrument_id];

    let data_provider = get_bbo_history_provider(instruments.clone(), Duration::hours(2400));

    let strategy_args = OfiMomentumArgs {
        instrument_id,
        window_ofi: Duration::minutes(8),
        window_ema: Duration::minutes(240),
        holding_duration: Duration::seconds(200),
        event_interval: Duration::seconds(1),
        theta: 5.,
        notional: 100_000.,
        price_offset: 0.,
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
    let sharpe = reporter.sharpe_ratio();
    println!("sharpe: {sharpe:?}");
    reporter.to_csv(Path::new("./report.csv")).unwrap();
}
