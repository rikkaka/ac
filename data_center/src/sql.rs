use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use either::Either;
use futures::{Stream, StreamExt};
use once_cell::sync::Lazy;
use sqlx::{
    Postgres,
    postgres::{PgPool, PgPoolOptions},
};
use utils::TsStreamMerger;

use crate::{
    CONFIG,
    types::{Bbo, InstId, Level1, Level1Stream, Trade},
};

pub static POOL: Lazy<PgPool> = Lazy::new(|| {
    let pg_host = &CONFIG.pg_host;
    PgPoolOptions::new()
        .max_connections(50)
        .connect_lazy(&pg_host)
        .unwrap()
});

#[derive(Default, Clone)]
pub struct QueryOption {
    pub instruments: Vec<InstId>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

impl QueryOption {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_instrument(mut self, inst_id: InstId) -> Self {
        self.instruments.push(inst_id);
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        let end = Utc::now();
        let start = end - duration;
        self.start = Some(start);
        self
    }
}

pub async fn insert_trade(trade: &Trade) -> Result<()> {
    sqlx::query!(
        "INSERT INTO okx_trades 
        (ts, instrument_id, trade_id, price, size, side, order_count)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT DO NOTHING",
        trade.ts,
        trade.instrument_id.as_str(),
        trade.trade_id.as_str(),
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
        bbo.instrument_id.as_str(),
        bbo.ask_price,
        bbo.ask_size,
        bbo.ask_order_count,
        bbo.bid_price,
        bbo.bid_size,
        bbo.bid_order_count
    )
    .execute(&*POOL)
    .await?;

    Ok(())
}

pub fn query_trade(query_option: QueryOption) -> impl Stream<Item = Trade> + Send {
    async_stream::stream! {
        let mut builder = sqlx::QueryBuilder::<Postgres>::new(
            "SELECT * FROM okx_trades WHERE 1=1"
        );

        if !query_option.instruments.is_empty() {
            builder.push(" AND instrument_id IN (");
            let mut sep = builder.separated(", ");
            for id in &query_option.instruments {
                sep.push_bind(id.as_str());
            }
            sep.push_unseparated(")");
        }

        if let Some(t) = query_option.start {
            builder.push(" AND ts >= ");
            builder.push_bind(t.timestamp_millis());
        }
        if let Some(t) = query_option.end {
            builder.push(" AND ts <= ");
            builder.push_bind(t.timestamp_millis());
        }

        builder.push(" ORDER BY ts ASC");

        let mut rows =
            builder.build_query_as::<Trade>()
                   .fetch(&*POOL);

        while let Some(row) = rows.next().await {
            match row {
                Ok(row) => yield row,
                Err(e) => tracing::error!("Error fetching trades: {:?}", e),
            }
        }
    }
}

pub fn query_bbo(query_option: QueryOption) -> impl Stream<Item = Bbo> + Send {
    async_stream::stream! {
        let mut builder = sqlx::QueryBuilder::<Postgres>::new(
            "SELECT * FROM okx_bbo WHERE 1=1"
        );

        if !query_option.instruments.is_empty() {
            builder.push(" AND instrument_id IN (");
            let mut sep = builder.separated(", ");
            for id in &query_option.instruments {
                sep.push_bind(id.as_str());
            }
            sep.push_unseparated(")");
        }

        if let Some(t) = query_option.start {
            builder.push(" AND ts >= ");
            builder.push_bind(t.timestamp_millis());
        }
        if let Some(t) = query_option.end {
            builder.push(" AND ts <= ");
            builder.push_bind(t.timestamp_millis());
        }

        builder.push(" ORDER BY ts ASC");

        let mut rows =
            builder.build_query_as::<Bbo>()
                   .fetch(&*POOL);

        while let Some(row) = rows.next().await {
            match row {
                Ok(row) => yield row,
                Err(e) => println!("Error fetching BBO: {:?}", e),
            }
        }
    }
}

pub fn query_bbo_trade(query_option: QueryOption) -> impl Stream<Item = Either<Bbo, Trade>> + Send {
    let bbo_stream = query_bbo(query_option.clone());
    let trade_stream = query_trade(query_option);

    TsStreamMerger::new(bbo_stream, trade_stream)
}

pub fn query_level1(query_option: QueryOption) -> impl Stream<Item = Level1> + Send {
    let bbo_trade_stream = query_bbo_trade(query_option);

    Level1Stream::new(bbo_trade_stream)
}
