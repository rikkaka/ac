use serde::{Deserialize, Serialize};

use super::types::*;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeArg<'a> {
    channel: Channel,
    #[serde(skip_serializing_if = "Option::is_none")]
    inst_type: Option<InstType>,
    inst_id: &'a str,
}

impl<'a> SubscribeArg<'a> {
    pub fn new_trades(inst_id: &'a str) -> Self {
        Self {
            channel: Channel::Trades,
            inst_type: None,
            inst_id,
        }
    }

    pub fn new_bbo_tbt(inst_id: &'a str) -> Self {
        Self {
            channel: Channel::BboTbt,
            inst_type: None,
            inst_id,
        }
    }

    pub fn new_orders(inst_type: InstType, inst_id: &'a str) -> Self {
        Self {
            channel: Channel::Orders,
            inst_type: Some(inst_type),
            inst_id,
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequest<OA> {
    id: String,
    op: OrderOp,
    args: [OA; 1],
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum OrderOp {
    Order,
    AmendOrder,
    CancelOrder,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LimitOrderArg {
    side: Side,
    inst_id: InstId,
    cl_ord_id: String,
    td_mode: TdMode,
    ord_type: OrdType,
    sz: String,
    px: String,
}

impl OrderRequest<LimitOrderArg> {
    pub fn new_limit(
        request_id: String,
        side: Side,
        inst_id: InstId,
        client_order_id: String,
        size: String,
        price: String,
    ) -> Self {
        let arg = LimitOrderArg {
            side,
            inst_id,
            cl_ord_id: client_order_id,
            td_mode: TdMode::Cross,
            ord_type: OrdType::Limit,
            sz: size,
            px: price,
        };
        Self {
            id: request_id,
            op: OrderOp::Order,
            args: [arg; 1],
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MarketOrderArg {
    side: Side,
    inst_id: InstId,
    cl_ord_id: String,
    td_mode: TdMode,
    ord_type: OrdType,
    sz: String,
}

impl OrderRequest<MarketOrderArg> {
    pub fn new_market(
        request_id: String,
        side: Side,
        inst_id: InstId,
        client_order_id: String,
        size: String,
    ) -> Self {
        let arg = MarketOrderArg {
            side,
            inst_id,
            cl_ord_id: client_order_id,
            td_mode: TdMode::Cross,
            ord_type: OrdType::Market,
            sz: size,
        };
        Self {
            id: request_id,
            op: OrderOp::Order,
            args: [arg; 1],
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmendOrderArg {
    inst_id: InstId,
    cl_ord_id: String,
    new_sz: String,
    new_px: String,
}

impl OrderRequest<AmendOrderArg> {
    pub fn new_amend(
        request_id: String,
        inst_id: InstId,
        client_order_id: String,
        new_size: String,
        new_price: String,
    ) -> Self {
        let arg = AmendOrderArg {
            inst_id,
            cl_ord_id: client_order_id,
            new_sz: new_size,
            new_px: new_price,
        };
        Self {
            id: request_id,
            op: OrderOp::AmendOrder,
            args: [arg; 1],
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderArg {
    inst_id: InstId,
    cl_ord_id: String,
}

impl OrderRequest<CancelOrderArg> {
    pub fn new_cancel(request_id: String, inst_id: InstId, client_order_id: String) -> Self {
        let arg = CancelOrderArg {
            inst_id,
            cl_ord_id: client_order_id,
        };
        Self {
            id: request_id,
            op: OrderOp::CancelOrder,
            args: [arg; 1],
        }
    }
}
