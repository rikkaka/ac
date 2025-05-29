#![allow(non_snake_case)]
pub mod types;

use core::{pin::Pin, task::Poll};
use std::{task::Context, time::Duration};

use anyhow::Result;
use futures::{Sink, Stream, ready};
use futures_util::SinkExt;
use pin_project::pin_project;
use serde_json::json;
use tokio_tungstenite::{
    WebSocketStream, connect_async,
    tungstenite::{self, Message},
};
use types::{Data, Push};

use crate::{delegate_sink, types::InstId, utils::{Duplex, Heartbeat}};

const PUBLIC_WS_URL: &str = "wss://ws.okx.com:8443/ws/v5/public";
const PRIVATE_WS_URL: &str = "wss://ws.okx.com:8443/ws/v5/private";
const PUBLIC_WS_URL_SIMU: &str = "wss://wspap.okx.com:8443/ws/v5/public";
const PRIVATE_WS_URL_SIMU: &str = "wss://wspap.okx.com:8443/ws/v5/private";

type WsStream = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub enum OkxWsEndpoint {
    Public,
    Private,
    PublicSimu,
    PrivateSimu,
}

impl OkxWsEndpoint {
    pub fn url(&self) -> &str {
        match self {
            OkxWsEndpoint::Public => PUBLIC_WS_URL,
            OkxWsEndpoint::Private => PRIVATE_WS_URL,
            OkxWsEndpoint::PublicSimu => PUBLIC_WS_URL_SIMU,
            OkxWsEndpoint::PrivateSimu => PRIVATE_WS_URL_SIMU,
        }
    }
}

#[pin_project]
pub struct OkxWsStream<S>
where 
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>>,
{
    /// WebSocket 流
    #[pin]
    inner: S,
}

pub async fn connect(endpoint: OkxWsEndpoint) -> Result<impl Duplex<Message, tungstenite::Error, (InstId, Data)>> {
    let (ws_stream, _) = connect_async(endpoint.url()).await?;
    let ws_stream = with_heartbeat(ws_stream);
    Ok(OkxWsStream { inner: ws_stream })
}

impl<S> Sink<Message> for OkxWsStream<S>
where 
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>>,
{
    type Error = tungstenite::Error;

    delegate_sink!(inner, Message);
}

impl<S> Stream for OkxWsStream<S>
where 
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>>
{
    /// 返回 (instrument_id, data)
    type Item = (InstId, Data);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            // 1. 取出下一条消息；若已结束直接返回 Ready(None)
            let Some(msg) = ready!(this.inner.as_mut().poll_next(cx)) else {
                return Poll::Ready(None);
            };

            // 2. 连接层面先处理错误
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("WebSocket error: {e}");
                    return Poll::Ready(None);
                }
            };

            // 3. 只关心文本消息
            let Message::Text(text) = msg else {
                tracing::warn!("Ignore non-text frame");
                continue;
            };

            tracing::debug!("Receive message: {text}");

            // 4. 反序列化 OKX push 帧
            let push: Push = match serde_json::from_str(&text) {
                Ok(p) => p,
                Err(_) => {
                    tracing::info!("Unidentified message: {text}");
                    continue;
                }
            };

            // 5. 事件帧（例如 subscribe、unsubscribe、error 等）
            if push.event.is_some() {
                tracing::info!("Receive event: {push:#?}");
                continue;
            }

            // 6. 数据帧
            match (push.data, push.arg.channel.as_str()) {
                (Some(raw), channel) => match Data::try_from_raw(raw[0], channel) {
                    Ok(data) => return Poll::Ready(Some((push.arg.instId, data))),
                    Err(e) => {
                        tracing::info!("Fail to deserialize data: {e}");
                        continue;
                    }
                },
                _ => {
                    tracing::info!("Push without data: {push:#?}");
                    continue;
                }
            }
        }
    }
}

pub async fn subscribe<S>(ws_sink: &mut S, channel: &str, instrument_id: &str) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let param = json!({
        "op": "subscribe",
        "args": [{
            "channel": channel,
            "instId": instrument_id
        }]
    })
    .to_string();
    ws_sink.send(param.into()).await?;

    Ok(())
}

pub fn with_heartbeat<S>(
    ws_stream: S,
) -> Heartbeat<S>
where
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>> + Unpin,
{
    Heartbeat::new(ws_stream, Duration::from_secs(1), Duration::from_millis(200))
}

