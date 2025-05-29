use std::marker::PhantomData;

use crate::{
    BrokerEvent, ClientEvent
};

mod calc;
mod executors;
mod single_ticker;

/// D: type for the data
pub trait Strategy<D> {
    fn on_event(&mut self, market_event: &BrokerEvent<D>) -> Vec<ClientEvent>;

    fn on_events<'a, I>(&mut self, market_evnets: I) -> Vec<ClientEvent>
    where
        D: 'a,
        I: Iterator<Item = &'a BrokerEvent<D>>,
    {
        let mut orders = Vec::new();
        for event in market_evnets {
            orders.extend(self.on_event(event));
        }
        orders
    }
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
    _phantom_data: PhantomData<D>,
}

impl<Sg, Ex, D> Strategy<D> for SignalExecuteStrategy<Sg, Ex, D>
where
    Sg: Signaler<D>,
    Ex: Executor<D>,
{
    fn on_event(&mut self, broker_event: &BrokerEvent<D>) -> Vec<ClientEvent> {
        self.executor.update(broker_event);
        if let Some(data) = broker_event.to_data() {
            let signal = self.signaler.on_data(data);
            self.executor.on_signal(signal)
        } else {
            vec![]
        }
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
            _phantom_data: PhantomData,
        }
    }
}
