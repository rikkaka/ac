use serde::{Deserialize, Serialize};

use super::types::*;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Request<A> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    op: Op,
    args: [A; 1],
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum Op {
    Subscribe,
    Order,
    AmendOrder,
    CancelOrder,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeArg {
    channel: Channel,
    #[serde(skip_serializing_if = "Option::is_none")]
    inst_type: Option<InstType>,
    inst_id: InstId,
}

impl Request<SubscribeArg> {
    pub fn subscribe_trades(inst_id: InstId) -> Self {
        let arg = SubscribeArg {
            channel: Channel::Trades,
            inst_type: None,
            inst_id,
        };
        Self {
            id: None,
            op: Op::Subscribe,
            args: [arg; 1],
        }
    }

    pub fn subscribe_bbo_tbt(inst_id: InstId) -> Self {
        let arg = SubscribeArg {
            channel: Channel::BboTbt,
            inst_type: None,
            inst_id,
        };
        Self {
            id: None,
            op: Op::Subscribe,
            args: [arg; 1],
        }
    }

    pub fn subscribe_orders(inst_type: InstType, inst_id: InstId) -> Self {
        let arg = SubscribeArg {
            channel: Channel::Orders,
            inst_type: Some(inst_type),
            inst_id,
        };
        Self {
            id: None,
            op: Op::Subscribe,
            args: [arg; 1],
        }
    }

    pub fn inst_id(&self) -> InstId {
        self.args[0].inst_id
    }

    pub fn channel(&self) -> Channel {
        self.args[0].channel
    }
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

impl Request<LimitOrderArg> {
    pub fn limit_order(
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
            id: Some(request_id),
            op: Op::Order,
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

impl Request<MarketOrderArg> {
    pub fn market_order(
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
            id: Some(request_id),
            op: Op::Order,
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

impl Request<AmendOrderArg> {
    pub fn amend_order(
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
            id: Some(request_id),
            op: Op::AmendOrder,
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

impl Request<CancelOrderArg> {
    pub fn cancel_order(request_id: String, inst_id: InstId, client_order_id: String) -> Self {
        let arg = CancelOrderArg {
            inst_id,
            cl_ord_id: client_order_id,
        };
        Self {
            id: Some(request_id),
            op: Op::CancelOrder,
            args: [arg; 1],
        }
    }
}
