pub mod backtest;
pub mod data;
pub mod okx;
pub mod strategy;
mod utils;

use std::marker::PhantomData;

use ::utils::Duplex;
use float_cmp::approx_eq;
use futures::{Stream, StreamExt};
use rustc_hash::FxHashMap;

use crate::strategy::Strategy;

pub use data_center::types::InstId;

pub trait DataProvider<D>: Stream<Item = D> + Unpin + Send {}
impl<D, S> DataProvider<D> for S where S: Stream<Item = D> + Unpin + Send {}

type OrderId = u64;
type Timestamp = u64;

#[derive(Debug, Clone)]
pub enum Order {
    Market(MarketOrder),
    Limit(LimitOrder),
}

impl Order {
    pub fn order_id(&self) -> OrderId {
        match self {
            Order::Market(order) => order.order_id,
            Order::Limit(order) => order.order_id,
        }
    }

    pub fn instrument_id(&self) -> InstId {
        match self {
            Order::Market(order) => order.instrument_id,
            Order::Limit(order) => order.instrument_id,
        }
    }

    pub fn side(&self) -> bool {
        match self {
            Order::Market(order) => order.side,
            Order::Limit(order) => order.side,
        }
    }

    /// 不含有方向信息的size。严格为正。
    pub fn size(&self) -> f64 {
        match self {
            Order::Market(order) => order.size,
            Order::Limit(order) => order.size,
        }
    }

    /// 将方向信息放到正负号的size，买单为正，卖单为负
    pub fn raw_size(&self) -> f64 {
        if self.side() {
            self.size()
        } else {
            -self.size()
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MarketOrder {
    pub order_id: OrderId,
    pub instrument_id: InstId,
    pub size: f64,
    pub side: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct LimitOrder {
    pub order_id: OrderId,
    pub instrument_id: InstId,
    pub price: f64,
    pub size: f64,
    /// filled_size 根据传回的fill信息进行更新
    pub filled_size: f64,
    pub side: bool,
}

impl LimitOrder {
    pub fn from_raw_size(
        raw_size: f64,
        order_id: OrderId,
        instrument_id: InstId,
        price: f64,
    ) -> Self {
        let (size, side) = if raw_size > 0. {
            (raw_size, true)
        } else {
            (-raw_size, false)
        };

        Self {
            order_id,
            instrument_id,
            price,
            size,
            side,
            filled_size: 0.,
        }
    }

    pub fn amended(&mut self, new_size: f64, new_price: f64) -> AmendOrder {
        self.size = self.filled_size + new_size;
        self.price = new_price;

        AmendOrder {
            order_id: self.order_id,
            instrument_id: self.instrument_id,
            new_size: self.size,
            new_price: self.price,
        }
    }

    pub fn unfilled_size(&self) -> f64 {
        self.size - self.filled_size
    }

    pub fn fill(mut self, fill: &Fill) -> Option<Self> {
        match fill.state {
            FillState::Live => Some(self),
            FillState::Partially => {
                self.filled_size = fill.acc_filled_size;
                Some(self)
            }
            FillState::Filled => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AmendOrder {
    pub order_id: u64,
    pub instrument_id: InstId,
    pub new_size: f64,
    pub new_price: f64,
}

#[derive(Debug, PartialEq, Default)]
pub enum ExecType {
    #[default]
    Taker,
    Maker,
}

#[derive(Debug, PartialEq, Default)]
pub enum FillState {
    Live,
    Partially,
    #[default]
    Filled,
}

#[derive(Debug, PartialEq, Default)]
pub struct Fill {
    pub order_id: OrderId,
    pub instrument_id: InstId,
    /// Filled size in this fill, not accumulative
    pub filled_size: f64,
    /// Accumulative filled size
    pub acc_filled_size: f64,
    pub price: f64,
    pub side: bool,
    pub exec_type: ExecType,
    pub state: FillState,
}

#[derive(Debug)]
pub enum BrokerEvent<D> {
    Data(D),
    Fill(Fill),
    Placed(Order),
    Amended(Order),
    Canceled(OrderId),
}

impl<D> BrokerEvent<D> {
    pub fn to_data(&self) -> Option<&D> {
        match self {
            BrokerEvent::Data(data) => Some(data),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ClientEvent {
    PlaceOrder(Order),
    AmendOrder(AmendOrder),
    CancelOrder(InstId, OrderId),
}

impl ClientEvent {
    pub fn place_limit_order(limit_order: LimitOrder) -> Self {
        Self::PlaceOrder(Order::Limit(limit_order))
    }

    pub fn is_order_event(&self) -> bool {
        match self {
            ClientEvent::PlaceOrder(_)
            | &ClientEvent::AmendOrder(_)
            | &ClientEvent::CancelOrder(_, _) => true,
            // _ => false
        }
    }
}

/// D: type for the data; IE: error type for the input
pub trait Broker<D> {
    async fn on_client_event(&mut self, client_event: ClientEvent);
    async fn on_client_events(&mut self, client_events: impl Iterator<Item = ClientEvent>) {
        for event in client_events {
            self.on_client_event(event).await;
        }
    }
    async fn next_broker_event(&mut self) -> Option<BrokerEvent<D>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Position {
    size: f64,
}

impl Position {
    pub fn new(size: f64) -> Self {
        Self { size }
    }

    pub fn new_from_fill(fill: &Fill) -> Self {
        let size = if fill.side {
            fill.filled_size
        } else {
            -fill.filled_size
        };
        Self { size }
    }

    pub fn update(&mut self, fill: &Fill) {
        if fill.side {
            self.size += fill.filled_size;
        } else {
            self.size -= fill.filled_size;
        }
    }

    pub fn is_clear(&self, size_digits: i32) -> bool {
        let eps = 10f64.powi(-size_digits);
        approx_eq!(f64, 0., self.size, epsilon = eps)
    }

    pub fn size(&self) -> f64 {
        self.size
    }
}

#[derive(Default)]
pub struct Portfolio {
    positions: FxHashMap<InstId, Position>,
}

impl Portfolio {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn update(&mut self, new_fill: &Fill) {
        let instrument_id = new_fill.instrument_id;

        if let Some(position) = self.positions.get_mut(&instrument_id) {
            position.update(new_fill);
            let balance = position.size().abs();
            if balance < 1e-12 {
                self.positions.remove(&instrument_id);
            }
        } else {
            self.positions
                .insert(instrument_id, Position::new_from_fill(new_fill));
        }
    }

    pub fn get_value(&self, inst_price: &FxHashMap<InstId, f64>) -> f64 {
        let mut value = 0.0;
        for (instrument_id, position) in &self.positions {
            let price = inst_price.get(instrument_id).unwrap();
            value += position.size * price;
        }
        value
    }
}

pub struct Engine<B, S, D> {
    broker: B,
    strategy: S,
    _phantom_data: PhantomData<D>,
}

impl<B, S, D> Engine<B, S, D>
where
    B: Broker<D>,
    S: Strategy<D>,
{
    pub fn new(broker: B, strategy: S) -> Self {
        Self {
            broker,
            strategy,
            _phantom_data: PhantomData,
        }
    }

    pub async fn run(&mut self) {
        loop {
            let Some(broker_event) = self.broker.next_broker_event().await else {
                break;
            };
            let client_events = self.strategy.on_event(&broker_event);
            self.broker.on_client_events(client_events.into_iter()).await;
        }
    }

    pub fn broker(&self) -> &B {
        &self.broker
    }

    pub fn strategy(&self) -> &S {
        &self.strategy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position() {
        fn gen_fill(side: bool, filled_size: f64) -> Fill {
            Fill {
                side,
                filled_size,
                ..Default::default()
            }
        }

        let mut position = Position::new_from_fill(&gen_fill(true, 10.0));
        position.update(&gen_fill(true, 5.0));
        assert_eq!(position.size(), 15.0);

        position.update(&gen_fill(false, 5.0));
        assert_eq!(position.size(), 10.0);

        position.update(&gen_fill(false, 15.0));
        assert_eq!(position.size(), -5.0);

        let mut position = Position::new(-10.0);
        position.update(&gen_fill(false, 5.0));
        assert_eq!(position.size(), -15.0);

        position.update(&gen_fill(true, 20.0));
        assert_eq!(position.size(), 5.0);
    }

    #[test]
    fn test_portfolio() {
        let mut portfolio = Portfolio::new();
        let fill1 = Fill {
            order_id: 1,
            instrument_id: InstId::BtcUsdtSwap,
            side: true,
            price: 150.0,
            filled_size: 10.0,
            acc_filled_size: 10.0,
            exec_type: ExecType::Taker,
            state: FillState::Filled,
        };
        portfolio.update(&fill1);
        assert_eq!(portfolio.positions.len(), 1);

        let fill2 = Fill {
            order_id: 2,
            instrument_id: InstId::BtcUsdtSwap,
            side: false,
            price: 155.0,
            filled_size: 5.0,
            acc_filled_size: 5.0,
            exec_type: ExecType::Maker,
            state: FillState::Filled,
        };
        portfolio.update(&fill2);
        assert_eq!(portfolio.positions.len(), 1);

        let fill3 = Fill {
            order_id: 3,
            instrument_id: InstId::EthUsdtSwap,
            side: true,
            price: 2800.0,
            filled_size: 2.0,
            acc_filled_size: 2.0,
            exec_type: ExecType::Taker,
            state: FillState::Filled,
        };
        portfolio.update(&fill3);
        assert_eq!(portfolio.positions.len(), 2);

        let mut inst_price = FxHashMap::default();
        inst_price.insert(InstId::BtcUsdtSwap, 160.0);
        inst_price.insert(InstId::EthUsdtSwap, 2900.0);
        let value = portfolio.get_value(&FxHashMap::from(inst_price));
        assert_eq!(value, 5.0 * 160.0 + 2.0 * 2900.0);
    }
}
