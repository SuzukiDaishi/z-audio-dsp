//! Audio-rate signal generators.
//!
//! The public API uses `Generator` terminology (not `Oscillator`); internal
//! implementations may use other names (e.g. [`AnalogGenerator`]).

pub mod analog;
pub mod noise;

pub use analog::{AnalogGenerator, AnalogWaveform};
pub use noise::NoiseGenerator;

use crate::Generator;
use crate::context::ProcessContext;

/// The kind of generator a [`GeneratorInstance`] should be constructed as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GeneratorKind {
    #[default]
    Sine,
    Triangle,
    Saw,
    Pulse,
    Noise,
}

impl GeneratorKind {
    /// Number of valid `ParamId::GeneratorKind` automation values.
    pub const VARIANT_COUNT: u32 = 5;

    /// Decodes a `ParamId::GeneratorKind` automation value, rounding to the
    /// nearest integer and clamping to `0..VARIANT_COUNT - 1`.
    pub fn from_param_value(value: f32) -> Self {
        match value.round().clamp(0.0, (Self::VARIANT_COUNT - 1) as f32) as u32 {
            0 => Self::Sine,
            1 => Self::Triangle,
            2 => Self::Saw,
            3 => Self::Pulse,
            _ => Self::Noise,
        }
    }

    /// Encodes this kind as a `ParamId::GeneratorKind` automation value.
    pub fn to_param_value(self) -> f32 {
        match self {
            Self::Sine => 0.0,
            Self::Triangle => 1.0,
            Self::Saw => 2.0,
            Self::Pulse => 3.0,
            Self::Noise => 4.0,
        }
    }
}

/// Configuration for a single voice's generator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeneratorParams {
    pub kind: GeneratorKind,
    pub gain: f32,
    pub phase_offset: f32,
    pub pulse_width: f32,
    /// Stereo position in `[-1.0, 1.0]` (`-1.0` = full left, `0.0` = center,
    /// `1.0` = full right), applied via a constant-power pan law.
    pub pan: f32,
}

impl Default for GeneratorParams {
    fn default() -> Self {
        Self {
            kind: GeneratorKind::Sine,
            gain: 1.0,
            phase_offset: 0.0,
            pulse_width: 0.5,
            pan: 0.0,
        }
    }
}

/// A concrete generator instance, dispatching to the implementation selected
/// by [`GeneratorParams::kind`].
pub enum GeneratorInstance {
    Analog(AnalogGenerator),
    Noise(NoiseGenerator),
}

impl GeneratorInstance {
    /// Constructs a generator instance matching `params.kind`, seeded with
    /// `seed` (used only by [`NoiseGenerator`]).
    pub fn from_params(params: &GeneratorParams, seed: u64) -> Self {
        match params.kind {
            GeneratorKind::Sine => GeneratorInstance::Analog(AnalogGenerator::new(
                AnalogWaveform::Sine,
                params.phase_offset,
                params.pulse_width,
            )),
            GeneratorKind::Triangle => GeneratorInstance::Analog(AnalogGenerator::new(
                AnalogWaveform::Triangle,
                params.phase_offset,
                params.pulse_width,
            )),
            GeneratorKind::Saw => GeneratorInstance::Analog(AnalogGenerator::new(
                AnalogWaveform::Saw,
                params.phase_offset,
                params.pulse_width,
            )),
            GeneratorKind::Pulse => GeneratorInstance::Analog(AnalogGenerator::new(
                AnalogWaveform::Pulse,
                params.phase_offset,
                params.pulse_width,
            )),
            GeneratorKind::Noise => GeneratorInstance::Noise(NoiseGenerator::new(seed)),
        }
    }

    /// Sets the oscillator frequency in Hz. No-op for [`NoiseGenerator`].
    pub fn set_frequency_hz(&mut self, frequency_hz: f32) {
        match self {
            GeneratorInstance::Analog(g) => g.set_frequency_hz(frequency_hz),
            GeneratorInstance::Noise(_) => {}
        }
    }

    /// Sets the pulse width (clamped to `[0.05, 0.95]`). No-op for variants
    /// that don't use it.
    pub fn set_pulse_width(&mut self, pulse_width: f32) {
        if let GeneratorInstance::Analog(g) = self {
            g.set_pulse_width(pulse_width);
        }
    }

    /// Produces the next single sample.
    pub fn next_sample(&mut self) -> f32 {
        match self {
            GeneratorInstance::Analog(g) => g.next_sample(),
            GeneratorInstance::Noise(g) => g.next_sample(),
        }
    }
}

impl Generator for GeneratorInstance {
    fn prepare(&mut self, sample_rate: f32, max_block_size: usize) {
        match self {
            GeneratorInstance::Analog(g) => g.prepare(sample_rate, max_block_size),
            GeneratorInstance::Noise(g) => g.prepare(sample_rate, max_block_size),
        }
    }

    fn reset(&mut self) {
        match self {
            GeneratorInstance::Analog(g) => g.reset(),
            GeneratorInstance::Noise(g) => g.reset(),
        }
    }

    fn process_mono(&mut self, ctx: &ProcessContext, out: &mut [f32]) {
        match self {
            GeneratorInstance::Analog(g) => g.process_mono(ctx, out),
            GeneratorInstance::Noise(g) => g.process_mono(ctx, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_kind_param_value_round_trips() {
        for kind in [
            GeneratorKind::Sine,
            GeneratorKind::Triangle,
            GeneratorKind::Saw,
            GeneratorKind::Pulse,
            GeneratorKind::Noise,
        ] {
            let encoded = kind.to_param_value();
            assert_eq!(GeneratorKind::from_param_value(encoded), kind);
        }
    }

    #[test]
    fn generator_kind_from_param_value_clamps_out_of_range() {
        assert_eq!(GeneratorKind::from_param_value(-1.0), GeneratorKind::Sine);
        assert_eq!(GeneratorKind::from_param_value(100.0), GeneratorKind::Noise);
    }

    #[test]
    fn generator_kind_from_param_value_rounds_to_nearest() {
        assert_eq!(
            GeneratorKind::from_param_value(1.4),
            GeneratorKind::Triangle
        );
        assert_eq!(GeneratorKind::from_param_value(1.6), GeneratorKind::Saw);
    }

    #[test]
    fn generator_kind_default_encodes_to_zero() {
        assert_eq!(GeneratorKind::default().to_param_value(), 0.0);
        assert_eq!(GeneratorKind::default(), GeneratorKind::Sine);
    }
}
