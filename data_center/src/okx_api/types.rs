use anyhow::{Ok, Result};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use smartstring::alias::String;

use crate::types::{Bbo, InstId, Level, Trade};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeArg<'a> {
    channel: Channel,
    #[serde(skip_serializing_if = "Option::is_none")]
    inst_type: Option<InstType>,
    inst_id: &'a str,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    Trades,
    BboTbt,
    Orders,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
pub enum InstType {
    Swap,
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

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum TdMode {
    Cross,
}
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum OrdType {
    Limit,
    Market,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Arg {
    pub channel: Channel,
    pub instId: InstId,
}

#[derive(Debug, Deserialize)]
pub struct Push<'a> {
    pub event: Option<String>,
    pub arg: Arg,
    #[serde(borrow)]
    pub data: Option<[&'a RawValue; 1]>,
}

pub enum Data {
    Trades(TradesData),
    BboTbt(InstId, DepthData),
    Orders(OrdersData),
}

impl Data {
    pub fn try_from_raw(value: &RawValue, arg: Arg) -> Result<Self> {
        match arg.channel {
            Channel::Trades => {
                let data = serde_json::from_str(value.get())?;
                Ok(Data::Trades(data))
            }
            Channel::BboTbt => {
                let data = serde_json::from_str(value.get())?;
                Ok(Data::BboTbt(arg.instId, data))
            }
            Channel::Orders => {
                let data = serde_json::from_str(value.get())?;
                Ok(Data::Orders(data))
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TradesData {
    pub instId: InstId,
    pub tradeId: String,
    pub px: String,
    pub sz: String,
    pub side: String,
    pub ts: String,
    pub count: String,
}

impl TradesData {
    pub fn try_into_trade(self) -> Result<Trade> {
        let ts = self.ts.parse::<i64>()?;
        let price = self.px.parse::<f64>()?;
        let size = self.sz.parse::<f64>()?;
        let side = match self.side.as_str() {
            "buy" => true,
            "sell" => false,
            _ => return Err(anyhow::anyhow!("Invalid side")),
        };
        let order_count = self.count.parse::<i32>()?;

        Ok(Trade {
            ts,
            instrument_id: self.instId,
            trade_id: self.tradeId,
            price,
            size,
            side,
            order_count,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct DepthData {
    /// "asks": [ [ "111.06", "55154", "0", "2" ], ... ]
    asks: Vec<[String; 4]>,
    /// "bids": [ [ "111.05", "57745", "0", "2" ], ... ]
    bids: Vec<[String; 4]>,
    /// "ts": "1670324386802"
    ts: String,
}

impl DepthData {
    pub fn try_into_bbo(self, instrument_id: InstId) -> Result<Bbo> {
        let ts = self.ts.parse::<i64>()?;
        let best_ask = Level {
            price: self.asks[0][0].parse::<f64>()?,
            size: self.asks[0][1].parse::<f64>()?,
            order_count: self.asks[0][3].parse::<i32>()?,
        };
        let best_bid = Level {
            price: self.bids[0][0].parse::<f64>()?,
            size: self.bids[0][1].parse::<f64>()?,
            order_count: self.bids[0][3].parse::<i32>()?,
        };

        Ok(Bbo {
            ts,
            instrument_id,
            best_ask,
            best_bid,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrdersData {
    cl_ord_id: String,
    state: OrderState,
    sz: String,
    fill_sz: String,
    acc_fill_sz: String,
    fill_pnl: String,
    fill_fee: String,
    amend_source: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrderState {
    Canceled,
    Live,
    PartiallyFilled,
    Filled,
}
