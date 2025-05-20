use rustc_hash::FxHashMap;

use crate::{data::Bbo, Fill, InstId, MarketOrder};

pub fn fill_market_order(order: &MarketOrder, inst_bbo: &FxHashMap<InstId, Bbo>) -> Fill {
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
    }
}