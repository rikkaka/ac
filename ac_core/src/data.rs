pub mod okx;

use data_center::types::{Action, OrdType, OrderPushType};

use crate::{BrokerEvent, ClientEvent, ExecType, Fill, FillState, InstId, LimitOrder, Order};

#[derive(Debug, Clone)]
pub struct Trade {
    /// Unix millis timestamp
    pub ts: i64,
    pub instrument_id: InstId,
    pub price: f64,
    pub size: f64,
    pub side: bool,
}

#[derive(Debug, Clone)]
pub struct Level {
    pub price: f64,
    pub size: f64,
    pub order_count: i32,
}

/// "Best bid and offer"
#[derive(Debug, Clone, Default, Copy)]
pub struct Bbo {
    /// Unix millis timestamp
    pub ts: u64,
    pub instrument_id: InstId,
    pub bid_price: f64,
    pub bid_size: f64,
    pub ask_price: f64,
    pub ask_size: f64,
}

impl Bbo {
    pub fn get_unbiased_price(&self) -> f64 {
        (self.bid_price * self.ask_size + self.ask_price * self.bid_size)
            / (self.bid_size + self.ask_size)
    }

    pub fn get_spread(&self) -> f64 {
        self.ask_price - self.bid_price
    }

    pub fn get_relevent_spread(&self) -> f64 {
        self.get_spread() / self.get_unbiased_price()
    }
}

impl From<data_center::types::Bbo> for Bbo {
    fn from(bbo: data_center::types::Bbo) -> Self {
        Self {
            ts: bbo.ts as u64,
            instrument_id: bbo.instrument_id,
            bid_price: bbo.bid_price,
            bid_size: bbo.bid_size,
            ask_price: bbo.ask_price,
            ask_size: bbo.ask_size,
        }
    }
}

pub struct Level1 {
    bbo: Bbo,
    last_price: f64,
    volume: f64,
}

impl<T> From<data_center::OrderPush> for BrokerEvent<T> {
    fn from(order_push: data_center::OrderPush) -> Self {
        let order = match order_push.ord_type {
            OrdType::Limit => Order::Limit(LimitOrder {
                order_id: order_push.order_id,
                instrument_id: order_push.inst_id,
                price: order_push.price,
                size: order_push.size,
                filled_size: order_push.filled_size,
                side: order_push.side,
            }),
            OrdType::Market => unimplemented!(),
        };

        match order_push.push_type {
            OrderPushType::Canceled => BrokerEvent::Canceled(order_push.order_id),
            OrderPushType::Amended => BrokerEvent::Amended(order),
            OrderPushType::Placed => BrokerEvent::Placed(order),
            OrderPushType::Fill => {
                let exec_type = match order_push.exec_type {
                    Some(data_center::types::ExecType::M) => ExecType::Maker,
                    Some(data_center::types::ExecType::T) => ExecType::Taker,
                    // Actually unreachable
                    _ => ExecType::Taker,
                };
                let state = match order_push.state {
                    data_center::types::OrderState::Filled => FillState::Filled,
                    _ => FillState::Partially,
                };
                let fill = Fill {
                    order_id: order_push.order_id,
                    instrument_id: order_push.inst_id,
                    filled_size: order_push.filled_size,
                    acc_filled_size: order_push.acc_filled_size,
                    price: order_push.price,
                    side: order_push.side,
                    exec_type,
                    state,
                };
                BrokerEvent::Fill(fill)
            }
        }
    }
}

impl BrokerEvent<Bbo> {
    pub fn try_from_data(data: data_center::Data) -> Option<Self> {
        match data {
            data_center::Data::Bbo(bbo) => Some(BrokerEvent::Data(bbo.into())),
            data_center::Data::Order(order_push) => Some(order_push.into()),
            data_center::Data::Trade(_) => None,
        }
    }
}

impl ClientEvent {
    fn try_into_action(client_event: ClientEvent) -> Action {
        todo!()
    }
}