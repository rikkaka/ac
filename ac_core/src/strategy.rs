use crate::{ClientEvent, BrokerEvent};

mod single_ticker;
mod calc;

/// D: type for the data
pub trait Strategy<D> {
    fn on_event(&mut self, market_event: &BrokerEvent<D>) -> Vec<ClientEvent>;

    fn on_events<'a, I>(&mut self, market_evnets : I) -> Vec<ClientEvent>
    where
        D: 'a,
        I: Iterator<Item = &'a BrokerEvent<D>>
    {
        let mut orders = Vec::new();
        for event in market_evnets {
            orders.extend(self.on_event(event));
        }
        orders
    }
}

