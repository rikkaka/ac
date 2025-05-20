mod backtest;
mod data;
mod strategy;
mod utils;

use futures::{Stream, StreamExt};
use ::utils::Duplex;

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

pub struct Fill {
    pub order_id: OrderId,
    pub price: f64,
    pub size: f64,
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
pub trait Broker<D, IE>: Duplex<Vec<ClientEvent>, IE, BrokerEvent<D>> {}
