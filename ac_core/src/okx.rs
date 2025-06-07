use chrono::Duration;
use data_center::{
    Action, Terminal,
    types::{InstId, Side},
};
use futures::{SinkExt, StreamExt};

use crate::{Broker, ClientEvent, Order, data::Bbo};

pub struct OkxBroker {
    terminal: Terminal,
}

impl OkxBroker {
    pub async fn new_bbo(instrument_id: InstId, history_duration: Duration) -> Self {
        let subscribe_actions = vec![
            Action::SubscribeBboTbt(instrument_id),
            Action::SubscribeOrders(instrument_id),
        ];
        let terminal = Terminal::new_okx(true, subscribe_actions, history_duration)
            .await
            .unwrap();
        Self { terminal }
    }
}

impl Broker<Bbo> for OkxBroker {
    async fn on_client_event(&mut self, client_event: ClientEvent) {
        let action = match client_event {
            ClientEvent::PlaceOrder(order) => match order {
                Order::Market(order) => {
                    let request_id = "".into();
                    let side = if order.side { Side::Buy } else { Side::Sell };
                    let inst_id = order.instrument_id;
                    let client_order_id = order.order_id.to_string().into();
                    let size = order.size.to_string().into();
                    Action::MarketOrder {
                        request_id,
                        side,
                        inst_id,
                        client_order_id,
                        size,
                    }
                }
                Order::Limit(order) => {
                    let request_id = "".into();
                    let side = if order.side { Side::Buy } else { Side::Sell };
                    let inst_id = order.instrument_id;
                    let client_order_id = order.order_id.to_string().into();
                    let size = order.size.to_string().into();
                    let price = order.price.to_string().into();
                    Action::LimitOrder {
                        request_id,
                        side,
                        inst_id,
                        client_order_id,
                        size,
                        price,
                    }
                }
            },
            ClientEvent::AmendOrder(amend) => {
                let request_id = "".into();
                let inst_id = amend.instrument_id;
                let client_order_id = amend.order_id.to_string().into();
                let new_size = amend.new_size.to_string().into();
                let new_price = amend.new_price.to_string().into();
                Action::AmendOrder {
                    request_id,
                    inst_id,
                    client_order_id,
                    new_size,
                    new_price,
                }
            }
            ClientEvent::CancelOrder(inst_id, order_id) => {
                let request_id = "".into();
                let client_order_id = order_id.to_string().into();
                Action::CancelOrder {
                    request_id,
                    inst_id,
                    client_order_id,
                }
            }
        };
        tracing::info!("Sending action: {action:?}");
        if let Err(e) = self.terminal.send(action).await {
            tracing::error!("Error sending action: {}", e);
        }
    }

    async fn next_broker_event(&mut self) -> Option<crate::BrokerEvent<Bbo>> {
        self.terminal
            .next()
            .await
            .and_then(|data| crate::BrokerEvent::try_from_data(data))
    }
}
