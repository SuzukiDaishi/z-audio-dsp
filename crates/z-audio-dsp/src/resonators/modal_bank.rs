//! Fixed-capacity modal resonator bank.

use super::BiquadResonator;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModalMode {
    pub frequency_hz: f32,
    pub gain: f32,
    pub decay_sec: f32,
}

impl Default for ModalMode {
    fn default() -> Self {
        Self {
            frequency_hz: 440.0,
            gain: 0.0,
            decay_sec: 1.0,
        }
    }
}

pub struct ModalBank<const N: usize> {
    sample_rate: f32,
    resonators: [BiquadResonator; N],
    active: usize,
}

impl<const N: usize> Default for ModalBank<N> {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            resonators: [BiquadResonator::default(); N],
            active: 0,
        }
    }
}

impl<const N: usize> ModalBank<N> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        for resonator in &mut self.resonators {
            resonator.configure(self.sample_rate, 440.0, 1.0, 0.0);
        }
    }

    pub fn set_modes(&mut self, modes: &[ModalMode]) {
        self.active = modes.len().min(N);
        for (resonator, mode) in self.resonators.iter_mut().zip(modes.iter()) {
            resonator.configure(
                self.sample_rate,
                mode.frequency_hz,
                mode.decay_sec,
                mode.gain,
            );
            resonator.reset();
        }
        for resonator in self.resonators.iter_mut().skip(self.active) {
            resonator.configure(self.sample_rate, 440.0, 1.0, 0.0);
            resonator.reset();
        }
    }

    pub fn reset(&mut self) {
        for resonator in &mut self.resonators {
            resonator.reset();
        }
    }

    pub fn limit_decays(&mut self, max_decay_sec: &[f32]) {
        for (resonator, max_decay) in self
            .resonators
            .iter_mut()
            .take(self.active)
            .zip(max_decay_sec.iter().copied())
        {
            resonator.limit_decay_sec(max_decay);
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let mut out = 0.0;
        for resonator in self.resonators.iter_mut().take(self.active) {
            out += resonator.process(input);
        }
        out
    }

    pub fn energy(&self) -> f32 {
        self.resonators
            .iter()
            .take(self.active)
            .map(BiquadResonator::energy)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_excites_modal_bank() {
        let mut bank: ModalBank<4> = ModalBank::new();
        bank.prepare(48_000.0);
        bank.set_modes(&[ModalMode {
            frequency_hz: 440.0,
            gain: 0.5,
            decay_sec: 0.5,
        }]);
        let first = bank.process(1.0);
        let second = bank.process(0.0);
        assert!(first.abs() > 0.0);
        assert!(second.is_finite());
    }
}
