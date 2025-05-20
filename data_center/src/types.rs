use sqlx::{postgres::PgRow, FromRow, Row};

#[derive(Debug, Clone)]
pub struct Trade {
    /// Unix millis timestamp
    pub ts: i64,
    pub instrument_id: String,
    pub trade_id: String,
    pub price: f64,
    pub size: f64,
    pub side: bool,
    pub order_count: i32,
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
    pub instrument_id: String,
    pub best_ask: Level,
    pub best_bid: Level,
}

impl FromRow<'_, PgRow> for Trade {
    fn from_row(row: &'_ PgRow) -> Result<Self, sqlx::Error> {
        Ok(Trade {
            ts: row.try_get("ts")?,
            instrument_id: row.try_get("instrument_id")?,
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
            instrument_id: row.try_get("instrument_id")?,
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