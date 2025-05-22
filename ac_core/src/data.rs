pub mod okx;

use rustc_hash::FxHashMap;

use crate::{DrawMatcher, ExecType, Fill, InstId, LimitOrder, MarketOrder, MatchOrder, Order};

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

impl MatchOrder for Bbo {
    fn fill_market_order(inst_bbo: &FxHashMap<InstId, Self>, order: &MarketOrder) -> Fill {
        let bbo = inst_bbo.get(&order.instrument_id).unwrap();
        let price = if order.side {
            bbo.best_ask.price
        } else {
            bbo.best_bid.price
        };
        Fill {
            order_id: order.order_id,
            price,
            size: order.size,
            exec_type: ExecType::Taker,
        }
    }

    fn try_fill_limit_order(
        inst_bbo: &FxHashMap<InstId, Bbo>,
        order: &LimitOrder,
        exec_type: ExecType,
    ) -> Option<Fill> {
        let bbo = inst_bbo.get(&order.instrument_id).unwrap();
        let price = if order.side && order.price >= bbo.best_ask.price {
            Some(bbo.best_ask.price)
        } else if !order.side && order.price <= bbo.best_bid.price {
            Some(bbo.best_bid.price)
        } else {
            None
        };
        price.map(|price| Fill {
            order_id: order.order_id,
            price,
            size: order.size,
            exec_type: exec_type,
        })
    }

    fn instrument_id(&self) -> InstId {
        self.instrument_id.clone()
    }
}

impl DrawMatcher<Bbo> for Bbo {
    fn draw_matcher(self) -> Option<Bbo> {
        Some(self)
    }
}