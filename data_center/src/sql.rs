use anyhow::Result;
use lazy_static::lazy_static;
use serde::Serialize;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::okx_api::TradesData;

lazy_static! {
    static ref POOL: PgPool = {
        let pg_host = dotenvy::var("PG_HOST")
            .expect("Please set PG_HOST in the .env or the environment variables");
        PgPoolOptions::new()
            .max_connections(50)
            .connect_lazy(&pg_host)
            .unwrap()
    };
}

#[derive(Debug, Serialize, Clone)]
pub struct Trade {
    ts: i64,
    instrument_id: String,
    price: f64,
    size: f64,
    side: bool,
    order_count: i32,
}

impl From<TradesData> for Trade {
    fn from(value: TradesData) -> Self {
        let ts = value.ts.parse().unwrap();
        let instrument_id = value.instId;
        let price = value.px.parse().unwrap();
        let size = value.sz.parse().unwrap();
        let side = match value.side.as_str() {
            "buy" => true,
            "sell" => false,
            _ => panic!(),
        };
        let order_count = value.count.parse().unwrap();

        Self {
            ts,
            instrument_id,
            price,
            size,
            side,
            order_count,
        }
    }
}

pub async fn insert_trade(trade: &Trade) -> Result<()> {
    sqlx::query!(
        "INSERT INTO okx_trades (ts, instrument_id, price, size, side, order_count) VALUES ($1, $2, $3, $4, $5, $6)",
        trade.ts,
        trade.instrument_id,
        trade.price,
        trade.size,
        trade.side,
        trade.order_count
    )
    .execute(&*POOL)
    .await?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_pg_connection() {
        let row: (i32,) = sqlx::query_as("SELECT 1 AS one")
            .fetch_one(&*POOL)
            .await
            .unwrap();
        assert_eq!(row.0, 1);
    }
}
