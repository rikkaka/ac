use rustc_hash::FxHashMap;

use crate::{ExecType, Fill, InstId, LimitOrder, MarketOrder, data::Bbo};

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
        exec_type: ExecType::Taker,
    }
}

pub fn try_fill_limit_order(
    order: &LimitOrder,
    inst_bbo: &FxHashMap<InstId, Bbo>,
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
