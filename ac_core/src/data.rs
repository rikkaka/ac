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

// impl MarketData for Bbo {
//     fn check_fill(&self, order: &Order) -> Option<crate::Fill> {
//         if !order.get_instrument_id().eq(&self.instrument_id) {
//             return None;
//         }
//     }
// }