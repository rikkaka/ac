//! 为进行回测，我们需要拿收集的数据，创造模拟的交易环境。
//! 例如，一个策略若需要trade+bbo的数据，那么回测时也应该ts by ts的接收trade + bbo的数据。
//! 这个mod的基本功能，是对于 数据的提供者 和 strategy ，计算strategy的表现。
//! A strategy receives data and returns orders. Thus this mod need to simulate
//! an environment where the results of the sequence of orders can be evaluated.
use std::{
    collections::VecDeque,
    fmt::Debug,
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};

use anyhow::Result;
use chrono::Duration;
use futures::{Sink, Stream, StreamExt, ready};
use pin_project::pin_project;
use rustc_hash::FxHashMap;
use serde::Serialize;
use statrs::statistics::Statistics;

use crate::{
    Broker, BrokerEvent, ClientEvent, DataProvider, ExecType, Fill, FillState, InstId, LimitOrder,
    MarketOrder, Order, OrderId, Portfolio, Timestamp, data::Bbo,
};

#[pin_project]
pub struct SandboxBroker<DP, D, M> {
    limit_orders: FxHashMap<OrderId, LimitOrder>,
    broker_events_buf: VecDeque<BrokerEvent<D>>,
    inst_matcher: FxHashMap<InstId, M>,
    #[pin]
    data_provider: DP,

    ts: Timestamp,

    cash: f64,
    transaction_cost_model: TransactionCostModel,
    portfolio: Portfolio,
    reporter: Reporter,
}

impl<DP, D, M> SandboxBroker<DP, D, M>
where
    DP: DataProvider<D>,
    D: MarketData<M>,
    M: MatchOrder,
{
    pub async fn new(
        instruments: Vec<InstId>,
        mut data_provider: DP,
        cash: f64,
        transaction_cost_model: TransactionCostModel,
        report_frequency: Duration,
    ) -> Self {
        let mut inst_matcher = FxHashMap::default();
        let mut ts = 0;
        while inst_matcher.len() < instruments.len() {
            if let Some(data) = data_provider.next().await {
                if let Some(matcher) = data.draw_matcher() {
                    ts = matcher.get_ts();
                    inst_matcher.insert(matcher.instrument_id(), matcher);
                }
            } else {
                tracing::error!("No enough data from the data provider");
            }
        }

        let mut reporter = Reporter::new(report_frequency);
        reporter.insert(ts, cash);

        Self {
            limit_orders: Default::default(),
            broker_events_buf: Default::default(),
            inst_matcher,
            data_provider,
            ts,
            cash,
            transaction_cost_model,
            portfolio: Portfolio::new(),
            reporter,
        }
    }

    pub fn reporter(&self) -> &Reporter {
        &self.reporter
    }

    fn on_fill(&mut self, fill: &Fill) {
        let cost = self.transaction_cost_model.calculate_cost(fill);
        self.cash -= cost;
        if fill.side {
            self.cash -= fill.price * fill.filled_size;
        } else {
            self.cash += fill.price * fill.filled_size;
        }

        self.portfolio.update(fill);
        let total_value = self.get_total_value();
        self.reporter.insert(self.ts, total_value);
        dbg!(fill);
    }

    pub fn on_data(&mut self, new_data: D) {
        self.ts = new_data.get_ts();
        if let Some(matcher) = new_data.draw_matcher() {
            self.inst_matcher.insert(matcher.instrument_id(), matcher);
            // 若有新的MatchOrder，尝试匹配所有的限价单。
            self.try_fill_placed_orders();
        }
    }

    /// 遍历所有挂单并检查能否成交；将成交的挂单推入事件并移除
    pub fn try_fill_placed_orders(&mut self) {
        let filled_orders: Vec<_> = self
            .limit_orders
            .iter()
            .filter_map(|(order_id, order)| {
                MatchOrder::try_fill_limit_order(&self.inst_matcher, order, ExecType::Maker)
                    .map(|fill| (*order_id, fill))
            })
            .collect();

        // 将成交的挂单推入事件并移除
        filled_orders.into_iter().for_each(|(order_id, fill)| {
            self.limit_orders.remove(&order_id);
            self.on_fill(&fill);
            self.broker_events_buf.push_back(BrokerEvent::Fill(fill));
        })
    }

    pub fn get_total_value(&self) -> f64 {
        let inst_price = M::get_inst_market_price(&self.inst_matcher);
        self.portfolio.get_value(&inst_price) + self.cash
    }
}

impl<DP, D, M> Sink<Vec<ClientEvent>> for SandboxBroker<DP, D, M>
where
    DP: DataProvider<D>,
    D: MarketData<M>,
    M: MatchOrder,
{
    type Error = anyhow::Error;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Vec<ClientEvent>) -> Result<(), Self::Error> {
        let this = self.get_mut();
        this.on_client_events(item.into_iter());
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

impl<DP, D, M> Stream for SandboxBroker<DP, D, M>
where
    DP: DataProvider<D>,
    D: MarketData<M>,
    M: MatchOrder,
{
    type Item = BrokerEvent<D>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();

        // 若buf中尚有未推送的事件，则推送
        if let Some(event) = this.broker_events_buf.pop_front() {
            return Poll::Ready(Some(event));
        }

        // 获取最新的Bbo数据并更新字段，同时检查挂单能否被fill。将新的fill的挂单与Bbo放入buf中，并推送buf的第一条数据。
        if let Some(data) = ready!(this.data_provider.as_mut().poll_next(cx)) {
            self.on_data(data.clone());
            self.broker_events_buf.push_back(BrokerEvent::Data(data));

            let event = self.broker_events_buf.pop_front().unwrap();

            Poll::Ready(Some(event))
        } else {
            let total_value = self.get_total_value();
            let ts = self.ts;
            self.reporter.insert(ts, total_value);
            self.reporter.end();
            Poll::Ready(None)
        }
    }
}

impl<DP, D, M> Broker<D> for SandboxBroker<DP, D, M>
where
    DP: DataProvider<D>,
    D: MarketData<M>,
    M: MatchOrder,
{
    fn on_client_event(&mut self, client_event: ClientEvent) {
        match client_event {
            ClientEvent::PlaceOrder(order) => match order {
                Order::Market(order) => {
                    let fill = MatchOrder::fill_market_order(&self.inst_matcher, &order);
                    self.on_fill(&fill);
                    self.broker_events_buf.push_back(BrokerEvent::Fill(fill));
                }
                Order::Limit(order) => {
                    if let Some(fill) = MatchOrder::try_fill_limit_order(
                        &self.inst_matcher,
                        &order,
                        ExecType::Taker,
                    ) {
                        self.on_fill(&fill);
                        self.broker_events_buf.push_back(BrokerEvent::Fill(fill));
                    } else {
                        self.limit_orders.insert(order.order_id, order);
                        self.broker_events_buf
                            .push_back(BrokerEvent::Placed(Order::Limit(order)));
                    }
                }
            },
            ClientEvent::AmendOrder(order) => {
                if let Order::Limit(order) = order {
                    if let Some(existing_order) = self.limit_orders.get_mut(&order.order_id) {
                        existing_order.price = order.price;
                        existing_order.size = order.size;
                        self.broker_events_buf
                            .push_back(BrokerEvent::Amended(Order::Limit(*existing_order)));
                    }
                }
            }
            ClientEvent::CancelOrder(order_id) => {
                self.limit_orders.remove(&order_id);
                self.broker_events_buf
                    .push_back(BrokerEvent::Canceled(order_id));
            }
        }
    }

    fn now(&self) -> Timestamp {
        self.ts
    }
}

/// 市场数据类型。由DataProvider流式提供。从中可能提取Matcher，用于撮合交易。
pub trait MarketData<M>: Clone + Debug {
    fn draw_matcher(self) -> Option<M>;
    fn get_ts(&self) -> Timestamp;
}
impl<T> MarketData<T> for T
where
    T: MatchOrder + Clone + Debug,
{
    fn draw_matcher(self) -> Option<T> {
        Some(self)
    }

    fn get_ts(&self) -> Timestamp {
        self.get_ts()
    }
}

/// 能够用于撮合订单的市场数据。一般是bbo。
pub trait MatchOrder: Sized {
    /// 由现存的Bbo，立即成交市价单。
    fn fill_market_order(inst_data: &FxHashMap<InstId, Self>, order: &MarketOrder) -> Fill;
    /// 限价单到达时，尝试以Taker成交限价单。随后每期对限价单进行匹配。
    fn try_fill_limit_order(
        inst_data: &FxHashMap<InstId, Self>,
        order: &LimitOrder,
        exec_type: ExecType,
    ) -> Option<Fill>;
    fn instrument_id(&self) -> InstId;
    fn get_ts(&self) -> Timestamp;
    fn market_price(&self) -> f64;

    /// 通过由 产品名-MatchOrder 组成的HashMap，得到所有产品的价格
    fn get_inst_market_price(inst_data: &FxHashMap<InstId, Self>) -> FxHashMap<InstId, f64> {
        inst_data
            .iter()
            .map(|(id, data)| (*id, data.market_price()))
            .collect()
    }
}

impl MatchOrder for Bbo {
    fn fill_market_order(inst_bbo: &FxHashMap<InstId, Self>, order: &MarketOrder) -> Fill {
        let bbo = inst_bbo.get(&order.instrument_id).unwrap();
        let price = if order.side {
            bbo.ask_price
        } else {
            bbo.bid_price
        };
        Fill {
            order_id: order.order_id,
            instrument_id: order.instrument_id,
            side: order.side,
            price,
            filled_size: order.size,
            acc_filled_size: order.size,
            exec_type: ExecType::Taker,
            state: FillState::Filled,
        }
    }

    // best ask等于买单价或best bid等于卖单价就成交。
    // 是最为保守的估计，即假设我们的挂单永远在队列末尾
    fn try_fill_limit_order(
        inst_bbo: &FxHashMap<InstId, Bbo>,
        order: &LimitOrder,
        exec_type: ExecType,
    ) -> Option<Fill> {
        let bbo = inst_bbo.get(&order.instrument_id).unwrap();

        // 若是Maker，成交会是挂单价；若是Taker，成交价会是最优买卖价
        let price = if exec_type == ExecType::Maker {
            order.price
        } else {
            if order.side {
                bbo.ask_price
            } else {
                bbo.bid_price
            }
        };
        // 若买单的价格高于最优卖单
        if (order.side && order.price >= bbo.ask_price)
        // 或卖单的价格低于最优买单
            || (!order.side && order.price <= bbo.bid_price)
        {
            let fill = Fill {
                order_id: order.order_id,
                instrument_id: order.instrument_id,
                side: order.side,
                price,
                filled_size: order.size,
                acc_filled_size: order.size,
                exec_type,
                state: FillState::Filled,
            };
            Some(fill)
        } else {
            None
        }
    }

    fn instrument_id(&self) -> InstId {
        self.instrument_id
    }

    fn get_ts(&self) -> Timestamp {
        self.ts as Timestamp
    }

    fn market_price(&self) -> f64 {
        self.get_unbiased_price()
    }
}

#[derive(Default)]
pub struct Reporter {
    value_history: Vec<Record>,
    frequency: u64,

    /// 最后一个频率桶的时间戳
    last_ts_bin: Timestamp,
    value_buf: f64,

    is_initialized: bool,
    is_end: bool,
}

impl Reporter {
    fn new(frequency: Duration) -> Self {
        Self {
            frequency: frequency.num_milliseconds() as u64,
            ..Default::default()
        }
    }

    fn pub_buf_record(&mut self) {
        let new_ts_bin = self.last_ts_bin + self.frequency;
        let new_record = Record::new(new_ts_bin, self.value_buf);
        self.value_history.push(new_record);
        self.last_ts_bin += self.frequency;
    }

    fn insert(&mut self, ts: Timestamp, value: f64) {
        if !self.is_initialized {
            self.last_ts_bin = ts / self.frequency * self.frequency;
            self.value_buf = value;
            self.is_initialized = true;
            return;
        }

        // 若新的数据的时间戳大于buf位于的bin，则将buf放入到value_history中
        if ts > self.last_ts_bin + self.frequency {
            while self.last_ts_bin + self.frequency < ts {
                self.pub_buf_record();
            }
        }
        self.value_buf = value;
    }

    fn end(&mut self) {
        if self.is_end {
            return;
        }
        self.is_end = true;

        // 若value_history为空，则直接存入buf数据
        if self.value_history.last().is_none() {
            let new_ts_bin = self.last_ts_bin + self.frequency;
            let record = Record::new(new_ts_bin, self.value_buf);
            self.value_history.push(record);
            return;
        };

        self.pub_buf_record();
    }

    pub fn to_csv(&self, path: &Path) -> Result<()> {
        let mut writer = csv::Writer::from_path(path)?;
        for record in &self.value_history {
            writer.serialize(record)?;
        }
        writer.flush()?;
        Ok(())
    }

    pub fn last_value(&self) -> Option<f64> {
        self.value_history.last().map(|record| record.value)
    }

    pub fn sharpe_ratio(&self) -> f64 {
        let returns: Vec<f64> = self
            .value_history
            .windows(2)
            .map(|window| {
                let prev_value = window[0].value;
                let curr_value = window[1].value;
                (curr_value - prev_value) / prev_value
            })
            .collect();

        let mean_return = returns.iter().mean();
        let std_dev = returns.iter().std_dev();
        mean_return / std_dev
    }
}

#[derive(Clone, PartialEq, Debug, Serialize)]
struct Record {
    ts: Timestamp,
    value: f64,
}

impl Record {
    fn new(ts: Timestamp, value: f64) -> Self {
        Self { ts, value }
    }
}

pub struct TransactionCostModel {
    maker_fee: f64,
    taker_fee: f64,
    slippage: f64,
}

impl TransactionCostModel {
    pub fn new(maker_fee: f64, taker_fee: f64, slippage: f64) -> Self {
        Self {
            maker_fee,
            taker_fee,
            slippage,
        }
    }

    pub fn new_okx(slippage: f64) -> Self {
        Self {
            maker_fee: 0.0002,
            taker_fee: 0.0005,
            slippage,
        }
    }

    pub fn calculate_cost(&self, fill: &Fill) -> f64 {
        let (fee, slippage) = if fill.exec_type == ExecType::Taker {
            (self.taker_fee, self.slippage)
        } else {
            (self.maker_fee, 0.)
        };
        let price = if fill.side {
            fill.price * (1.0 + slippage)
        } else {
            fill.price * (1.0 - slippage)
        };
        price * fill.filled_size * (1.0 + fee) - fill.price * fill.filled_size
    }
}

#[cfg(test)]
mod tests {
    use float_cmp::assert_approx_eq;

    use super::*;

    #[test]
    fn test_reporter_insert_same_bin() {
        let mut reporter = Reporter::new(Duration::milliseconds(100));
        reporter.insert(150, 10.0);
        reporter.insert(180, 15.0);

        // Second value in same bin just updates buffer
        assert_eq!(reporter.value_buf, 15.0);
        assert_eq!(reporter.value_history.len(), 0);
    }

    #[test]
    fn test_reporter_insert_multiple_bins() {
        let mut reporter = Reporter::new(Duration::milliseconds(100));
        reporter.insert(150, 10.0);
        reporter.insert(450, 30.0);

        // Should create multiple history entries
        assert_eq!(reporter.value_history.len(), 3);
        assert_eq!(reporter.value_history[0], Record::new(200, 10.0));
        assert_eq!(reporter.value_history[1], Record::new(300, 10.0));
        assert_eq!(reporter.value_history[2], Record::new(400, 10.0));

        reporter.end();
        assert_eq!(reporter.value_history[3], Record::new(500, 30.0));
    }

    #[test]
    fn test_reporter_end() {
        let mut reporter = Reporter::new(Duration::milliseconds(100));
        reporter.insert(150, 10.0);
        reporter.end();
        reporter.end(); // End again does nothing

        // End should push the final buffered value
        assert_eq!(reporter.value_history.len(), 1);
        assert_eq!(reporter.value_history[0], Record::new(200, 10.0));
    }

    // Mock DataProvider for testing
    struct MockDataProvider {
        data: Vec<Bbo>,
        index: usize,
    }

    impl MockDataProvider {
        fn new(data: Vec<Bbo>) -> Self {
            Self { data, index: 0 }
        }
    }

    impl Stream for MockDataProvider {
        type Item = Bbo;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            if self.index < self.data.len() {
                let data = self.data[self.index];
                self.index += 1;
                Poll::Ready(Some(data))
            } else {
                Poll::Ready(None)
            }
        }
    }

    impl Unpin for MockDataProvider {}

    fn create_mock_bbo(ts: u64, bid_price: f64, ask_price: f64) -> Bbo {
        Bbo {
            ts,
            instrument_id: InstId::EthUsdtSwap,
            bid_price,
            ask_price,
            bid_size: 1.,
            ask_size: 1.,
        }
    }

    fn create_market_order(order_id: u64, size: f64, side: bool) -> Order {
        Order::Market(MarketOrder {
            order_id,
            instrument_id: InstId::EthUsdtSwap,
            size,
            side,
        })
    }

    fn create_limit_order(order_id: u64, price: f64, size: f64, side: bool) -> Order {
        Order::Limit(LimitOrder {
            order_id,
            instrument_id: InstId::EthUsdtSwap,
            price,
            size,
            side,
            filled_size: 0.,
        })
    }

    macro_rules! create_sandbox_broker {
        ($inst_id:expr, $mock_data:expr) => {
            SandboxBroker::new(
                vec![$inst_id],
                MockDataProvider::new($mock_data),
                100000.0,
                TransactionCostModel::new(0.001, 0.002, 0.0001),
                Duration::milliseconds(1000),
            )
            .await
        };
        () => {};
    }

    #[tokio::test]
    async fn test_sandbox_broker_market_order() {
        let mock_data = vec![create_mock_bbo(1000, 50000.0, 50001.0)];

        let mut broker = create_sandbox_broker!(InstId::EthUsdtSwap, mock_data);

        let market_order = create_market_order(1, 1.0, true);

        broker.on_client_event(ClientEvent::PlaceOrder(market_order));

        // Should have a fill event in buffer
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Fill(fill) => {
                assert_eq!(fill.order_id, 1);
                assert_eq!(fill.price, 50001.0); // Should fill at ask price
                assert_eq!(fill.filled_size, 1.0);
                assert!(fill.side);
                assert_eq!(fill.exec_type, ExecType::Taker);
            }
            _ => panic!("Expected Fill event"),
        }

        // Cash should be reduced by the cost
        assert!(broker.cash < 100000.0);
    }

    #[tokio::test]
    async fn test_sandbox_broker_limit_order_immediate_fill() {
        let mock_data = vec![create_mock_bbo(1000, 50000.0, 50001.0)];

        let data_provider = MockDataProvider::new(mock_data);
        let transaction_cost_model = TransactionCostModel::new(0.001, 0.002, 0.0001);

        let mut broker = SandboxBroker::new(
            vec![InstId::EthUsdtSwap],
            data_provider,
            100000.0,
            transaction_cost_model,
            Duration::milliseconds(1000),
        )
        .await;

        // Place a limit buy order at ask price (should fill immediately)
        let limit_order = create_limit_order(2, 50001.0, 0.5, true);

        broker.on_client_event(ClientEvent::PlaceOrder(limit_order));

        // Should have a fill event
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Fill(fill) => {
                assert_eq!(fill.order_id, 2);
                assert_eq!(fill.price, 50001.0);
                assert_eq!(fill.filled_size, 0.5);
                assert!(fill.side);
                assert_eq!(fill.exec_type, ExecType::Taker);
            }
            _ => panic!("Expected Fill event"),
        }
    }

    #[tokio::test]
    async fn test_sandbox_broker_limit_order_placed() {
        let mock_data = vec![
            create_mock_bbo(1000, 50000.0, 50001.0),
            create_mock_bbo(2000, 50002.0, 50003.0),
        ];

        let mut broker = create_sandbox_broker!(InstId::EthUsdtSwap, mock_data);

        dbg!(&broker.data_provider.index);

        // Place a limit buy order below current bid (should not fill)
        let limit_order = create_limit_order(3, 49999.0, 1.0, true);

        broker.on_client_event(ClientEvent::PlaceOrder(limit_order));
        let event = broker.next().await.unwrap();
        assert!(matches!(event, BrokerEvent::Placed(_)));

        // Should have the order in limit_orders map
        assert!(broker.limit_orders.contains_key(&3));
        assert_eq!(broker.limit_orders.len(), 1);

        // Should get data event, not fill
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Data(_) => {
                // Expected
            }
            _ => panic!("Expected Data event"),
        }
    }

    #[tokio::test]
    async fn test_sandbox_broker_limit_order_fill_on_price_movement() {
        let mock_data = vec![
            create_mock_bbo(1000, 50000.0, 50001.0),
            create_mock_bbo(2000, 50000.0, 50001.0),
            create_mock_bbo(3000, 49997.0, 49998.0), // Price drops
        ];

        let mut broker = create_sandbox_broker!(InstId::EthUsdtSwap, mock_data);

        // Place a limit buy order
        let limit_order = create_limit_order(4, 49999.0, 1.0, true);

        broker.on_client_event(ClientEvent::PlaceOrder(limit_order));

        // First event should be order placed
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Placed(_) => {
                // Expected
            }
            _ => panic!("Expected Placed event: {event:#?}"),
        }

        // First event should be data
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Data(_) => {
                // Expected
            }
            _ => panic!("Expected Data event: {event:#?}"),
        }

        // Second event should be fill (after price movement)
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Fill(fill) => {
                assert_eq!(fill.order_id, 4);
                assert_eq!(fill.price, 49999.0); // Fill at ask price
                assert_eq!(fill.exec_type, ExecType::Maker);
            }
            _ => panic!("Expected Fill event: {event:#?}"),
        }

        // Order should be removed from limit_orders
        assert!(!broker.limit_orders.contains_key(&4));
    }

    #[tokio::test]
    async fn test_sandbox_broker_amend_order() {
        let mock_data = vec![
            create_mock_bbo(1000, 50000.0, 50001.0),
            create_mock_bbo(1000, 50002.0, 50003.0),
        ];

        let mut broker = create_sandbox_broker!(InstId::EthUsdtSwap, mock_data);

        let limit_order = create_limit_order(5, 49999.0, 1.0, true);

        broker.on_client_event(ClientEvent::PlaceOrder(limit_order));
        let event = broker.next().await.unwrap();
        assert!(matches!(event, BrokerEvent::Placed(_)));

        // Amend the order
        let amended_order = create_limit_order(5, 50001.0, 0.8, true);

        broker.on_client_event(ClientEvent::AmendOrder(amended_order));
        let event = broker.next().await.unwrap();
        assert!(matches!(event, BrokerEvent::Amended(_)));

        // Check that order was amended
        let order = broker.limit_orders.get(&5).unwrap();
        assert_eq!(order.price, 50001.0);
        assert_eq!(order.size, 0.8);

        // Get data event
        let event = broker.next().await.unwrap();
        assert!(matches!(event, BrokerEvent::Data(_)));
    }

    #[tokio::test]
    async fn test_sandbox_broker_cancel_order() {
        let mock_data = vec![create_mock_bbo(1000, 50000.0, 50001.0)];

        let mut broker = create_sandbox_broker!(InstId::EthUsdtSwap, mock_data);

        // Place a limit order
        let limit_order = create_limit_order(6, 49999.0, 1.0, true);

        broker.on_client_event(ClientEvent::PlaceOrder(limit_order));
        assert!(broker.limit_orders.contains_key(&6));

        // Cancel the order
        broker.on_client_event(ClientEvent::CancelOrder(6));

        // Order should be removed
        assert!(!broker.limit_orders.contains_key(&6));
        assert_eq!(broker.limit_orders.len(), 0);
    }

    #[tokio::test]
    async fn test_sandbox_broker_multiple_orders_complex_scenario() {
        let mock_data = vec![
            create_mock_bbo(0, 50000.0, 50001.0),
            create_mock_bbo(1000, 50000.0, 50001.0),
            create_mock_bbo(2000, 49995.0, 49996.0), // Price drops significantly
            create_mock_bbo(3000, 50005.0, 50006.0), // Price rises
        ];

        let mut broker = create_sandbox_broker!(InstId::EthUsdtSwap, mock_data);
        let initial_cash = broker.cash;

        // Place multiple orders
        let orders = vec![
            ClientEvent::PlaceOrder(create_limit_order(10, 49998.0, 0.5, true)),
            ClientEvent::PlaceOrder(create_limit_order(11, 50002.0, 1.0, false)),
            ClientEvent::PlaceOrder(create_market_order(12, 0.1, true)),
        ];

        broker.on_client_events(orders.into_iter());

        // Should have 2 limit orders placed and 1 market order filled
        assert_eq!(broker.limit_orders.len(), 2);

        let mut fill_count = 0;
        let mut data_count = 0;

        let expected_fills = vec![
            Fill {
                order_id: 12,
                instrument_id: InstId::EthUsdtSwap,
                side: true,
                price: 50001.0, // Market order should fill at ask price
                filled_size: 0.1,
                acc_filled_size: 0.1,
                exec_type: ExecType::Taker,
                state: FillState::Filled,
            },
            Fill {
                order_id: 10,
                instrument_id: InstId::EthUsdtSwap,
                side: true,
                price: 49998.0,
                filled_size: 0.5,
                acc_filled_size: 0.5,
                exec_type: ExecType::Maker,
                state: FillState::Filled,
            },
            Fill {
                order_id: 11,
                instrument_id: InstId::EthUsdtSwap,
                side: false,
                price: 50002.0,
                filled_size: 1.,
                acc_filled_size: 1.,
                exec_type: ExecType::Maker,
                state: FillState::Filled,
            },
        ];

        // Process all events
        while let Some(event) = broker.next().await {
            match event {
                BrokerEvent::Fill(fill) => {
                    assert_eq!(fill, expected_fills[fill_count]);
                    fill_count += 1;
                }
                BrokerEvent::Data(_) => {
                    data_count += 1;
                }
                _ => {}
            }
        }

        let position = &broker.portfolio.positions[&InstId::EthUsdtSwap];
        assert_approx_eq!(f64, position.size, 0.5 - 1. + 0.1); // 0.5 bought, 1 sold, 0.1 bought
        assert!(position.size < 0.); // Net position should be short

        // Should have processed several fills and data events
        assert_eq!(data_count, 3); // All market data events

        // Cash should have changed due to trades
        assert_ne!(broker.cash, initial_cash);

        // Portfolio should have positions
        assert!(!broker.portfolio.positions.is_empty());

        dbg!(&broker.reporter.value_history);
    }

    #[tokio::test]
    async fn test_sandbox_broker_reporter() {
        // Create market data with clear price changes
        let mock_data = vec![
            create_mock_bbo(999, 49999.0, 50000.0),  // Initial price
            create_mock_bbo(1999, 50999.0, 51000.0), // Price up
            create_mock_bbo(2999, 48999.0, 49000.0), // Price down
            create_mock_bbo(3999, 51999.0, 52000.0), // Price up again
        ];

        let data_provider = MockDataProvider::new(mock_data);

        // Zero transaction costs
        let transaction_cost_model = TransactionCostModel::new(0.0, 0.0, 0.0);

        // Initial cash 10,000
        let initial_cash = 10000.0;
        let mut broker = SandboxBroker::new(
            vec![InstId::EthUsdtSwap],
            data_provider,
            initial_cash,
            transaction_cost_model,
            Duration::milliseconds(1000),
        )
        .await;

        // 1. Buy 0.1 BTC at 50,000
        broker.on_client_event(ClientEvent::PlaceOrder(create_market_order(1, 0.1, true)));

        // Get first event (fill)
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Fill(_) => {
                // Expected - market order filled
            }
            _ => panic!("Expected Fill event"),
        }

        // 2. Price moves up, get data event
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Data(_) => {
                // Expected - price change event
            }
            _ => panic!("Expected Data event"),
        }

        // 3. Sell 0.05 BTC at 51,000
        broker.on_client_event(ClientEvent::PlaceOrder(create_market_order(2, 0.05, false)));

        // Get sell fill event
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Fill(_) => {
                // Expected - market order filled
            }
            _ => panic!("Expected Fill event"),
        }

        // 4. Price moves down, get data event
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Data(_) => {
                // Expected - price change event
            }
            _ => panic!("Expected Data event"),
        }

        // 5. Buy 0.1 BTC at 49,000
        broker.on_client_event(ClientEvent::PlaceOrder(create_market_order(3, 0.1, true)));

        // Get buy fill event
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Fill(_) => {
                // Expected - market order filled
            }
            _ => panic!("Expected Fill event"),
        }

        // 6. Final price moves up, get data event
        let event = broker.next().await.unwrap();
        match event {
            BrokerEvent::Data(_) => {
                // Expected - final price change event
            }
            _ => panic!("Expected Data event"),
        }

        // No more events
        assert!(broker.next().await.is_none());

        dbg!(&broker.reporter.value_history);
        // Check reporter's value history
        assert_eq!(broker.reporter.value_history.len(), 4);

        // Expected portfolio values at each timestamp:
        // t=1000: initial cash (after buying 0.1 BTC at (49,999, 50,000))
        //         cash: 10000 - 0.1*50,000 = 5000, portfolio: 0.1 BTC @ 49,999.5
        // t=2000: after price rise to (50,999, 51,000) and selling 0.05 BTC
        //         cash: 5000 + 0.05*50,999, portfolio: 0.05 BTC @ 50,999.5
        // t=3000: after price dropping to (48,999, 49,000) and buying 0.1 BTC
        //         cash: 5000 + 0.05*50,999 - 0.1*48,999, portfolio: 0.15 BTC @ 48,999.5
        // t=4000: after price rising to (51,999, 52,000)
        //         cash: 5000 + 0.05*50,999 - 0.1*48,999 = 2650, portfolio: 0.15 BTC @ 51,999.5
        let mut cash = 10_000.;
        let mut btc_holding = 0.;
        let mut btc_price = 49_999.5;

        cash -= 0.1 * 50_000.;
        btc_holding += 0.1;
        assert_approx_eq!(
            f64,
            broker.reporter.value_history[0].value,
            cash + btc_holding * btc_price,
            epsilon = 1e-6
        );

        cash += 0.05 * 50_999.;
        btc_holding -= 0.05;
        btc_price = 50_999.5;
        assert_approx_eq!(
            f64,
            broker.reporter.value_history[1].value,
            cash + btc_holding * btc_price,
            epsilon = 1e-6
        );

        cash -= 0.1 * 49_000.;
        btc_holding += 0.1;
        btc_price = 48_999.5;
        assert_approx_eq!(
            f64,
            broker.reporter.value_history[2].value,
            cash + btc_holding * btc_price,
            epsilon = 1e-6
        );

        btc_price = 51_999.5;
        assert_approx_eq!(
            f64,
            broker.reporter.value_history[3].value,
            cash + btc_holding * btc_price,
            epsilon = 1e-6
        );

        // Verify timestamps
        assert_eq!(broker.reporter.value_history[0].ts, 1000);
        assert_eq!(broker.reporter.value_history[1].ts, 2000);
        assert_eq!(broker.reporter.value_history[2].ts, 3000);
        assert_eq!(broker.reporter.value_history[3].ts, 4000);

        // Verify final portfolio state
        assert_approx_eq!(f64, broker.cash, cash, epsilon = 1e-6);
        assert!(broker.portfolio.positions[&InstId::EthUsdtSwap].size > 0.); // Long
        assert_approx_eq!(
            f64,
            broker.portfolio.positions[&InstId::EthUsdtSwap].size,
            0.15,
            epsilon = 1e-6
        );
    }
}
