use std::marker::PhantomData;

use anyhow::Result;
use chrono::Duration;
use futures::{Sink, SinkExt, Stream, StreamExt, stream};
use serde::Serialize;
use tokio_tungstenite::tungstenite;
use utils::Duplex;

pub use super::actions::Action;

use crate::{
    Data,
    okx_api::{
        OkxWsEndpoint,
        actions::{Request, SubscribeArg},
        connect,
        types::Channel,
    },
    sql::{QueryOption, query_bbo},
    types::InstId,
};

// 解析订阅并建立连接，推送数据。还可接收写入以发送消息。
// 推送的是可以直接拿去用的Data。
pub struct Terminal<T> {
    history_stream: Box<dyn Stream<Item = Data>>,
    ws_stream: Box<dyn Duplex<Action, anyhow::Error, Data>>,
    _phantom_data: PhantomData<T>,
}

impl<T> Terminal<T>
where
    T: Serialize + Clone + 'static,
{
    pub async fn new(
        endpoint: OkxWsEndpoint,
        subscribe_action: Action,
        history_duration: Duration,
    ) -> Result<Self> {
        if !matches!(subscribe_action, Action::SubscribeBboTbt(_)) {
            unimplemented!()
        }
        let history_stream = query_bbo(
            QueryOption::new()
                .with_instrument(InstId::EthUsdtSwap)
                .with_duration(history_duration),
        )
        .map(|bbo| Data::Bbo(bbo));
        let ws_stream = connect(endpoint, vec![subscribe_action]).await?;

        Ok(Self {
            history_stream: Box::new(history_stream),
            ws_stream: Box::new(ws_stream),
            _phantom_data: PhantomData,
        })
    }
}
