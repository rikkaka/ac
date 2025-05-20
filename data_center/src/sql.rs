use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use futures::{Stream, StreamExt};
use lazy_static::lazy_static;
use sqlx::{
    Postgres,
    postgres::{PgPool, PgPoolOptions},
};

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
        VALUES ($1, $2, $3, $4, $5, $6)",
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

#[derive(Default)]
pub struct TradeQuerier {
    instrument_id: Vec<String>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

impl TradeQuerier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_instrument_id(mut self, instrument_id: String) -> Self {
        self.instrument_id.push(instrument_id);
        self
    }

    pub fn with_start(mut self, start: DateTime<Utc>) -> Self {
        self.start.replace(start);
        self
    }

    pub fn with_end(mut self, end: DateTime<Utc>) -> Self {
        self.end.replace(end);
        self
    }

    pub fn with_latest(self, duration: Duration) -> Self {
        let now = Utc::now();
        let start = now - duration;
        self.with_start(start)
    }

    pub fn query(&self) -> impl Stream<Item = Result<Trade, sqlx::Error>> {
        let ids = self.instrument_id.clone();
        let start = self.start;
        let end = self.end;

        async_stream::stream! {
            let mut builder = sqlx::QueryBuilder::<Postgres>::new(
                "SELECT * FROM okx_trades WHERE 1=1"
            );

            if !ids.is_empty() {
                builder.push(" AND instrument_id IN (");
                let mut sep = builder.separated(", ");
                for id in &ids {
                    sep.push_bind(id);
                }
                sep.push_unseparated(")");
            }

            if let Some(t) = start {
                builder.push(" AND ts >= ");
                builder.push_bind(t.timestamp_millis());
            }
            if let Some(t) = end {
                builder.push(" AND ts <= ");
                builder.push_bind(t.timestamp_millis());
            }

            builder.push(" ORDER BY ts DESC");

            // 真正执行
            let mut rows =
                builder.build_query_as::<Trade>()
                       .fetch(&*POOL);

            while let Some(row) = rows.next().await {
                yield row;      // 往外一个个发
            }
        }
    }
}

pub async fn insert_bbo(bbo: &Bbo) -> Result<()> {
    sqlx::query!(
        "INSERT INTO okx_bbo 
        (ts, instrument_id, price_ask, size_ask, order_count_ask, price_bid, size_bid, order_count_bid)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
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

#[derive(Default)]
pub struct BboQuerier {
    instrument_id: Vec<String>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

impl BboQuerier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_instrument_id(mut self, instrument_id: String) -> Self {
        self.instrument_id.push(instrument_id);
        self
    }

    pub fn with_start(mut self, start: DateTime<Utc>) -> Self {
        self.start.replace(start);
        self
    }

    pub fn with_end(mut self, end: DateTime<Utc>) -> Self {
        self.end.replace(end);
        self
    }

    pub fn with_latest(self, duration: Duration) -> Self {
        let now = Utc::now();
        let start = now - duration;
        self.with_start(start)
    }

    pub fn query(&self) -> impl Stream<Item = Result<Bbo, sqlx::Error>> {
        let ids = self.instrument_id.clone();
        let start = self.start;
        let end = self.end;

        async_stream::stream! {
            let mut builder = sqlx::QueryBuilder::<Postgres>::new(
                "SELECT * FROM okx_bbo WHERE 1=1"
            );

            if !ids.is_empty() {
                builder.push(" AND instrument_id IN (");
                let mut sep = builder.separated(", ");
                for id in &ids {
                    sep.push_bind(id);
                }
                sep.push_unseparated(")");
            }

            if let Some(t) = start {
                builder.push(" AND ts >= ");
                builder.push_bind(t.timestamp_millis());
            }
            if let Some(t) = end {
                builder.push(" AND ts <= ");
                builder.push_bind(t.timestamp_millis());
            }

            builder.push(" ORDER BY ts DESC");

            // 真正执行
            let mut rows =
                builder.build_query_as::<Bbo>()
                       .fetch(&*POOL);

            while let Some(row) = rows.next().await {
                yield row;      // 往外一个个发
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
}
