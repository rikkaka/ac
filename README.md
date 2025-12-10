This is a simple framework for algorithmic trading at arbitrary frequency. To streamline the entire process of algorithmic trading, it enables the user to collect and persist the data, write their strategies, backtest the strategies on the persisted data, and trade with their strategies in both sandbox and live environments.

In this repository, I write code for ticker-level algorithmic trading of ETC-USDT on the OKX exchange, while this framework is applicable to any exchange API.

## Quick start

With this quick start, you can collect and persist the ticker-level ETC-USDT-SWAP data from the OKX exchange, backtest an OFI-momentum strategy on these data, and run the strategy on the real market.

1. Clone the repo.
2. Create a `.env` file and fill it out according to `.env.example`.
3. Initialize the database by running `data_center\init_db.sql` for the postgresql database.
4. Collect and persist the data by running
```sh
cargo run -p data_center --bin maintain_data
```
5. When we have enough data, backtest the strategy by running
```sh
cargo run -p ac_core --bin backtest
```
6. Check the performance of the strategy in `report.csv`.
7. Run the strategy on the real market by running
```sh
cargo run -p ac_core --bin run
```

## Breakdown of the framework

### Data collection and persistence
The process of data collection and persistence includes gathering the data from the exchange API, converting them into a custom form, and save them in a database. The custom data form is defined in module [data_center\src\types.rs](data_center\src\types.rs). It is exchange-independent, allowing us to easily switch between exchanges. The OKX official API is used in module [data_center\src\okx_api.rs](data_center\src\okx_api.rs) to collect real-time data. After the collected data are transformed into the custom form, we persist them in a database. Here I use a postgresql database. The codes interacting with the database is in module [data_center\src\sql.rs](data_center\src\sql.rs).

Here is an example of ticker-level ETH-USDT swaps data from OKX in [data_center\src\bin\maintain_data.rs](data_center\src\bin\maintain_data.rs)

### Broker
The trait `Broker` in [ac_core\src\lib.rs](ac_core\src\lib.rs) represents a simulated terminal of local requests. It cope with the events from the client side (like placing an order and canceling an order) and push events from the broker side (like new market information and newly executed orders). With different implementation of `Broker`, we can construct a real trading environment or a sandbox environment. The struct `SandboxBroker` in [ac_core\src\backtest.rs](ac_core\src\backtest.rs) implements the trait `Broker` to construct a sandbox environment. It pushes the persisted data to the client and uses those data to determine the execution behavior of client orders. The `OkxBroker` in [ac_core\src\okx.rs](ac_core\src\okx.rs) uses the official API of OKX to push the live data from OKX to the client and submit the placed orders from the client to OKX.

By using different `Broker`, we can easily switch between backtesting and live trading environments for a strategy.

### Strategy
An implementation of the trait `Strategy` in [ac_core\src\strategy.rs](ac_core\src\strategy.rs) is a representation of the client side. It is triggered when a broker event arrives and it responds with client events, like places new orders or modify existing orders. Here is a simple order flow imbalance strategy in [ac_core\src\strategy\single_ticker\ofi_momentum.rs](ac_core\src\strategy\single_ticker\ofi_momentum.rs).

### Engine
The trait `Engine` in [ac_core\src\lib.rs](ac_core\src\lib.rs) streamlines the interactive between the broker and the strategy. It bridge the `Broker`, which push `BrokerEvent` and deal with `ClientEvent`, and the `Strategy`, which push `ClientEvent` and deal with `BrokerEvent`. Running an `Engine` means we start the strategy on a real or sandbox environment, depending on the kind of the broker.
