pub mod okx;


use crate::InstId;

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
    pub ask_size: f64
}

impl Bbo {
    pub fn get_unbiased_price(&self) -> f64 {
        (self.bid_price * self.ask_size + self.ask_price * self.bid_size) / (self.bid_size + self.ask_size)
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
            bid_price: bbo.best_bid.price,
            bid_size: bbo.best_bid.size,
            ask_price: bbo.best_ask.price,
            ask_size: bbo.best_ask.size
        }
    }
}

pub struct Level1 {
    bbo: Bbo,
    last_price: f64,
    volume: f64,
}
