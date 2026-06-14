//! Naive (non-band-limited) analog-style waveform generator.

use crate::Generator;
use crate::context::ProcessContext;
use crate::math::TAU;

/// The waveform shape produced by an [`AnalogGenerator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalogWaveform {
    Sine,
    Triangle,
    Saw,
    Pulse,
}

/// A simple phase-accumulator oscillator producing one of the
/// [`AnalogWaveform`] shapes. Output is naive (no anti-aliasing); band-limited
/// variants (PolyBLEP, wavetables) are planned for later phases.
pub struct AnalogGenerator {
    sample_rate: f32,
    phase: f32,
    phase_offset: f32,
    frequency_hz: f32,
    waveform: AnalogWaveform,
    pulse_width: f32,
}

impl AnalogGenerator {
    /// Creates a new generator. `phase_offset` is the phase (normalized into
    /// `[0.0, 1.0)`) that [`AnalogGenerator::reset`] restores. `pulse_width`
    /// is clamped to `[0.05, 0.95]` and only used by [`AnalogWaveform::Pulse`].
    pub fn new(waveform: AnalogWaveform, phase_offset: f32, pulse_width: f32) -> Self {
        let phase_offset = phase_offset.rem_euclid(1.0);
        Self {
            sample_rate: 48_000.0,
            phase: phase_offset,
            phase_offset,
            frequency_hz: 0.0,
            waveform,
            pulse_width: pulse_width.clamp(0.05, 0.95),
        }
    }

    /// Sets the oscillator frequency in Hz.
    pub fn set_frequency_hz(&mut self, frequency_hz: f32) {
        self.frequency_hz = frequency_hz;
    }

    /// Sets the pulse width, clamped to `[0.05, 0.95]`.
    pub fn set_pulse_width(&mut self, pulse_width: f32) {
        self.pulse_width = pulse_width.clamp(0.05, 0.95);
    }

    /// Computes the next sample and advances the oscillator phase.
    pub fn next_sample(&mut self) -> f32 {
        let value = match self.waveform {
            AnalogWaveform::Sine => (self.phase * TAU).sin(),
            AnalogWaveform::Saw => 2.0 * self.phase - 1.0,
            AnalogWaveform::Pulse => {
                if self.phase < self.pulse_width {
                    1.0
                } else {
                    -1.0
                }
            }
            AnalogWaveform::Triangle => 4.0 * (self.phase - 0.5).abs() - 1.0,
        };

        self.phase += self.frequency_hz / self.sample_rate;
        self.phase -= self.phase.floor();

        value
    }
}

impl Generator for AnalogGenerator {
    fn prepare(&mut self, sample_rate: f32, _max_block_size: usize) {
        debug_assert!(sample_rate > 0.0);
        self.sample_rate = sample_rate;
    }

    fn reset(&mut self) {
        self.phase = self.phase_offset;
    }

    fn process_mono(&mut self, _ctx: &ProcessContext, out: &mut [f32]) {
        for sample in out.iter_mut() {
            *sample = self.next_sample();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(osc: &mut AnalogGenerator, n: usize) -> Vec<f32> {
        (0..n).map(|_| osc.next_sample()).collect()
    }

    #[test]
    fn sine_440_zero_crossings_per_second() {
        let mut osc = AnalogGenerator::new(AnalogWaveform::Sine, 0.0, 0.5);
        osc.prepare(48_000.0, 128);
        osc.set_frequency_hz(440.0);

        let samples = render(&mut osc, 48_000);
        let mut crossings: i32 = 0;
        for i in 1..samples.len() {
            if (samples[i - 1] < 0.0) != (samples[i] < 0.0) {
                crossings += 1;
            }
        }
        // A 440 Hz sine has 880 zero crossings per second.
        assert!((crossings - 880).abs() <= 2, "crossings = {crossings}");
    }

    #[test]
    fn sine_440_rms_is_one_over_sqrt2() {
        let mut osc = AnalogGenerator::new(AnalogWaveform::Sine, 0.0, 0.5);
        osc.prepare(48_000.0, 128);
        osc.set_frequency_hz(440.0);

        let samples = render(&mut osc, 48_000);
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        let rms = (sum_sq / samples.len() as f32).sqrt();
        assert!(
            (rms - core::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
            "rms = {rms}"
        );
    }

    #[test]
    fn saw_stays_within_range() {
        let mut osc = AnalogGenerator::new(AnalogWaveform::Saw, 0.0, 0.5);
        osc.prepare(48_000.0, 128);
        osc.set_frequency_hz(220.0);
        for s in render(&mut osc, 48_000) {
            assert!((-1.0..=1.0).contains(&s));
        }
    }

    #[test]
    fn triangle_stays_within_range() {
        let mut osc = AnalogGenerator::new(AnalogWaveform::Triangle, 0.0, 0.5);
        osc.prepare(48_000.0, 128);
        osc.set_frequency_hz(220.0);
        for s in render(&mut osc, 48_000) {
            assert!((-1.0..=1.0).contains(&s));
        }
    }

    #[test]
    fn pulse_width_boundaries_are_stable() {
        for pw in [0.05_f32, 0.5, 0.95] {
            let mut osc = AnalogGenerator::new(AnalogWaveform::Pulse, 0.0, pw);
            osc.prepare(48_000.0, 128);
            osc.set_frequency_hz(220.0);

            let samples = render(&mut osc, 48_000);
            let high_count = samples.iter().filter(|&&s| s > 0.0).count();
            let ratio = high_count as f32 / samples.len() as f32;
            assert!((ratio - pw).abs() < 0.01, "pw = {pw}, ratio = {ratio}");
            for s in &samples {
                assert!(s.is_finite());
            }
        }
    }

    #[test]
    fn phase_wraps_into_unit_range() {
        let mut osc = AnalogGenerator::new(AnalogWaveform::Sine, 0.0, 0.5);
        osc.prepare(48_000.0, 128);
        osc.set_frequency_hz(48_000.0 * 0.9); // near-Nyquist, phase increment ~0.9 per sample
        for _ in 0..1000 {
            osc.next_sample();
            assert!((0.0..1.0).contains(&osc.phase));
        }
    }

    #[test]
    fn reset_restores_phase_offset() {
        let mut osc = AnalogGenerator::new(AnalogWaveform::Sine, 0.25, 0.5);
        osc.prepare(48_000.0, 128);
        osc.set_frequency_hz(440.0);
        osc.next_sample();
        osc.next_sample();
        osc.reset();
        assert_eq!(osc.phase, 0.25);
    }
}
