
use chrono::Duration;

use crate::{
    data::Bbo, strategy::{
        calc::{Ema, Emav}, executors::NaiveLimitExecutor, Signal, SignalExecuteStrategy, Signaler, Strategy
    }, InstId, Timestamp
};

/// 订单流失衡。
///
/// 入场条件：
/// - 多头：$z_t >  \sigma$
/// - 空头：$z_t < -\sigma$
#[derive(Default)]
pub struct OfiMomentum {
    /// 策略的窗口长度
    window_ofi: u64,
    window_ema: u64,
    /// 入场的标准化OFI阈值
    theta: f64,

    /// 策略预热期的长度
    warm_up_duration: u64,
    first_ts: Option<Timestamp>,

    variables: Option<Variables>,
}

struct Variables {
    bbo: Bbo,
    ofi_segment: f64,
    /// EMA of ofi
    ofi: Ema,
    /// EMA of the EMA of ofi
    eam_ofi: Emav,
}

impl Variables {
    fn new(bbo: Bbo, window_ema_ofi: u64, window_ema_ema_ofi: u64) -> Self {
        Self {
            bbo,
            ofi_segment: f64::NAN,
            ofi: Ema::new(window_ema_ofi as f64),
            eam_ofi: Emav::new(window_ema_ema_ofi as f64),
        }
    }

    fn update(&mut self, bbo: &Bbo) {
        let mut ofi_segment = 0.;
        let old_bbo = &self.bbo;
        if bbo.bid_price >= old_bbo.bid_price {
            ofi_segment += bbo.bid_size
        }
        if bbo.bid_price <= old_bbo.bid_price {
            ofi_segment -= old_bbo.bid_size
        }
        if bbo.ask_price <= old_bbo.ask_price {
            ofi_segment -= bbo.ask_size
        }
        if bbo.ask_price >= old_bbo.ask_price {
            ofi_segment += old_bbo.ask_size
        }
        self.ofi_segment = ofi_segment;

        let dt = bbo.ts - old_bbo.ts;
        self.ofi.update(ofi_segment, dt as f64);
        let ofi = self.ofi.mean().unwrap();
        self.eam_ofi.update(ofi, dt as f64);
    }

    /// 计算ema_ofi的z-score
    fn get_signal(&self, theta: f64) -> Option<Signal> {
        let ofi = self.ofi.mean()?;
        dbg!(ofi);
        let mean_ofi = self.eam_ofi.mean()?;
        dbg!(mean_ofi);
        let var_ofi = self.eam_ofi.variance()?;
        dbg!(var_ofi);

        let z_score = (ofi - mean_ofi) / var_ofi.sqrt();
        dbg!(z_score);
        if z_score > theta {
            Some(Signal::Long)
        } else if z_score < -theta {
            Some(Signal::Short)
        } else {
            None
        }
    }
}

impl OfiMomentum {
    pub fn new(window_ofi: Duration, window_ema: Duration, theta: f64) -> Self {
        let window_ofi = window_ofi.num_milliseconds() as u64;
        let window_ema = window_ema.num_milliseconds() as u64;
        Self {
            window_ofi,
            window_ema,
            theta,
            warm_up_duration: 3 * window_ofi.max(window_ema),
            ..Default::default()
        }
    }
}

impl Signaler<Bbo> for OfiMomentum {
    fn on_data(&mut self, bbo: &Bbo) -> Option<Signal> {
        // Initialize first timestamp
        if self.first_ts.is_none() {
            self.first_ts = Some(bbo.ts);
        }

        // Initialize variables on first data
        if self.variables.is_none() {
            self.variables = Some(Variables::new(
                *bbo,
                self.window_ofi,
                self.window_ema,
            ));
            return None;
        }

        // Update variables with new data
        let variables = self.variables.as_mut().unwrap();
        variables.update(bbo);

        // Check if warm-up period is complete
        let elapsed = bbo.ts - self.first_ts.unwrap();
        if elapsed > self.warm_up_duration {
            variables.get_signal(self.theta)
        } else {
            None
        }
    }
}

pub struct OfiMomentumArgs {
    pub instrument_id: InstId,
    pub window_ofi: Duration,
    pub window_ema: Duration,
    pub theta: f64,
    /// 信号消失后的持仓时间
    pub holding_duration: Duration,
    
    pub notional: f64,
    pub size_digits: u32,
    /// 策略实例的全局唯一标识符，小于2^16
    pub order_id_offset: u64
}

impl OfiMomentumArgs {
    pub fn into_strategy(self) -> impl Strategy<Bbo> {
        let ofi_momentum_signaler = OfiMomentum::new(self.window_ofi, self.window_ema, self.theta);
        let executor = NaiveLimitExecutor::new(self.instrument_id, self.notional, self.size_digits, self.holding_duration, self.order_id_offset);
        SignalExecuteStrategy::new(ofi_momentum_signaler, executor)
    }
}