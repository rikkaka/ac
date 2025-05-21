pub mod okx;

use crate::{InstId, Order};

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
#[derive(Debug, Clone)]
pub struct Bbo {
    /// Unix millis timestamp
    pub ts: i64,
    pub instrument_id: InstId,
    pub best_ask: Level,
    pub best_bid: Level,
}

impl From<data_center::types::Bbo> for Bbo {
    fn from(bbo: data_center::types::Bbo) -> Self {
        Self {
            ts: bbo.ts,
            instrument_id: bbo.instrument_id,
            best_ask: Level {
                price: bbo.best_ask.price,
                size: bbo.best_ask.size,
                order_count: bbo.best_ask.order_count,
            },
            best_bid: Level {
                price: bbo.best_bid.price,
                size: bbo.best_bid.size,
                order_count: bbo.best_bid.order_count,
            },
        }
    }
}