//! Attack/release smoothing for detector envelopes and gain reduction.

#[derive(Debug, Clone, Copy)]
pub struct BallisticsFilter {
    sample_rate: f32,
    attack_ms: f32,
    release_ms: f32,
    attack_coeff: f32,
    release_coeff: f32,
    state: f32,
}

impl Default for BallisticsFilter {
    fn default() -> Self {
        let mut filter = Self {
            sample_rate: 48_000.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            state: 0.0,
        };
        filter.configure(48_000.0, 10.0, 100.0);
        filter
    }
}

impl BallisticsFilter {
    pub fn configure(&mut self, sample_rate: f32, attack_ms: f32, release_ms: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.attack_ms = attack_ms.max(0.001);
        self.release_ms = release_ms.max(0.001);
        self.attack_coeff = coeff(self.sample_rate, self.attack_ms);
        self.release_coeff = coeff(self.sample_rate, self.release_ms);
    }

    pub fn reset(&mut self) {
        self.state = 0.0;
    }

    pub fn reset_to(&mut self, value: f32) {
        self.state = value.max(0.0);
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let input = input.max(0.0);
        let c = if input > self.state {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.state = c * self.state + (1.0 - c) * input;
        self.state
    }

    pub fn state(&self) -> f32 {
        self.state
    }
}

fn coeff(sample_rate: f32, time_ms: f32) -> f32 {
    (-1.0 / ((time_ms * 0.001).max(1.0e-6) * sample_rate.max(1.0))).exp()
}
