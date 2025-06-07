use ac_core::InstId;
use ac_core::okx::OkxBroker;
use ac_core::{Engine, strategy::single_ticker::ofi_momentum::OfiMomentumArgs};
use chrono::Duration;

#[tokio::main]
async fn main() {
    let _guard = utils::init_tracing();

    let instrument_id = InstId::EthUsdtSwap;

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

    let broker = OkxBroker::new_bbo(instrument_id, Duration::minutes(240)).await;

    let mut engine = Engine::new(broker, strategy);
    engine.run().await;
}
