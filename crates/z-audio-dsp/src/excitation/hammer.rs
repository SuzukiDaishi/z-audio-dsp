//! Short hammer/click excitation for modal instruments.

use crate::math::{TAU, flush_denormal};

#[derive(Debug, Clone, Copy)]
pub struct HammerExciter {
    sample_rate: f32,
    time_samples: u32,
    duration_samples: u32,
    velocity: f32,
    hardness: f32,
    noise_amount: f32,
    rng: u32,
}

impl Default for HammerExciter {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            time_samples: 0,
            duration_samples: 0,
            velocity: 0.0,
            hardness: 0.5,
            noise_amount: 0.08,
            rng: 0x1234_abcd,
        }
    }
}

impl HammerExciter {
    pub fn new(seed: u32) -> Self {
        Self {
            rng: seed.max(1),
            ..Default::default()
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
    }

    pub fn trigger(&mut self, velocity: f32, frequency_hz: f32, hardness: f32, noise_amount: f32) {
        self.velocity = velocity.clamp(0.0, 1.0);
        self.hardness = hardness.clamp(0.0, 1.0);
        self.noise_amount = noise_amount.clamp(0.0, 1.0);
        let piano_span = (4186.0_f32 / 27.5_f32).log2();
        let note_norm = ((frequency_hz.max(1.0) / 27.5).log2() / piano_span).clamp(0.0, 1.0);
        let contact_ms = 0.22 + 1.80 * (1.0 - note_norm).powf(2.0);
        let duration_ms =
            contact_ms * (1.18 - 0.46 * self.hardness) * (1.08 - 0.28 * self.velocity);
        self.duration_samples = (duration_ms * 0.001 * self.sample_rate).round() as u32;
        self.duration_samples = self.duration_samples.clamp(8, 256);
        self.time_samples = 0;
    }

    pub fn is_done(&self) -> bool {
        self.time_samples >= self.duration_samples
    }

    pub fn next_sample(&mut self) -> f32 {
        if self.is_done() {
            return 0.0;
        }
        let denom = self.duration_samples.saturating_sub(1).max(1) as f32;
        let x = self.time_samples as f32 / denom;
        let force = (0.5 - 0.5 * (TAU * x).cos()).powf(0.65 + self.hardness * 0.7);
        let contact_noise = self.next_noise()
            * self.noise_amount
            * (1.0 - x).powf(2.0)
            * (0.2 + self.hardness * 0.8);
        let felt_snap = if x < 0.16 {
            (1.0 - x / 0.16).powf(2.0) * self.hardness * 0.22
        } else {
            0.0
        };
        self.time_samples += 1;
        flush_denormal(
            (force * (0.62 + self.hardness * 0.55) + contact_noise + felt_snap)
                * self.velocity.powf(0.72)
                * (0.35 + 0.95 * self.velocity),
        )
    }

    fn next_noise(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x.max(1);
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}
