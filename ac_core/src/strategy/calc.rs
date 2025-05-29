
pub struct Ema {
    /// The smoothing time constant (tau).
    tau: f64,
    /// EMA of the value.
    mean: Option<f64>,
}

impl Ema {
    /// Create a new EMA+variance with given time constant tau.
    pub fn new(tau: f64) -> Self {
        assert!(tau > 0.0, "tau must be positive");
        Self { tau, mean: None }
    }

    /// Reset EMA and variance with an initial sample.
    pub fn reset(&mut self, init: f64) {
        self.mean = Some(init);
    }

    /// Update with a new sample at time interval dt.
    /// Returns a tuple (mean, variance).
    pub fn update(&mut self, sample: f64, dt: f64) -> f64 {
        assert!(dt >= 0.0, "dt must be non-negative");
        let alpha = 1.0 - (-dt / self.tau).exp();
        // Update mean
        let new_mean = match self.mean {
            Some(m) => m * (1.0 - alpha) + sample * alpha,
            None => sample,
        };
        self.mean = Some(new_mean);
        
        new_mean
    }

    /// Get current EMA mean.
    pub fn mean(&self) -> Option<f64> {
        self.mean
    }
}


/// Expoential moving average and variance
pub struct Emav {
    /// The smoothing time constant (tau).
    tau: f64,
    /// EMA of the value.
    mean: Option<f64>,
    /// EMA of the squared value.
    mean_sq: Option<f64>,
}

impl Emav {
    /// Create a new EMA+variance with given time constant tau.
    pub fn new(tau: f64) -> Self {
        assert!(tau > 0.0, "tau must be positive");
        Emav { tau, mean: None, mean_sq: None }
    }

    /// Reset EMA and variance with an initial sample.
    pub fn reset(&mut self, init: f64) {
        self.mean = Some(init);
        self.mean_sq = Some(init * init);
    }

    /// Update with a new sample at time interval dt.
    /// Returns a tuple (mean, variance).
    pub fn update(&mut self, sample: f64, dt: f64) -> (f64, f64) {
        assert!(dt >= 0.0, "dt must be non-negative");
        let alpha = 1.0 - (-dt / self.tau).exp();
        // Update mean
        let new_mean = match self.mean {
            Some(m) => m * (1.0 - alpha) + sample * alpha,
            None => sample,
        };
        self.mean = Some(new_mean);
        // Update mean of squares
        let new_mean_sq = match self.mean_sq {
            Some(msq) => msq * (1.0 - alpha) + sample * sample * alpha,
            None => sample * sample,
        };
        self.mean_sq = Some(new_mean_sq);
        // Compute variance = E[x²] - E[x]² (floored at zero)
        let var = (new_mean_sq - new_mean * new_mean).max(0.0);
        (new_mean, var)
    }

    /// Get current EMA mean.
    pub fn mean(&self) -> Option<f64> {
        self.mean
    }

    /// Get current EMA variance.
    pub fn variance(&self) -> Option<f64> {
        match (self.mean, self.mean_sq) {
            (Some(m), Some(msq)) => Some((msq - m * m).max(0.0)),
            _ => None,
        }
    }
}
