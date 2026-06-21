//! Two-state resonator suitable for modal synthesis.

use crate::math::{TAU, flush_denormal};

#[derive(Debug, Clone, Copy)]
pub struct BiquadResonator {
    sample_rate: f32,
    frequency_hz: f32,
    decay_sec: f32,
    gain: f32,
    rot_re: f32,
    rot_im: f32,
    decay: f32,
    re: f32,
    im: f32,
}

impl Default for BiquadResonator {
    fn default() -> Self {
        let mut resonator = Self {
            sample_rate: 48_000.0,
            frequency_hz: 440.0,
            decay_sec: 1.0,
            gain: 0.0,
            rot_re: 1.0,
            rot_im: 0.0,
            decay: 0.0,
            re: 0.0,
            im: 0.0,
        };
        resonator.configure(48_000.0, 440.0, 1.0, 0.0);
        resonator
    }
}

impl BiquadResonator {
    pub fn configure(&mut self, sample_rate: f32, frequency_hz: f32, decay_sec: f32, gain: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.frequency_hz = frequency_hz.clamp(1.0, self.sample_rate * 0.49);
        self.gain = gain;
        self.set_decay_sec(decay_sec);
        let theta = TAU * self.frequency_hz / self.sample_rate;
        self.rot_re = theta.cos();
        self.rot_im = theta.sin();
    }

    pub fn set_decay_sec(&mut self, decay_sec: f32) {
        self.decay_sec = decay_sec.max(0.005);
        self.decay = (-1.0 / (self.decay_sec * self.sample_rate)).exp();
    }

    pub fn limit_decay_sec(&mut self, max_decay_sec: f32) {
        if max_decay_sec.is_finite() && max_decay_sec > 0.0 && max_decay_sec < self.decay_sec {
            self.set_decay_sec(max_decay_sec);
        }
    }

    pub fn reset(&mut self) {
        self.re = 0.0;
        self.im = 0.0;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.re += input * self.gain;
        let out = self.re;
        let re = self.re * self.rot_re - self.im * self.rot_im;
        let im = self.re * self.rot_im + self.im * self.rot_re;
        self.re = flush_denormal(re * self.decay);
        self.im = flush_denormal(im * self.decay);
        flush_denormal(out)
    }

    pub fn energy(&self) -> f32 {
        self.re.abs() + self.im.abs()
    }
}
