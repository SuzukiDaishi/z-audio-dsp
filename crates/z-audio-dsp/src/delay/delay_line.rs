//! Realtime-safe circular delay line.

use crate::math::lerp;

/// Circular delay buffer. Allocation happens only in [`DelayLine::prepare`].
#[derive(Debug, Clone, Default)]
pub struct DelayLine {
    buffer: Vec<f32>,
    write_pos: usize,
}

impl DelayLine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates enough storage for `max_delay_samples`. A one-sample guard is
    /// added so `delay == len - 1` is always readable.
    pub fn prepare(&mut self, max_delay_samples: usize) {
        self.buffer.resize(max_delay_samples.max(1) + 1, 0.0);
        self.write_pos = 0;
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn push(&mut self, x: f32) {
        if self.buffer.is_empty() {
            return;
        }
        self.buffer[self.write_pos] = x;
        self.write_pos += 1;
        if self.write_pos >= self.buffer.len() {
            self.write_pos = 0;
        }
    }

    pub fn read_int(&self, delay_samples: usize) -> f32 {
        if self.buffer.is_empty() {
            return 0.0;
        }
        let len = self.buffer.len();
        let delay = delay_samples.min(len - 1);
        let idx = (self.write_pos + len - delay) % len;
        self.buffer[idx]
    }

    pub fn read_frac_lerp(&self, delay_samples: f32) -> f32 {
        if self.buffer.is_empty() {
            return 0.0;
        }
        let delay = delay_samples.clamp(0.0, (self.buffer.len() - 1) as f32);
        let d0 = delay.floor() as usize;
        let d1 = (d0 + 1).min(self.buffer.len() - 1);
        lerp(self.read_int(d0), self.read_int(d1), delay - d0 as f32)
    }

    /// Returns a delayed sample and then writes `input`.
    pub fn process_int(&mut self, input: f32, delay_samples: usize) -> f32 {
        if delay_samples == 0 {
            self.push(input);
            input
        } else {
            let out = self.read_int(delay_samples);
            self.push(input);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_delay_returns_expected_samples() {
        let mut delay = DelayLine::new();
        delay.prepare(4);
        assert_eq!(delay.process_int(1.0, 2), 0.0);
        assert_eq!(delay.process_int(2.0, 2), 0.0);
        assert_eq!(delay.process_int(3.0, 2), 1.0);
        assert_eq!(delay.process_int(4.0, 2), 2.0);
    }

    #[test]
    fn clear_resets_buffer() {
        let mut delay = DelayLine::new();
        delay.prepare(4);
        delay.push(1.0);
        delay.push(2.0);
        delay.clear();
        assert_eq!(delay.read_int(1), 0.0);
    }
}
