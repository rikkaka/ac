use std::task::Poll;

use either::Either;
use futures::{Stream, ready};
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use smartstring::alias::String;
use sqlx::{FromRow, Row, postgres::PgRow};
use utils::Timestamped;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
pub enum InstId {
    #[default]
    EthUsdtSwap,
    BtcUsdtSwap,
}

impl InstId {
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            Self::EthUsdtSwap => "ETH-USDT-SWAP",
            Self::BtcUsdtSwap => "BTC-USDT-SWAP",
        }
    }
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
    pub best_ask: Level,
    pub best_bid: Level,
}

impl Timestamped for Bbo {
    fn get_ts(&self) -> i64 {
        self.ts
    }
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
            best_ask: Level {
                price: row.try_get("price_ask")?,
                size: row.try_get("size_ask")?,
                order_count: row.try_get("order_count_ask")?,
            },
            best_bid: Level {
                price: row.try_get("price_bid")?,
                size: row.try_get("size_bid")?,
                order_count: row.try_get("order_count_bid")?,
            },
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
