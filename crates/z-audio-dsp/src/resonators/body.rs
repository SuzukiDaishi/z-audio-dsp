//! Lightweight stereo body/soundboard resonator.

use super::{BiquadResonator, ModalMode};

const BODY_MODES: [ModalMode; 24] = [
    ModalMode {
        frequency_hz: 82.0,
        gain: 0.0060,
        decay_sec: 0.72,
    },
    ModalMode {
        frequency_hz: 103.0,
        gain: 0.0075,
        decay_sec: 0.74,
    },
    ModalMode {
        frequency_hz: 129.0,
        gain: 0.0090,
        decay_sec: 0.68,
    },
    ModalMode {
        frequency_hz: 163.0,
        gain: 0.0105,
        decay_sec: 0.64,
    },
    ModalMode {
        frequency_hz: 205.0,
        gain: 0.0115,
        decay_sec: 0.58,
    },
    ModalMode {
        frequency_hz: 258.0,
        gain: 0.0120,
        decay_sec: 0.54,
    },
    ModalMode {
        frequency_hz: 326.0,
        gain: 0.0120,
        decay_sec: 0.50,
    },
    ModalMode {
        frequency_hz: 412.0,
        gain: 0.0110,
        decay_sec: 0.46,
    },
    ModalMode {
        frequency_hz: 520.0,
        gain: 0.0100,
        decay_sec: 0.41,
    },
    ModalMode {
        frequency_hz: 655.0,
        gain: 0.0090,
        decay_sec: 0.36,
    },
    ModalMode {
        frequency_hz: 824.0,
        gain: 0.0080,
        decay_sec: 0.32,
    },
    ModalMode {
        frequency_hz: 1038.0,
        gain: 0.0070,
        decay_sec: 0.28,
    },
    ModalMode {
        frequency_hz: 1295.0,
        gain: 0.0060,
        decay_sec: 0.24,
    },
    ModalMode {
        frequency_hz: 1590.0,
        gain: 0.0053,
        decay_sec: 0.21,
    },
    ModalMode {
        frequency_hz: 1950.0,
        gain: 0.0047,
        decay_sec: 0.18,
    },
    ModalMode {
        frequency_hz: 2380.0,
        gain: 0.0042,
        decay_sec: 0.16,
    },
    ModalMode {
        frequency_hz: 2890.0,
        gain: 0.0036,
        decay_sec: 0.14,
    },
    ModalMode {
        frequency_hz: 3480.0,
        gain: 0.0031,
        decay_sec: 0.12,
    },
    ModalMode {
        frequency_hz: 4170.0,
        gain: 0.0027,
        decay_sec: 0.105,
    },
    ModalMode {
        frequency_hz: 4980.0,
        gain: 0.0023,
        decay_sec: 0.09,
    },
    ModalMode {
        frequency_hz: 5900.0,
        gain: 0.0019,
        decay_sec: 0.078,
    },
    ModalMode {
        frequency_hz: 6900.0,
        gain: 0.0016,
        decay_sec: 0.068,
    },
    ModalMode {
        frequency_hz: 8020.0,
        gain: 0.0013,
        decay_sec: 0.058,
    },
    ModalMode {
        frequency_hz: 9250.0,
        gain: 0.0011,
        decay_sec: 0.050,
    },
];

pub struct BodyResonator {
    sample_rate: f32,
    left: [BiquadResonator; BODY_MODES.len()],
    right: [BiquadResonator; BODY_MODES.len()],
    amount: f32,
    width: f32,
}

impl Default for BodyResonator {
    fn default() -> Self {
        let mut body = Self {
            sample_rate: 48_000.0,
            left: [BiquadResonator::default(); BODY_MODES.len()],
            right: [BiquadResonator::default(); BODY_MODES.len()],
            amount: 0.35,
            width: 0.75,
        };
        body.prepare(48_000.0);
        body
    }
}

impl BodyResonator {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.configure();
    }

    pub fn set_params(&mut self, amount: f32, width: f32) {
        self.amount = amount.clamp(0.0, 1.0);
        self.width = width.clamp(0.0, 1.0);
    }

    pub fn reset(&mut self) {
        for r in &mut self.left {
            r.reset();
        }
        for r in &mut self.right {
            r.reset();
        }
    }

    pub fn process(&mut self, input: f32) -> (f32, f32) {
        let mut l = 0.0;
        let mut r = 0.0;
        for resonator in &mut self.left {
            l += resonator.process(input) * self.amount;
        }
        for resonator in &mut self.right {
            r += resonator.process(input) * self.amount;
        }
        let mid = (l + r) * 0.5;
        let side = (l - r) * 0.5 * self.width;
        (mid + side, mid - side)
    }

    fn configure(&mut self) {
        for (i, mode) in BODY_MODES.iter().enumerate() {
            self.left[i].configure(
                self.sample_rate,
                mode.frequency_hz,
                mode.decay_sec,
                mode.gain,
            );
            self.right[i].configure(
                self.sample_rate,
                mode.frequency_hz * (1.0 + 0.007 * (i as f32 + 1.0)),
                mode.decay_sec * 0.93,
                mode.gain,
            );
        }
    }
}
