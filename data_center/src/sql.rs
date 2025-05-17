use anyhow::Result;
use lazy_static::lazy_static;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::types::{Bbo, Trade};

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

pub async fn insert_trade(trade: &Trade) -> Result<()> {
    sqlx::query!(
        "INSERT INTO okx_trades 
        (ts, instrument_id, price, size, side, order_count)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT DO NOTHING",
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

pub async fn insert_bbo(bbo: &Bbo) -> Result<()> {
    sqlx::query!(
        "INSERT INTO okx_bbo 
        (ts, instrument_id, price_ask, size_ask, order_count_ask, price_bid, size_bid, order_count_bid)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT DO NOTHING",
        bbo.ts,
        bbo.instrument_id,
        bbo.best_ask.price,
        bbo.best_ask.size,
        bbo.best_ask.order_count,
        bbo.best_bid.price,
        bbo.best_bid.size,
        bbo.best_bid.order_count
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
