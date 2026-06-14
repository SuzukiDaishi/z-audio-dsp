//! Single-pole parameter smoothing to avoid audible "zipper noise" when
//! parameters change abruptly.

/// A one-pole smoothed parameter. Call [`SmoothedParam::set_target`] when the
/// user-facing value changes, then call [`SmoothedParam::tick`] once per
/// sample to advance `current` toward `target`.
#[derive(Debug, Clone, Copy)]
pub struct SmoothedParam {
    current: f32,
    target: f32,
    coeff: f32,
}

impl SmoothedParam {
    /// Creates a new smoothed parameter, initialized to `initial` with no
    /// smoothing configured yet. Call [`SmoothedParam::configure`] before use
    /// to set the smoothing time constant for a given sample rate.
    pub fn new(initial: f32) -> Self {
        Self {
            current: initial,
            target: initial,
            coeff: 0.0,
        }
    }

    /// Configures the one-pole smoothing coefficient so that `current` moves
    /// roughly `1 - 1/e` (~63%) of the remaining distance to `target` within
    /// `time_constant_seconds`.
    pub fn configure(&mut self, sample_rate: f32, time_constant_seconds: f32) {
        debug_assert!(sample_rate > 0.0);
        if time_constant_seconds <= 0.0 {
            self.coeff = 1.0;
        } else {
            self.coeff = 1.0 - (-1.0 / (time_constant_seconds * sample_rate)).exp();
        }
    }

    /// Sets a new target value. `current` moves toward this on subsequent
    /// [`SmoothedParam::tick`] calls.
    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    /// Immediately sets both `current` and `target`, bypassing smoothing.
    pub fn set_immediate(&mut self, value: f32) {
        self.current = value;
        self.target = value;
    }

    /// Advances `current` one sample toward `target` and returns the new
    /// value.
    pub fn tick(&mut self) -> f32 {
        self.current += (self.target - self.current) * self.coeff;
        self.current
    }

    /// Returns the current (smoothed) value without advancing it.
    pub fn current(&self) -> f32 {
        self.current
    }

    /// Returns the target value.
    pub fn target(&self) -> f32 {
        self.target
    }

    /// Returns `true` once `current` is within `epsilon` of `target`.
    pub fn is_settled(&self, epsilon: f32) -> bool {
        (self.target - self.current).abs() <= epsilon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothed_param_converges_monotonically() {
        let mut p = SmoothedParam::new(0.0);
        p.configure(48_000.0, 0.005);
        p.set_target(1.0);

        let mut last = p.current();
        for _ in 0..2000 {
            let value = p.tick();
            assert!(value >= last);
            last = value;
        }
        assert!(p.is_settled(1e-3));
    }

    #[test]
    fn immediate_value_has_no_smoothing() {
        let mut p = SmoothedParam::new(0.0);
        p.configure(48_000.0, 0.005);
        p.set_immediate(2.0);
        assert_eq!(p.current(), 2.0);
        assert_eq!(p.tick(), 2.0);
    }

    #[test]
    fn zero_time_constant_jumps_immediately() {
        let mut p = SmoothedParam::new(0.0);
        p.configure(48_000.0, 0.0);
        p.set_target(5.0);
        assert_eq!(p.tick(), 5.0);
    }
}
