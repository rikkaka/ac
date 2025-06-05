use std::marker::PhantomData;

use chrono::Duration;

use crate::{BrokerEvent, ClientEvent, Timestamp};

mod calc;
mod executors;
pub mod single_ticker;

/// D: type for the data
///
/// Strategy内部不维护订单信息。每次下单后，等待服务器返回订单信息。假设服务器的订单信息可以在cooling_duration内返回。在每次Client Event后，在cooling_duraton，不做出任何行动。
///
/// Broker提供Order与AllOrder。在收到Order类型的BrokerEvent后，更新相应Order的状态。在收到AllOrder类型的BrokerEvent后，重置自己的Order。
/// 一般来说，在下单、成交事件发生时，Broker推送Order。在连接断开后，Broker会请求一次全部Order的信息，并推送。
pub trait Strategy<D> {
    fn on_event_inner(&mut self, broker_event: &BrokerEvent<D>) -> Vec<ClientEvent>;

    fn get_cooling_duration(&self) -> Duration {
        Duration::seconds(1)
    }

    fn last_event_ts_mut_ptr(&mut self) -> &mut Timestamp;
    fn last_event_ts(&self) -> Timestamp;

    fn reset_last_event_ts(&mut self, now: Timestamp) {
        let ts_ptr = self.last_event_ts_mut_ptr();
        *ts_ptr = now;
    }

    fn is_cooling(&self, now: Timestamp) -> bool {
        let last_ts = self.last_event_ts();
        let cooling_duration = self.get_cooling_duration().num_milliseconds() as u64;
        now - last_ts < cooling_duration
    }

    fn on_event(&mut self, broker_event: &BrokerEvent<D>, now: Timestamp) -> Vec<ClientEvent> {
        let client_events = self.on_event_inner(broker_event);
        if self.is_cooling(now) {
            return vec![];
        }
        if !client_events.is_empty() {
            self.reset_last_event_ts(now);
        }
        client_events
    }

    // fn on_events<'a, I>(&mut self, market_evnets: I, now: Timestamp) -> Vec<ClientEvent>
    // where
    //     D: 'a,
    //     I: Iterator<Item = &'a BrokerEvent<D>>,
    // {
    //     let mut orders = Vec::new();
    //     for event in market_evnets {
    //         orders.extend(self.on_event(event, now));
    //     }
    //     orders
    // }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Long,
    Short,
}

impl Signal {
    pub fn is_long(&self) -> bool {
        matches!(self, Signal::Long)
    }

    pub fn is_short(&self) -> bool {
        matches!(self, Signal::Short)
    }
}

pub trait Signaler<D> {
    fn on_data(&mut self, data: &D) -> Option<Signal>;
}

pub trait Executor<D> {
    fn update(&mut self, broker_event: &BrokerEvent<D>);
    fn on_signal(&mut self, signal: Option<Signal>) -> Vec<ClientEvent>;
}

pub struct SignalExecuteStrategy<Sg, Ex, D> {
    signaler: Sg,
    executor: Ex,
    last_event_ts: Timestamp,
    _phantom_data: PhantomData<D>,
}

impl<Sg, Ex, D> Strategy<D> for SignalExecuteStrategy<Sg, Ex, D>
where
    Sg: Signaler<D>,
    Ex: Executor<D>,
{
    fn on_event_inner(&mut self, broker_event: &BrokerEvent<D>) -> Vec<ClientEvent> {
        self.executor.update(broker_event);
        if let Some(data) = broker_event.to_data() {
            let signal = self.signaler.on_data(data);
            self.executor.on_signal(signal)
        } else {
            vec![]
        }
    }

    fn last_event_ts_mut_ptr(&mut self) -> &mut Timestamp {
        &mut self.last_event_ts
    }

    fn last_event_ts(&self) -> Timestamp {
        self.last_event_ts
    }
}

impl<Sg, Ex, D> SignalExecuteStrategy<Sg, Ex, D>
where
    Sg: Signaler<D>,
    Ex: Executor<D>,
{
    pub fn new(signaler: Sg, executor: Ex) -> Self {
        Self {
            signaler,
            executor,
            last_event_ts: 0,
            _phantom_data: PhantomData,
        }
    }
}
