use chrono::Duration;
use float_cmp::approx_eq;

use crate::{
    data::Bbo, utils::{round_f64, truncate_f64}, BrokerEvent, ClientEvent, InstId, LimitOrder, Order, Position, Timestamp
};

use super::{Executor, Signal};

// 生成订单的逻辑：先计算期望的持仓，再与当前的持仓相减，得到所需的订单。与当前的挂单进行对比，判断维持/改单/取消

/// A naive limit order executor based on bbo. 根据信号尝试建仓。若为多头信号，则在 最优买价 + price_offset 挂限价单。若在给定时间内未成交，则取消订单。
/// 若在成交前信号转为空头，则取消并反向挂空单。若距离最后一次信号的时长到达给定值，挂单平仓。
#[derive(Default)]
pub struct NaiveLimitExecutor {
    instrument_id: InstId,
    notional: f64,
    /// The digits of the size
    size_digits: i32,
    size_eps: f64,
    price_digits: i32,
    /// 下单的名义金额门槛
    notional_threshold: f64,
    /// 挂单价格朝激进方向的偏移量
    price_offset: f64,

    bbo: Bbo,

    last_signal: Option<Signal>,
    /// 最后一个非None的Signal抵达的ts
    last_signal_ts: Timestamp,
    /// 信号消失后继续持有的时长
    holding_duration: Timestamp,

    last_event_ts: Timestamp,
    /// 发出事件的最小时间间隔，避免频繁发出事件
    event_interval: Timestamp,

    position: Position,
    placed_order: Option<LimitOrder>,

    next_order_id_body: u64,
    /// 小于2^16，用于作为每个策略的Order id的末位唯一标识符
    order_id_offset: u64,
}

impl NaiveLimitExecutor {
    pub fn new(
        instrument_id: InstId,
        notional: f64,
        size_digits: i32,
        price_digits: i32,
        price_offset: f64,
        holding_duration: Duration,
        event_interval: Duration,
        order_id_offset: u64,
    ) -> Self {
        Self {
            instrument_id,
            notional,
            size_digits,
            size_eps: 10f64.powi(-(size_digits as i32)),
            notional_threshold: 0.05 * notional,
            price_offset,
            price_digits,
            holding_duration: holding_duration.num_milliseconds() as u64,
            event_interval: event_interval.num_milliseconds() as u64,
            order_id_offset,
            ..Default::default()
        }
    }

    fn get_ideal_position(&self, signal: Option<Signal>) -> Position {
        let Some(signal) = signal else {
            if self.position.is_clear(self.size_digits) {
                // 无信号且无仓位，维持空仓
                return self.position;
            } else {
                // 无信号但有仓位，检测持仓是否超过时限，若是则平仓，若不是则维持仓位
                if self.bbo.ts - self.last_signal_ts >= self.holding_duration {
                    return Position::new(0.);
                } else {
                    return self.position;
                }
            }
        };

        match signal {
            Signal::Long => {
                let size = self.notional / self.bbo.bid_price;
                let size = truncate_f64(size, self.size_digits);
                Position::new(size)
            }
            Signal::Short => {
                let size = -self.notional / self.bbo.ask_price;
                let size = truncate_f64(size, self.size_digits);
                Position::new(size)
            }
        }
    }

    fn get_next_order_id(&mut self) -> u64 {
        let order_id_body = self.next_order_id_body;
        self.next_order_id_body += 1;

        (order_id_body << 16) | self.order_id_offset
    }

    fn gen_order(&mut self, raw_size: f64, price: f64) -> Option<LimitOrder> {
        if approx_eq!(f64, raw_size, 0., epsilon = self.size_eps) {
            return None;
        }
        if raw_size.abs() * price < self.notional_threshold {
            return None
        }
        let order = LimitOrder::from_raw_size(
            raw_size,
            self.get_next_order_id(),
            self.instrument_id,
            price,
        );
        Some(order)
    }

    // 将应有的挂单规模与实际挂单规模对比，并按需发出事件
    fn get_event_from_target_order(&mut self, raw_size: f64, price: f64) -> Vec<ClientEvent> {
        // 若不存在挂单，则直接下单
        let Some(ref mut old_order) = self.placed_order else {
            let order = self.gen_order(raw_size, price);
            self.placed_order = order;
            let event = order.map(ClientEvent::place_limit_order);
            return event.into_iter().collect();
        };

        // 若目标订单的size为0，则取消目前挂单
        if approx_eq!(f64, 0., raw_size, epsilon = self.size_eps) {
            let old_order_id = old_order.order_id;
            self.placed_order = None;
            return vec![ClientEvent::CancelOrder(old_order_id)];
        }

        let (new_side, new_size) = crate::utils::get_side_size_from_raw_size(raw_size);
        if new_side == old_order.side {
            // 方向匹配，订单规模或价格不匹配，则进行改单
            if !approx_eq!(
                f64,
                old_order.unfilled_size(),
                new_size,
                epsilon = self.size_eps
            ) || old_order.price != price
            {
                let modified_order = old_order.modified(new_size, price);
                return vec![ClientEvent::ModifyOrder(Order::Limit(modified_order))];
            }

            // 两个思路：1：在收到信号后，在信号改变前，维持最初的挂单，不改单；
            // 2：锚定最激进的限价单，持续改单。
            // 上面被注释掉的代码是第2种思路。目前先试试第一种思路。也许可以写进参数。
            vec![]
        } else {
            // 方向不匹配，则取消订单并重新下单
            let mut events = vec![];
            let old_order_id = old_order.order_id;
            events.push(ClientEvent::CancelOrder(old_order_id));
            let new_order = self.gen_order(raw_size, price);
            self.placed_order = new_order;
            events.extend(new_order.map(ClientEvent::place_limit_order));
            events
        }
    }

    fn calc_target_order_arg(&self, target_position: Position) -> (f64, f64) {
        let target_order_size = target_position.size - self.position.size;
        let price = if target_order_size > 0. {
            self.bbo.bid_price + self.price_offset
        } else {
            self.bbo.ask_price - self.price_offset
        };
        let price = round_f64(price, self.price_digits);
        (target_order_size, price)
    }
}

impl Executor<Bbo> for NaiveLimitExecutor {
    fn update(&mut self, broker_event: &BrokerEvent<Bbo>) {
        match broker_event {
            BrokerEvent::Data(bbo) => self.bbo = *bbo,
            BrokerEvent::Fill(fill) => {
                self.placed_order = self.placed_order.and_then(|order| order.fill(fill));
                self.position.update(fill);
            }
        }
    }

    fn on_signal(&mut self, signal: Option<Signal>) -> Vec<ClientEvent> {
        // // 信号未改变时，早停
        // if signal.is_some() && self.last_signal == signal {
        //     return vec![];
        // }

        if self.bbo.ts - self.last_event_ts < self.event_interval {
            return vec![]
        }

        // 根据信号，获取目标仓位
        let ideal_position: Position = self.get_ideal_position(signal);
        // 根据目标仓位，获取目标挂单
        let (ideal_order_size, price) = self.calc_target_order_arg(ideal_position);
        // 根据目标挂单，获取操作
        let events = self.get_event_from_target_order(ideal_order_size, price);
        
        // 更新signal相关状态
        self.last_signal = signal;
        if signal.is_some() {
            self.last_signal_ts = self.bbo.ts
        }

        if !events.is_empty() {
            self.last_event_ts = self.bbo.ts
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BrokerEvent, ClientEvent, ExecType, Fill, FillState, Order};

    fn create_test_executor() -> NaiveLimitExecutor {
        NaiveLimitExecutor::new(
            "TEST_INST".try_into().unwrap(),
            1000.0, // notional
            2,      // size_digits
            2,
            0.,
            Duration::milliseconds(10000),  // holding_duration in ms
            Duration::seconds(0),
            123,    // order_id_offset
        )
    }

    fn create_test_bbo(ts: u64, bid_price: f64, ask_price: f64) -> Bbo {
        Bbo {
            ts,
            instrument_id: "TEST_INST".try_into().unwrap(),
            bid_price,
            bid_size: 10.0,
            ask_price,
            ask_size: 10.0,
        }
    }

    #[test]
    fn test_new_executor() {
        let executor = create_test_executor();
        assert_eq!(executor.instrument_id.as_str(), "TEST_INST");
        assert_eq!(executor.notional, 1000.0);
        assert_eq!(executor.size_digits, 2);
        assert_eq!(executor.size_eps, 0.01);
        assert_eq!(executor.holding_duration, 10000);
        assert_eq!(executor.order_id_offset, 123);
    }

    #[test]
    fn test_long_signal() {
        let mut executor = create_test_executor();

        // Update with a BBO
        let bbo = create_test_bbo(1000, 100.0, 101.0);
        executor.update(&BrokerEvent::Data(bbo));

        // Process a long signal
        let events = executor.on_signal(Some(Signal::Long));

        assert_eq!(events.len(), 1);
        if let ClientEvent::PlaceOrder(Order::Limit(order)) = &events[0] {
            assert_eq!(order.instrument_id.as_str(), "TEST_INST");
            assert!(order.side); // Buy side
            assert_eq!(order.price, 100.0); // Should use bid price
            assert_eq!(order.size, 10.0); // 1000 / 100 = 10
        } else {
            panic!("Expected PlaceOrder event with limit order");
        }
    }

    #[test]
    fn test_short_signal() {
        let mut executor = create_test_executor();

        // Update with a BBO
        let bbo = create_test_bbo(1000, 100.0, 101.0);
        executor.update(&BrokerEvent::Data(bbo));

        // Process a short signal
        let events = executor.on_signal(Some(Signal::Short));

        assert_eq!(events.len(), 1);
        if let ClientEvent::PlaceOrder(Order::Limit(order)) = &events[0] {
            assert_eq!(order.instrument_id.as_str(), "TEST_INST");
            assert!(!order.side); // Sell side
            assert_eq!(order.price, 101.0); // Should use ask price
            assert_eq!(order.size, 9.90); // 1000 / 101 = 9.90 (truncated to 2 decimals)
        } else {
            panic!("Expected PlaceOrder event with limit order");
        }
    }

    #[test]
    fn test_signal_change() {
        let mut executor = create_test_executor();

        // Update with a BBO
        let bbo = create_test_bbo(1000, 100.0, 101.0);
        executor.update(&BrokerEvent::Data(bbo));

        // Process a long signal
        let events = executor.on_signal(Some(Signal::Long));
        assert_eq!(events.len(), 1);

        let order_id = match &events[0] {
            ClientEvent::PlaceOrder(Order::Limit(order)) => order.order_id,
            _ => panic!("Expected PlaceOrder event"),
        };

        // Switch to short signal
        let events = executor.on_signal(Some(Signal::Short));
        assert_eq!(events.len(), 2); // Cancel + place new order

        // Check cancel order
        assert!(matches!(events[0], ClientEvent::CancelOrder(id) if id == order_id));

        // Check new order
        if let ClientEvent::PlaceOrder(Order::Limit(order)) = &events[1] {
            assert!(!order.side); // Sell side
            assert_eq!(order.price, 101.0); // Ask price
        } else {
            panic!("Expected PlaceOrder event with limit order");
        }
    }

    #[test]
    fn test_fill_handling() {
        let mut executor = create_test_executor();

        // Update with a BBO
        let bbo = create_test_bbo(1000, 100.0, 101.0);
        executor.update(&BrokerEvent::Data(bbo));

        // Process a long signal
        let events = executor.on_signal(Some(Signal::Long));
        assert_eq!(events.len(), 1);

        let order_id = match &events[0] {
            ClientEvent::PlaceOrder(Order::Limit(order)) => order.order_id,
            _ => panic!("Expected PlaceOrder event"),
        };

        // Simulate a fill
        let fill = Fill {
            order_id,
            instrument_id: "TEST_INST".try_into().unwrap(),
            filled_size: 10.0,
            acc_filled_size: 10.0,
            price: 100.0,
            side: true,
            exec_type: ExecType::Maker,
            state: FillState::Filled,
        };
        executor.update(&BrokerEvent::Fill(fill));

        // Verify that placed_order is now None after full fill
        assert!(executor.placed_order.is_none());

        // Now with a short signal, should place a sell order for full position size
        let events = executor.on_signal(Some(Signal::Short));
        assert_eq!(events.len(), 1);

        if let ClientEvent::PlaceOrder(Order::Limit(order)) = &events[0] {
            assert!(!order.side); // Sell side
            assert_eq!(order.size, 19.90); // Position (10.0) + Short signal (9.90)
        } else {
            panic!("Expected PlaceOrder event with limit order");
        }
    }

    #[test]
    fn test_partial_fill() {
        let mut executor = create_test_executor();

        // Update with a BBO
        let bbo = create_test_bbo(1000, 100.0, 101.0);
        executor.update(&BrokerEvent::Data(bbo));

        // Process a long signal
        let events = executor.on_signal(Some(Signal::Long));
        assert_eq!(events.len(), 1);

        let order_id = match &events[0] {
            ClientEvent::PlaceOrder(Order::Limit(order)) => order.order_id,
            _ => panic!("Expected PlaceOrder event"),
        };

        // Simulate a partial fill
        let fill = Fill {
            order_id,
            instrument_id: "TEST_INST".try_into().unwrap(),
            filled_size: 5.0,
            acc_filled_size: 5.0,
            price: 100.0,
            side: true,
            exec_type: ExecType::Maker,
            state: FillState::Partially,
        };
        executor.update(&BrokerEvent::Fill(fill));

        // Should still have a placed order with updated filled_size
        assert!(executor.placed_order.is_some());
        assert_eq!(executor.placed_order.as_ref().unwrap().filled_size, 5.0);

        // Price change should trigger order modification
        let bbo = create_test_bbo(2000, 102.0, 103.0);
        executor.update(&BrokerEvent::Data(bbo));

        let events = executor.on_signal(Some(Signal::Long));
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], ClientEvent::ModifyOrder(_)));
    }

    #[test]
    fn test_position_timeout() {
        let mut executor = create_test_executor();

        // Update with a BBO
        let bbo = create_test_bbo(1000, 100.0, 101.0);
        executor.update(&BrokerEvent::Data(bbo));

        // Process a long signal
        let events = executor.on_signal(Some(Signal::Long));
        let order_id = match &events[0] {
            ClientEvent::PlaceOrder(Order::Limit(order)) => order.order_id,
            _ => panic!("Expected PlaceOrder event"),
        };

        // Simulate a fill
        let fill = Fill {
            order_id,
            instrument_id: "TEST_INST".try_into().unwrap(),
            filled_size: 10.0,
            acc_filled_size: 10.0,
            price: 100.0,
            side: true,
            exec_type: ExecType::Maker,
            state: FillState::Filled,
        };
        executor.update(&BrokerEvent::Fill(fill));

        // Update with a BBO with timestamp still within holding period
        let bbo = create_test_bbo(5000, 102.0, 103.0);
        executor.update(&BrokerEvent::Data(bbo));

        // No signal but within holding period, should not close position
        let events = executor.on_signal(None);
        assert_eq!(events.len(), 0);

        // Update with a BBO with timestamp beyond holding period
        let bbo = create_test_bbo(12000, 102.0, 103.0); // 1000 + 10000 + 1000
        executor.update(&BrokerEvent::Data(bbo));

        // No signal and beyond holding period, should close position
        let events = executor.on_signal(None);
        assert_eq!(events.len(), 1);

        if let ClientEvent::PlaceOrder(Order::Limit(order)) = &events[0] {
            assert!(!order.side); // Sell side to close position
            assert_eq!(order.size, 10.0); // Close entire position
            assert_eq!(order.price, 103.0); // Use ask price for selling
        } else {
            panic!("Expected PlaceOrder event with limit order");
        }
    }

    #[test]
    fn test_complex_scenario() {
        let mut executor = create_test_executor();

        // 1. 初始状态，更新BBO
        let bbo = create_test_bbo(1000, 100.0, 101.0);
        executor.update(&BrokerEvent::Data(bbo));

        // 2. 接收多头信号，产生买单
        let events = executor.on_signal(Some(Signal::Long));
        assert_eq!(events.len(), 1);

        let buy_order_id = match &events[0] {
            ClientEvent::PlaceOrder(Order::Limit(order)) => {
                assert!(order.side); // 确认是买单
                assert_eq!(order.price, 100.0); // 买价
                assert_eq!(order.size, 10.0); // 规模：1000/100=10
                order.order_id
            }
            _ => panic!("Expected PlaceOrder event"),
        };

        // 3. 部分成交
        let fill1 = Fill {
            order_id: buy_order_id,
            instrument_id: "TEST_INST".try_into().unwrap(),
            filled_size: 4.0,
            acc_filled_size: 4.0,
            price: 100.0,
            side: true,
            exec_type: ExecType::Maker,
            state: FillState::Partially,
        };
        executor.update(&BrokerEvent::Fill(fill1));

        // 确认订单仍然存在并且填充了部分
        assert!(executor.placed_order.is_some());
        assert_eq!(executor.placed_order.as_ref().unwrap().filled_size, 4.0);
        assert_eq!(executor.position.size(), 4.0);

        // 4. 价格变动
        let bbo = create_test_bbo(2000, 99.0, 100.0);
        executor.update(&BrokerEvent::Data(bbo));

        // 5. 信号变为空头
        let events = executor.on_signal(Some(Signal::Short));
        assert_eq!(events.len(), 2); // 取消旧单 + 下新单

        // 确认取消旧订单
        assert!(matches!(events[0], ClientEvent::CancelOrder(id) if id == buy_order_id));

        // 获取新卖单ID
        let sell_order_id = match &events[1] {
            ClientEvent::PlaceOrder(Order::Limit(order)) => {
                assert!(!order.side); // 确认是卖单
                assert_eq!(order.price, 100.0); // 卖价
                // 规模：原有持仓(4.0) + 新空头规模(1000/100=10.0) = 14.0
                assert_eq!(order.size, 14.0);
                order.order_id
            }
            _ => panic!("Expected PlaceOrder event"),
        };

        // 6. 部分成交卖单
        let fill2 = Fill {
            order_id: sell_order_id,
            instrument_id: "TEST_INST".try_into().unwrap(),
            filled_size: 8.0,
            acc_filled_size: 8.0,
            price: 100.0,
            side: false,
            exec_type: ExecType::Maker,
            state: FillState::Partially,
        };
        executor.update(&BrokerEvent::Fill(fill2));

        // 确认持仓变化：4.0 - 8.0 = -4.0
        assert_eq!(executor.position.size(), -4.0);

        // 7. 价格再次变动
        let bbo = create_test_bbo(3000, 98.0, 99.0);
        executor.update(&BrokerEvent::Data(bbo));

        // 8. 无信号输入，但持仓时间未超出
        let events = executor.on_signal(None);
        assert_eq!(events.len(), 1); // 维持现有持仓，但取消挂单
        match events[0] {
            ClientEvent::CancelOrder(order_id) => {
                assert_eq!(order_id, 1 << 16 | 123)
            }
            _ => panic!("Expected CancelOrder event"),
        }

        // 9. 价格再次变动，时间推移至超出持有期
        let bbo = create_test_bbo(15000, 97.0, 98.0); // 3000 + 10000 + 2000
        executor.update(&BrokerEvent::Data(bbo));

        // 10. 无信号输入，但持仓时间已超出
        let events = executor.on_signal(None);
        assert_eq!(events.len(), 1); // 应产生平仓事件

        let close_order_id = match &events[0] {
            ClientEvent::PlaceOrder(Order::Limit(order)) => {
                assert!(order.side); // 买单平空仓
                assert_eq!(order.price, 97.0); // 买价
                assert_eq!(order.size, 4.0); // 平掉全部-4.0持仓
                order.order_id
            }
            _ => panic!("Expected PlaceOrder event"),
        };

        // 11. 平仓完全成交
        let fill3 = Fill {
            order_id: close_order_id,
            instrument_id: "TEST_INST".try_into().unwrap(),
            filled_size: 4.0,
            acc_filled_size: 4.0,
            price: 97.0,
            side: true,
            exec_type: ExecType::Maker,
            state: FillState::Filled,
        };
        executor.update(&BrokerEvent::Fill(fill3));

        // 12. 价格再次变动
        let bbo = create_test_bbo(16000, 96.0, 97.0);
        executor.update(&BrokerEvent::Data(bbo));

        // 13. 确认持仓已清空
        assert_eq!(executor.position.size(), 0.0);

        // 14. 多头信号，再次下单
        let events = executor.on_signal(Some(Signal::Long));
        assert_eq!(events.len(), 1);

        if let ClientEvent::PlaceOrder(Order::Limit(order)) = &events[0] {
            assert!(order.side);
            assert_eq!(order.price, 96.0);
            assert_eq!(order.size, 10.41); // 1000/96=10.41666..., 保留2位小数
        } else {
            panic!("Expected PlaceOrder event with limit order");
        }
    }
}
