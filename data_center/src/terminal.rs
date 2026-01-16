use std::{pin::Pin, task::Poll};

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use futures::{Sink, Stream, StreamExt, ready};
use pin_project::pin_project;
use utils::Duplex;

use crate::{
    Data, delegate_sink,
    okx_api::{OkxWsEndpoint, connect, connect_adapted},
    sql::{QueryOption, query_bbo},
    types::{Action, InstId},
};

// 解析订阅并建立连接，推送数据。还可接收写入以发送消息。
// 推送的是可以直接拿去用的Data。
#[pin_project]
pub struct Terminal {
    history_stream: Pin<Box<dyn Stream<Item = Data>>>,
    is_history_ended: bool,

    #[pin]
    ws_stream: Box<dyn Duplex<Action, anyhow::Error, Data>>,

    last_order_time: DateTime<Utc>,
}

impl Terminal {
    pub async fn new_okx(
        is_simu: bool,
        subscribe_actions: Vec<Action>,
        history_duration: Duration,
    ) -> Result<Self> {
        for action in &subscribe_actions {
            if !matches!(
                action,
                Action::SubscribeOrders(_) | Action::SubscribeBboTbt(_)
            ) {
                unimplemented!()
            }
        };
        let history_stream = query_bbo(
            QueryOption::new()
                .with_instrument(InstId::EthUsdtSwap)
                .with_duration(history_duration),
        )
        .map(Data::Bbo);
        let ws_stream = connect_adapted(subscribe_actions, is_simu).await?;

        Ok(Self {
            history_stream: Box::pin(history_stream),
            is_history_ended: false,
            ws_stream: Box::new(ws_stream),
            last_order_time: DateTime::<Utc>::from_timestamp_nanos(0),
        })
    }
}

impl Stream for Terminal {
    type Item = Data;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if !*this.is_history_ended {
            match ready!(this.history_stream.as_mut().poll_next(cx)) {
                Some(data) => return Poll::Ready(Some(data)),
                None => *this.is_history_ended = true,
            };
        }

        let data = ready!(this.ws_stream.as_mut().poll_next(cx));
        return Poll::Ready(data);
    }
}

impl Sink<Action> for Terminal {
    type Error = anyhow::Error;

    delegate_sink!(ws_stream, Action);
}
