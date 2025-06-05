#![allow(non_snake_case)]
pub mod types;

use core::{pin::Pin, task::Poll};
use std::{task::Context, time::Duration};

use anyhow::Result;
use futures::{Sink, Stream, ready};
use futures_util::SinkExt;
use pin_project::pin_project;
use serde::Serialize;
use serde_json::json;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Message},
};
use types::{Data, Push};

use crate::{
    delegate_sink,
    okx_api::types::{OrderRequest, SubscribeArg},
    utils::{AutoReconnect, Duplex, Heartbeat},
};

const PUBLIC_WS_URL: &str = "wss://ws.okx.com:8443/ws/v5/public";
const PRIVATE_WS_URL: &str = "wss://ws.okx.com:8443/ws/v5/private";
const PUBLIC_WS_URL_SIMU: &str = "wss://wspap.okx.com:8443/ws/v5/public";
const PRIVATE_WS_URL_SIMU: &str = "wss://wspap.okx.com:8443/ws/v5/private";

#[derive(Clone, Copy)]
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

pub async fn connect(
    endpoint: OkxWsEndpoint,
    subscribe_args: Vec<SubscribeArg<'static>>,
) -> Result<impl Duplex<Message, tungstenite::Error, Data>> {
    let make_connection = move || {
        let subscribe_args = subscribe_args.clone();
        async move {
            let (ws_stream, _) = connect_async(endpoint.url()).await?;
            let mut ws_stream = with_heartbeat(ws_stream);
            subscribe(&mut ws_stream, &subscribe_args).await?;
            Ok::<_, anyhow::Error>(OkxWsStream { inner: ws_stream })
        }
    };

    let ws_stream = AutoReconnect::new(make_connection).await?;
    let ws_stream = Box::pin(ws_stream);
    Ok(ws_stream)
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
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>>,
{
    /// 返回 (instrument_id, data)
    type Item = Data;

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
            match push.data {
                Some(raw) => match Data::try_from_raw(raw[0], push.arg) {
                    Ok(data) => return Poll::Ready(Some(data)),
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

pub async fn subscribe<S>(ws_sink: &mut S, args: &[SubscribeArg<'_>]) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    for arg in args {
        let param = json!({
            "op": "subscribe",
            "args": [arg]
        })
        .to_string();
        ws_sink.send(param.into()).await?;
    }

    Ok(())
}

pub async fn order_request<S, OA: Serialize>(ws_sink: &mut S, param: OrderRequest<OA>) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let param = serde_json::to_string(&param)?;
    ws_sink.send(param.into()).await?;

    Ok(())
}

pub fn with_heartbeat<S>(ws_stream: S) -> Heartbeat<S>
where
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>> + Unpin,
{
    Heartbeat::new(
        ws_stream,
        Duration::from_millis(600),
        Duration::from_millis(100),
    )
}
