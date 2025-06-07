pub mod actions;
pub(crate) mod pushes;
pub(crate) mod types;

use core::{pin::Pin, task::Poll};
use std::{task::Context, time::Duration};

use crate::{
    CONFIG,
    types::{Action, Data},
};
use anyhow::{Result, anyhow, bail};
use base64::Engine;
use chrono::Utc;
use futures::{Sink, Stream, ready};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use pin_project::pin_project;
use pushes::Push;
use sha2::Sha256;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Message},
};
use utils::Duplex;

use crate::utils::{AutoReconnect, Heartbeat};

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
    pub fn is_private(&self) -> bool {
        matches!(self, OkxWsEndpoint::Private | OkxWsEndpoint::PrivateSimu)
    }
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
    #[pin]
    inner: S,
}

impl<S> OkxWsStream<S>
where
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>>,
{
    async fn login(&mut self) -> Result<()> {
        dotenvy::dotenv_override()
            .expect("Please set PG_HOST in the .env or the environment variables");
        let api_key = &CONFIG.api_key;
        let passphrase = &CONFIG.passphrase;
        let secret_key = &CONFIG.secret_key;

        /// 生成签名：Base64( HMAC-SHA256( timestamp + METHOD + REQUEST_PATH, SECRET_KEY ) )
        fn build_sign(secret_key: &str, timestamp: i64) -> String {
            // 拼接待签名字符串
            let payload = format!("{timestamp}GET/users/self/verify");

            // 计算 HMAC-SHA256
            let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes()).unwrap();
            mac.update(payload.as_bytes());
            let result = mac.finalize().into_bytes();

            // Base64 编码
            base64::engine::general_purpose::STANDARD.encode(result)
        }
        let timestamp = Utc::now().timestamp();
        let sign = build_sign(&secret_key, timestamp);

        let login_message = serde_json::json!({
            "op": "login",
            "args": [{
                "apiKey": api_key,
                "passphrase": passphrase,
                "sign": sign,
                "timestamp": timestamp,
            }]
        });

        tracing::debug!("Send login message: {login_message}");
        self.inner
            .send(login_message.to_string().into())
            .await
            .map_err(|e| anyhow!("Failed to send login message: {e}"))?;
        let msg = self.inner.next().await.unwrap().unwrap();
        let msg: serde_json::Value = serde_json::from_str(&msg.to_string()).unwrap();
        let event = msg["event"].as_str().unwrap();
        if event != "login" {
            bail!("Failed to login: {msg:#?}")
        }

        Ok(())
    }
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
            if endpoint.is_private() {
                ws_stream.login().await?;
            }
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
                    tracing::info!("Fail to convert push to data: {e}");
                    continue;
                }
            }
        }
    }
}

/// okx的private和pubic只能接受限定频道的订阅。该struct的Sink能够针对action，send向给定的订阅。
#[pin_project(project = OkxWsStreamAdaptedProj)]
pub struct OkxWsStreamAdapted<S> {
    #[pin]
    public: S,
    #[pin]
    private: S,
}

impl Action {
    fn is_private(&self) -> bool {
        match self {
            Action::SubscribeTrades(_) | Action::SubscribeBboTbt(_) => false,
            Action::SubscribeOrders(_)
            | Action::LimitOrder { .. }
            | Action::MarketOrder { .. }
            | Action::AmendOrder { .. }
            | Action::CancelOrder { .. } => true,
        }
    }
}

impl<S> Stream for OkxWsStreamAdapted<S>
where
    S: Duplex<Action, anyhow::Error, Data>,
{
    type Item = <S as Stream>::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        if let Some(item) = ready!(this.public.as_mut().poll_next(cx)) {
            return Poll::Ready(Some(item));
        }
        if let Some(item) = ready!(this.private.as_mut().poll_next(cx)) {
            return Poll::Ready(Some(item));
        }
        Poll::Ready(None)
    }
}

impl<S> Sink<Action> for OkxWsStreamAdapted<S>
where
    S: Duplex<Action, anyhow::Error, Data>,
{
    type Error = <S as Sink<Action>>::Error;

    fn poll_ready(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        let mut this = self.project();
        ready!(this.public.as_mut().poll_ready(cx))?;
        ready!(this.private.as_mut().poll_ready(cx))?;
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Action) -> std::result::Result<(), Self::Error> {
        let this = self.project();
        let mut sink = if item.is_private() {
            this.private
        } else {
            this.public
        };
        sink.as_mut().start_send(item).map_err(|e| {
            tracing::error!("Failed to send message: {e}");
            e
        })
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        let mut this = self.project();
        ready!(this.public.as_mut().poll_flush(cx))?;
        ready!(this.private.as_mut().poll_flush(cx))?;
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        let mut this = self.project();
        ready!(this.public.as_mut().poll_close(cx))?;
        ready!(this.private.as_mut().poll_close(cx))?;
        Poll::Ready(Ok(()))
    }
}

pub async fn connect_adapted(
    subscribe_actions: Vec<Action>,
    is_simu: bool,
) -> Result<impl Duplex<Action, anyhow::Error, Data>> {
    let (public_endpoint, private_endpoint) = if is_simu {
        (OkxWsEndpoint::PublicSimu, OkxWsEndpoint::PrivateSimu)
    } else {
        (OkxWsEndpoint::Public, OkxWsEndpoint::Private)
    };
    let (private_actions, public_actions): (Vec<_>, Vec<_>) = subscribe_actions
        .into_iter()
        .partition(|action| action.is_private());
    let public_ws = connect(public_endpoint, public_actions).await?;
    let private_ws = connect(private_endpoint, private_actions).await?;
    let adapted_ws = OkxWsStreamAdapted {
        public: public_ws,
        private: private_ws,
    };
    Ok(adapted_ws)
}

pub fn with_heartbeat<S>(ws_stream: S) -> Heartbeat<S>
where
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>> + Unpin,
{
    Heartbeat::new(
        ws_stream,
        Duration::from_millis(CONFIG.heartbeat_interval),
        Duration::from_millis(CONFIG.heartbeat_timeout),
    )
}
