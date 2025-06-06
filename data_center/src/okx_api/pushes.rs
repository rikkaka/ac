use anyhow::{Ok, Result, anyhow};
use serde::Deserialize;
use serde_json::value::RawValue;
use smartstring::alias::String;

use super::types::*;
use crate::types::{Bbo, InstId, OrderPush, OrderPushType, Side, Trade};

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Arg {
    pub channel: Channel,
    pub inst_id: InstId,
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
    Orders(InstId, OrdersData),
}

impl Data {
    pub fn try_from_push(push: Push) -> Result<Self> {
        let raw_data = push.data.ok_or(anyhow!("Push without data: {push:#?}"))?;
        let raw_data = *raw_data.first()
            .ok_or(anyhow!("Push without data: {push:#?}"))?;
        let raw_data_str = raw_data.get();
        match push.arg.channel {
            Channel::Trades => {
                let data = serde_json::from_str(raw_data_str)?;
                Ok(Data::Trades(data))
            }
            Channel::BboTbt => {
                let data = serde_json::from_str(raw_data_str)?;
                Ok(Data::BboTbt(push.arg.inst_id, data))
            }
            Channel::Orders => {
                let data = serde_json::from_str(raw_data_str)?;
                Ok(Data::Orders(push.arg.inst_id, data))
            }
        }
    }
}

impl crate::types::Data {
    pub fn try_from_okx_data(okx_data: Data) -> Result<Self> {
        match okx_data {
            Data::Trades(data) => {
                let trade = data.try_into_trade()?;
                Ok(Self::Trade(trade))
            }
            Data::BboTbt(inst_id, data) => {
                let bbo = data.try_into_bbo(inst_id)?;
                Ok(Self::Bbo(bbo))
            }
            Data::Orders(inst_id, data) => {
                let order_push = data.try_into_order_push(inst_id)?;
                Ok(Self::Order(order_push))
            }
        }
    }

    pub fn try_from_okx_push(okx_push: Push) -> Result<Self> {
        let okx_data = Data::try_from_push(okx_push)?;
        Self::try_from_okx_data(okx_data)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradesData {
    pub inst_id: InstId,
    pub trade_id: String,
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
            instrument_id: self.inst_id,
            trade_id: self.trade_id,
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

        Ok(Bbo {
            ts,
            instrument_id,
            bid_price: self.bids[0][0].parse::<f64>()?,
            bid_size: self.bids[0][1].parse::<f64>()?,
            bid_order_count: self.bids[0][3].parse::<i32>()?,
            ask_price: self.asks[0][0].parse::<f64>()?,
            ask_size: self.asks[0][1].parse::<f64>()?,
            ask_order_count: self.asks[0][3].parse::<i32>()?,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrdersData {
    cl_ord_id: String,
    state: OrderState,
    side: Side,
    sz: String,
    fill_sz: String,
    acc_fill_sz: String,
    fill_pnl: String,
    cancel_source: String,
    amend_result: String,
    exec_type: String,
    ord_type: OrdType,
}

impl OrdersData {
    pub fn try_into_order_push(self, inst_id: InstId) -> Result<OrderPush> {
        let size = self.sz.parse::<f64>()?;
        let filled_size = self.fill_sz.parse::<f64>()?;
        let acc_filled_size = self.acc_fill_sz.parse::<f64>()?;
        let price = self.fill_pnl.parse::<f64>()?;
        let side = match self.side {
            Side::Buy => true,
            Side::Sell => false,
        };
        let exec_type = match self.exec_type.as_str() {
            "T" => Some(ExecType::T),
            "M" => Some(ExecType::M),
            _ => None,
        };

        let push_type = match (
            filled_size == 0.,
            self.cancel_source.is_empty(),
            self.amend_result.is_empty(),
        ) {
            (false, _, _) => OrderPushType::Fill,
            (_, false, _) => OrderPushType::Canceled,
            (_, _, false) => OrderPushType::Amended,
            _ => OrderPushType::Placed,
        };

        Ok(OrderPush {
            order_id: self.cl_ord_id.parse()?,
            inst_id,
            state: self.state,
            size,
            filled_size,
            acc_filled_size,
            price,
            side,
            ord_type: self.ord_type,
            exec_type,
            push_type,
        })
    }
}
