mod backtest;
mod data;
mod strategy;
mod utils;

use ::utils::Duplex;
use futures::{Stream, StreamExt};
use rustc_hash::FxHashMap;

type InstId = String;

pub trait DataProvider<D>: Stream<Item = D> + Unpin + Send {
    fn next(&mut self) -> impl Future<Output = Option<D>> + Send;
    fn instruments(&self) -> &[InstId];
}

type OrderId = u32;

pub enum Order {
    Market(MarketOrder),
    Limit(LimitOrder),
}

pub struct MarketOrder {
    pub order_id: OrderId,
    pub instrument_id: InstId,
    pub size: f64,
    pub side: bool,
}

pub struct LimitOrder {
    pub order_id: OrderId,
    pub instrument_id: InstId,
    pub price: f64,
    pub size: f64,
    pub side: bool,
}

pub enum ExecType {
    Taker,
    Maker,
}

pub struct Fill {
    pub order_id: OrderId,
    pub price: f64,
    pub size: f64,
    pub exec_type: ExecType,
}

pub enum BrokerEvent<D> {
    Data(D),
    Fill(Fill),
}

pub enum ClientEvent {
    PlaceOrder(Order),
    ModifyOrder(Order),
    CancelOrder(OrderId),
}

/// D: type for the data; IE: error type for the input
pub trait Broker<D>: Duplex<Vec<ClientEvent>, anyhow::Error, BrokerEvent<D>> {}

pub trait MatchOrder: Sized {
    fn fill_market_order(inst_data: &FxHashMap<InstId, Self>, order: &MarketOrder) -> Fill;
    fn try_fill_limit_order(
        inst_data: &FxHashMap<InstId, Self>,
        order: &LimitOrder,
        exec_type: ExecType,
    ) -> Option<Fill>;
    fn instrument_id(&self) -> InstId;
}

pub trait DrawMatcher<M>: Clone {
    fn draw_matcher(self) -> Option<M>;
}