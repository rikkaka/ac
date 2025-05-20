//! 为进行回测，我们需要拿收集的数据，创造模拟的交易环境。
//! 例如，一个策略若需要trade+bbo的数据，那么回测时也应该ts by ts的接收trade + bbo的数据。
//! 这个mod的基本功能，是对于 数据的提供者 和 strategy ，计算strategy的表现。
//! A strategy receives data and returns orders. Thus this mod need to simulate
//! an environment where the results of the sequence of orders can be evaluated.
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Sink, Stream, ready};
use pin_project::pin_project;
use rustc_hash::FxHashMap;

use crate::{
    Broker, BrokerEvent, ClientEvent, DataProvider, Fill, InstId, LimitOrder, Order, OrderId,
    data::Bbo, strategy::Strategy, utils::fill_market_order,
};

#[pin_project]
pub struct BboBroker<D, DP> {
    limit_orders: FxHashMap<OrderId, LimitOrder>,
    broker_events_buf: VecDeque<BrokerEvent<D>>,
    inst_bbo: FxHashMap<InstId, Bbo>,
    #[pin]
    data_provider: DP,
}

impl<DP> BboBroker<Bbo, DP>
where
    DP: DataProvider<Bbo>,
{
    async fn new(mut data_provider: DP) -> Self {
        let mut inst_bbo = FxHashMap::default();
        while inst_bbo.len() < data_provider.instruments().len() {
            if let Some(bbo) = data_provider.next().await {
                inst_bbo.insert(bbo.instrument_id.clone(), bbo);
            }
        }

        Self {
            limit_orders: Default::default(),
            broker_events_buf: Default::default(),
            inst_bbo,
            data_provider,
        }
    }

    fn on_client_event(&mut self, client_event: ClientEvent) {
        match client_event {
            ClientEvent::PlaceOrder(order) => {
                match order {
                    Order::Market(order) => {
                        let fill = fill_market_order(&order, &self.inst_bbo);
                        self.broker_events_buf.push_back(BrokerEvent::Fill(fill));
                    }
                    Order::Limit(order) => {
                        // Handle limit order
                        self.limit_orders.insert(order.order_id, order);
                    }
                }
            }
            ClientEvent::ModifyOrder(order) => {
                if let Order::Limit(order) = order {
                    if let Some(existing_order) = self.limit_orders.get_mut(&order.order_id) {
                        existing_order.price = order.price;
                        existing_order.size = order.size;
                    }
                }
            }
            ClientEvent::CancelOrder(order_id) => {
                self.limit_orders.remove(&order_id);
            }
        }
    }

    fn on_client_events<I>(&mut self, client_events: I)
    where
        I: Iterator<Item = ClientEvent>,
    {
        for event in client_events {
            self.on_client_event(event);
        }
    }
}

impl<DP> Sink<Vec<ClientEvent>> for BboBroker<Bbo, DP>
where
    DP: DataProvider<Bbo>,
{
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Vec<ClientEvent>) -> Result<(), Self::Error> {
        let this = self.get_mut();
        this.on_client_events(item.into_iter());
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

impl<DP> Stream for BboBroker<Bbo, DP>
where
    DP: DataProvider<Bbo>,
{
    type Item = BrokerEvent<Bbo>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        if let Some(event) = this.broker_events_buf.pop_front() {
            return Poll::Ready(Some(event));
        }

        if let Some(data) = ready!(this.data_provider.as_mut().poll_next(cx)) {
            return Poll::Ready(Some(BrokerEvent::Data(data)));
        } else {
            return Poll::Ready(None);
        }
    }
}

impl<DP: DataProvider<Bbo>> Broker<Bbo, ()> for BboBroker<Bbo, DP> {}

