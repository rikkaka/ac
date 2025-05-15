use anyhow::Result;
use serde::Deserialize;
use serde_json::value::RawValue;

use crate::types::{Bbo, Level, Trade};

#[derive(Debug, Deserialize, Clone)]
pub struct Arg {
    pub channel: String,
    pub instId: String,
}

#[derive(Debug, Deserialize)]
pub struct Push<'a> {
    pub event: Option<String>,
    pub arg: Arg,
    #[serde(borrow)]
    pub data: Option<[&'a RawValue; 1]>,
}

#[derive(Debug, Deserialize)]
pub struct TradesData {
    pub instId: String,
    pub px: String,
    pub sz: String,
    pub side: String,
    pub ts: String,
    pub count: String,
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

pub enum Data {
    Trades(TradesData),
    BboTbt(DepthData),
}

impl TryFrom<TradesData> for Trade {
    type Error = anyhow::Error;

    fn try_from(value: TradesData) -> Result<Self> {
        let ts = value.ts.parse::<i64>()?;
        let price = value.px.parse::<f64>()?;
        let size = value.sz.parse::<f64>()?;
        let side = match value.side.as_str() {
            "buy" => true,
            "sell" => false,
            _ => return Err(anyhow::anyhow!("Invalid side")),
        };
        let order_count = value.count.parse::<i32>()?;

        Ok(Trade {
            ts,
            instrument_id: value.instId,
            price,
            size,
            side,
            order_count,
        })
    }
}

impl TryFrom<DepthData> for Bbo {
    type Error = anyhow::Error;

    fn try_from(value: DepthData) -> Result<Self> {
        let ts = value.ts.parse::<i64>()?;
        let instrument_id = String::new(); // TODO: Get instrument_id from somewhere
        let best_ask = Level {
            price: value.asks[0][0].parse::<f64>()?,
            size: value.asks[0][1].parse::<f64>()?,
            order_count: value.asks[0][3].parse::<i32>()?,
        };
        let best_bid = Level {
            price: value.bids[0][0].parse::<f64>()?,
            size: value.bids[0][1].parse::<f64>()?,
            order_count: value.bids[0][3].parse::<i32>()?,
        };

        Ok(Bbo {
            ts,
            instrument_id,
            best_ask,
            best_bid,
        })
    }
}

impl Data {
    pub fn try_from_raw(value: &RawValue, channel: &str) -> Result<Self> {
        match channel {
            "trades" => {
                let data = serde_json::from_str(value.get())?;
                Ok(Data::Trades(data))
            }
            "bbo-tbt" => {
                let data = serde_json::from_str(value.get())?;
                Ok(Data::BboTbt(data))
            }
            s => {
                tracing::error!("Unimplemented channel: {s}");
                unimplemented!()
            }
        }
    }
}
