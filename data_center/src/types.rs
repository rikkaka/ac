use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Trade {
    pub ts: i64,
    pub instrument_id: String,
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

#[derive(Debug, Clone)]
pub struct Bbo {
    pub ts: i64,
    pub instrument_id: String,
    pub best_ask: Level,
    pub best_bid: Level,
}
