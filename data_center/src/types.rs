use std::task::Poll;

use either::Either;
use futures::{Stream, ready};
use pin_project::pin_project;
use smartstring::alias::String;
use sqlx::{FromRow, Row, postgres::PgRow};
use utils::Timestamped;

use crate::okx_api::types::{ExecType, OrderState};

pub use crate::okx_api::types::InstId;

impl InstId {
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            Self::EthUsdtSwap => "ETH-USDT-SWAP",
            Self::BtcUsdtSwap => "BTC-USDT-SWAP",
        }
    }
}

pub enum Data {
    Trade(Trade),
    Bbo(Bbo),
    Order(OrderPush)
}

#[derive(Debug, Clone)]
pub struct Trade {
    /// Unix millis timestamp
    pub ts: i64,
    pub instrument_id: InstId,
    pub trade_id: String,
    pub price: f64,
    pub size: f64,
    pub side: bool,
    pub order_count: i32,
}

impl Timestamped for Trade {
    fn get_ts(&self) -> i64 {
        self.ts
    }
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
    pub bid_price: f64,
    pub bid_size: f64,
    pub bid_order_count: i32,
    pub ask_price: f64,
    pub ask_size: f64,
    pub ask_order_count: i32,
}

impl Timestamped for Bbo {
    fn get_ts(&self) -> i64 {
        self.ts
    }
}

#[derive(Debug, Clone)]
pub struct OrderPush {
    pub order_id: String,
    pub state: OrderState,
    pub size: f64,
    pub filled_size: f64,
    pub acc_filled_size: f64,
    pub price: f64,
    pub side: bool,
    pub exec_type: Option<ExecType>,
    pub is_amended: bool,
}

impl FromRow<'_, PgRow> for Trade {
    fn from_row(row: &'_ PgRow) -> Result<Self, sqlx::Error> {
        Ok(Trade {
            ts: row.try_get("ts")?,
            instrument_id: serde_plain::from_str(row.try_get::<&str, _>("instrument_id")?)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            trade_id: row.try_get::<&str, _>("trade_id")?.into(),
            price: row.try_get("price")?,
            size: row.try_get("size")?,
            side: row.try_get("side")?,
            order_count: row.try_get("order_count")?,
        })
    }
}

impl FromRow<'_, PgRow> for Bbo {
    fn from_row(row: &'_ PgRow) -> Result<Self, sqlx::Error> {
        Ok(Bbo {
            ts: row.try_get("ts")?,
            instrument_id: serde_plain::from_str(row.try_get::<&str, _>("instrument_id")?)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
                ask_price: row.try_get("price_ask")?,
                ask_size: row.try_get("size_ask")?,
                ask_order_count: row.try_get("order_count_ask")?,
                bid_price: row.try_get("price_bid")?,
                bid_size: row.try_get("size_bid")?,
                bid_order_count: row.try_get("order_count_bid")?,
        })
    }
}

impl From<Bbo> for Either<Bbo, Trade> {
    fn from(value: Bbo) -> Self {
        Self::Left(value)
    }
}

impl From<Trade> for Either<Bbo, Trade> {
    fn from(value: Trade) -> Self {
        Self::Right(value)
    }
}

#[derive(Debug)]
pub struct Level1 {
    bbo: Bbo,
    last_price: f64,
    volume: f64,
    buying_volume: f64,
    selling_volume: f64,
}

#[pin_project]
pub struct Level1Stream<S> {
    #[pin]
    bbo_trade_stream: S,

    weighted_price: f64,
    volume: f64,
    buying_volume: f64,
    selling_volume: f64,
}

impl<S> Level1Stream<S>
where
    S: Stream<Item = Either<Bbo, Trade>>,
{
    pub fn new(bbo_trade_stream: S) -> Self {
        Self {
            bbo_trade_stream,
            weighted_price: 0.,
            volume: 0.,
            buying_volume: 0.,
            selling_volume: 0.,
        }
    }
}

impl<S> Stream for Level1Stream<S>
where
    S: Stream<Item = Either<Bbo, Trade>>,
{
    type Item = Level1;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            let Some(bbo_trade) = ready!(this.bbo_trade_stream.as_mut().poll_next(cx)) else {
                return Poll::Ready(None);
            };

            match bbo_trade {
                Either::Left(bbo) => {
                    let level1 = Level1 {
                        bbo,
                        last_price: *this.weighted_price,
                        volume: *this.volume,
                        buying_volume: *this.buying_volume,
                        selling_volume: *this.selling_volume,
                    };
                    *this.weighted_price = 0.;
                    *this.volume = 0.;
                    return Poll::Ready(Some(level1));
                }
                Either::Right(trade) => {
                    let size = trade.size * trade.order_count as f64;
                    *this.weighted_price = (*this.weighted_price * *this.volume
                        + trade.price * size)
                        / (*this.volume + size);
                    *this.volume += size;
                    if trade.side {
                        *this.buying_volume += trade.size
                    } else {
                        *this.selling_volume += trade.size
                    }
                }
            }
        }
    }
}
