//! Schroeder allpass delay stage.

use crate::delay::DelayLine;
use crate::math::flush_denormal;

#[derive(Debug, Clone, Default)]
pub struct AllpassDelay {
    delay: DelayLine,
}

impl AllpassDelay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn prepare(&mut self, max_delay_samples: usize) {
        self.delay.prepare(max_delay_samples);
    }

    pub fn clear(&mut self) {
        self.delay.clear();
    }

    pub fn process(&mut self, input: f32, delay_samples: usize, gain: f32) -> f32 {
        let delayed = self.delay.read_int(delay_samples.max(1));
        let output = -gain * input + delayed;
        self.delay.push(input + gain * output);
        flush_denormal(output)
    }
}
