pub mod actions;
pub(crate) mod pushes;
pub mod terminal;
pub(crate) mod types;

use core::{pin::Pin, task::Poll};
use std::{task::Context, time::Duration};

use crate::{
    okx_api::actions::{Action, SubscribeArg},
    types::Data,
};
use anyhow::{Result, anyhow};
use futures::{Sink, Stream, ready};
use futures_util::SinkExt;
use pin_project::pin_project;
use pushes::Push;
use serde::Serialize;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Message},
};
use utils::Duplex;

use crate::{
    delegate_sink,
    okx_api::actions::Request,
    utils::{AutoReconnect, Heartbeat},
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
    subscribe_actions: Vec<Action>,
) -> Result<impl Duplex<Action, anyhow::Error, Data>> {
    let make_connection = move || {
        let subscribe_actions = subscribe_actions.clone();
        async move {
            let (ws_stream, _) = connect_async(endpoint.url()).await?;
            let ws_stream = with_heartbeat(ws_stream);
            let mut ws_stream = OkxWsStream { inner: ws_stream };
            for request in subscribe_actions {
                ws_stream.send(request).await?
            }
            Ok(ws_stream)
        }
    };

    let ws_stream = AutoReconnect::new(make_connection).await?;
    let ws_stream = Box::pin(ws_stream);
    Ok(ws_stream)
}

impl<S> Sink<Action> for OkxWsStream<S>
where
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>>,
{
    type Error = anyhow::Error;

    fn poll_ready(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        let mut this = self.project();
        ready!(this.inner.as_mut().poll_ready(cx))?;
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Action) -> std::result::Result<(), Self::Error> {
        let mut this = self.project();
        let message = item.to_message();
        tracing::debug!("Send message: {message:?}");
        this.inner
            .as_mut()
            .start_send(message)
            .map_err(|e| anyhow!("Failed to send message: {e}"))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        let mut this = self.project();
        ready!(this.inner.as_mut().poll_flush(cx))?;
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        let mut this = self.project();
        ready!(this.inner.as_mut().poll_close(cx))?;
        Poll::Ready(Ok(()))
    }
}

impl<S> Stream for OkxWsStream<S>
where
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>>,
{
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
            match Data::try_from_okx_push(push) {
                Ok(data) => return Poll::Ready(Some(data)),
                Err(e) => {
                    tracing::info!("Fail to deserialize data: {e}");
                    continue;
                }
            }
        }
    }
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
