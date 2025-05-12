#![allow(non_snake_case)]
use anyhow::{Ok, Result, anyhow, bail};
use futures_util::{SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite::Message};

const PUBLIC_WS_URL: &str = "wss://ws.okx.com:8443/ws/v5/public";
const PRIVATE_WS_URL: &str = "wss://ws.okx.com:8443/ws/v5/private";
const PUBLIC_WS_URL_SIMU: &str = "wss://wspap.okx.com:8443/ws/v5/public";
const PRIVATE_WS_URL_SIMU: &str = "wss://wspap.okx.com:8443/ws/v5/private";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradesData {
    pub instId: String,
    pub px: String,
    pub sz: String,
    pub side: String,
    pub ts: String,
    pub count: String,
}

pub async fn subscribe(
    channel: &str,
    instrument_id: &str,
) -> Result<impl Stream<Item = Value>> {
    let (mut ws_stream, _) = connect_async(PUBLIC_WS_URL).await?;

    let param = json!({
        "op": "subscribe",
        "args": [{
            "channel": channel,
            "instId": instrument_id
        }]
    })
    .to_string();
    ws_stream.send(param.into()).await?;

    let mut ws_stream = ws_stream.map(|msg| {
        let msg = msg.unwrap().to_string();
        serde_json::from_str::<Value>(&msg).unwrap()
    });

    let msg = ws_stream
        .next()
        .await
        .ok_or(anyhow!("Websocket receives nothing"))?;

    let event = msg.get("event").ok_or(anyhow!("Receive no event"))?;
    if event != "subscribe" {
        bail!("Event not subscribe")
    }

    Ok(ws_stream)
}

pub async fn subscribe_trades(instrument_id: &str) -> Result<impl Stream<Item = TradesData>> {
    let ws_stream = subscribe("trades", instrument_id).await?;
    let ws_stream = ws_stream.map(|value| {
        let value_data = value.get("data").unwrap().get(0).unwrap().to_owned();
        serde_json::from_value::<TradesData>(value_data).unwrap()
    });

    Ok(ws_stream)
}