//! Low-frequency oscillator with multiple waveforms and fixed target routing.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::Modulator;
use crate::context::ProcessContext;
use crate::math::TAU;

/// LFO waveform shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LfoWaveform {
    #[default]
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    RandomHold,
}

impl LfoWaveform {
    /// Number of valid `ParamId::LfoWaveform` automation values.
    pub const VARIANT_COUNT: u32 = 6;

    /// Decodes a `ParamId::LfoWaveform` automation value, rounding to the
    /// nearest integer and clamping to `0..VARIANT_COUNT - 1`.
    pub fn from_param_value(value: f32) -> Self {
        match value.round().clamp(0.0, (Self::VARIANT_COUNT - 1) as f32) as u32 {
            0 => Self::Sine,
            1 => Self::Triangle,
            2 => Self::SawUp,
            3 => Self::SawDown,
            4 => Self::Square,
            _ => Self::RandomHold,
        }
    }

    /// Encodes this waveform as a `ParamId::LfoWaveform` automation value.
    pub fn to_param_value(self) -> f32 {
        match self {
            Self::Sine => 0.0,
            Self::Triangle => 1.0,
            Self::SawUp => 2.0,
            Self::SawDown => 3.0,
            Self::Square => 4.0,
            Self::RandomHold => 5.0,
        }
    }
}

/// Fixed modulation targets available to a voice's LFO in Phase 1. Arbitrary
/// routing (a modulation matrix) is a future addition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LfoTarget {
    #[default]
    None,
    Gain,
    PitchSemitone,
    EqLowFreq,
    EqMidFreq,
    EqHighFreq,
}

impl LfoTarget {
    /// Number of valid `ParamId::LfoTarget` automation values.
    pub const VARIANT_COUNT: u32 = 6;

    /// Decodes a `ParamId::LfoTarget` automation value, rounding to the
    /// nearest integer and clamping to `0..VARIANT_COUNT - 1`.
    pub fn from_param_value(value: f32) -> Self {
        match value.round().clamp(0.0, (Self::VARIANT_COUNT - 1) as f32) as u32 {
            0 => Self::None,
            1 => Self::Gain,
            2 => Self::PitchSemitone,
            3 => Self::EqLowFreq,
            4 => Self::EqMidFreq,
            _ => Self::EqHighFreq,
        }
    }

    /// Encodes this target as a `ParamId::LfoTarget` automation value.
    pub fn to_param_value(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Gain => 1.0,
            Self::PitchSemitone => 2.0,
            Self::EqLowFreq => 3.0,
            Self::EqMidFreq => 4.0,
            Self::EqHighFreq => 5.0,
        }
    }
}

/// LFO configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LfoParams {
    /// When `false`, [`Lfo::next_sample`] holds its phase and returns `0.0`,
    /// silencing modulation regardless of `target`/`amount`.
    pub enabled: bool,
    pub waveform: LfoWaveform,
    pub rate_hz: f32,
    pub amount: f32,
    pub target: LfoTarget,
    pub retrigger: bool,
}

impl Default for LfoParams {
    fn default() -> Self {
        Self {
            enabled: true,
            waveform: LfoWaveform::Sine,
            rate_hz: 5.0,
            amount: 0.0,
            target: LfoTarget::None,
            retrigger: true,
        }
    }
}

/// A low-frequency oscillator producing a control signal in `[-1.0, 1.0]`.
///
/// Interpretation of the output depends on [`LfoParams::target`] and is
/// applied by the caller (e.g. `z-audio-synth`'s `Voice`/`SimpleSynth`):
///
/// ```text
/// Gain                : gain *= 1.0 + lfo * amount
/// PitchSemitone       : note += lfo * amount   (semitones, before midi_note_to_hz)
/// EqLowFreq/Mid/High  : frequency *= 2^(lfo * amount)   (amount in octaves)
/// ```
pub struct Lfo {
    sample_rate: f32,
    phase: f32,
    params: LfoParams,
    random_value: f32,
    rng: SmallRng,
    last_value: f32,
}

impl Lfo {
    /// Creates a new LFO seeded with `seed` (used for [`LfoWaveform::RandomHold`]).
    pub fn new(params: LfoParams, seed: u64) -> Self {
        let mut rng = SmallRng::seed_from_u64(seed);
        let random_value = rng.gen_range(-1.0..=1.0);
        Self {
            sample_rate: 48_000.0,
            phase: 0.0,
            params,
            random_value,
            rng,
            last_value: random_value,
        }
    }

    /// Replaces the LFO parameters.
    pub fn set_params(&mut self, params: LfoParams) {
        self.params = params;
    }

    /// Returns the current parameters.
    pub fn params(&self) -> &LfoParams {
        &self.params
    }

    /// If [`LfoParams::retrigger`] is set, resets the LFO phase (and, for
    /// [`LfoWaveform::RandomHold`], draws a new held value).
    pub fn note_on(&mut self) {
        if self.params.retrigger {
            self.phase = 0.0;
            self.random_value = self.rng.gen_range(-1.0..=1.0);
        }
    }

    /// Returns the most recently produced output value without advancing.
    pub fn last_value(&self) -> f32 {
        self.last_value
    }

    /// Computes the next output sample and advances the LFO phase.
    ///
    /// While [`LfoParams::enabled`] is `false`, the phase is held and `0.0`
    /// is returned without consuming any randomness.
    pub fn next_sample(&mut self) -> f32 {
        if !self.params.enabled {
            self.last_value = 0.0;
            return 0.0;
        }

        let value = match self.params.waveform {
            LfoWaveform::Sine => (self.phase * TAU).sin(),
            LfoWaveform::Triangle => 4.0 * (self.phase - 0.5).abs() - 1.0,
            LfoWaveform::SawUp => 2.0 * self.phase - 1.0,
            LfoWaveform::SawDown => 1.0 - 2.0 * self.phase,
            LfoWaveform::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoWaveform::RandomHold => self.random_value,
        };

        self.phase += self.params.rate_hz / self.sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            if self.params.waveform == LfoWaveform::RandomHold {
                self.random_value = self.rng.gen_range(-1.0..=1.0);
            }
        }

        self.last_value = value;
        value
    }
}

impl Modulator for Lfo {
    fn prepare(&mut self, sample_rate: f32, _max_block_size: usize) {
        debug_assert!(sample_rate > 0.0);
        self.sample_rate = sample_rate;
    }

    fn reset(&mut self) {
        self.phase = 0.0;
    }

    fn process_control(&mut self, _ctx: &ProcessContext, out: &mut [f32]) {
        for o in out.iter_mut() {
            *o = self.next_sample();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_with(waveform: LfoWaveform) -> LfoParams {
        LfoParams {
            enabled: true,
            waveform,
            rate_hz: 5.0,
            amount: 1.0,
            target: LfoTarget::Gain,
            retrigger: true,
        }
    }

    #[test]
    fn all_waveforms_stay_within_range() {
        for waveform in [
            LfoWaveform::Sine,
            LfoWaveform::Triangle,
            LfoWaveform::SawUp,
            LfoWaveform::SawDown,
            LfoWaveform::Square,
            LfoWaveform::RandomHold,
        ] {
            let mut lfo = Lfo::new(params_with(waveform), 7);
            lfo.prepare(48_000.0, 128);
            for _ in 0..48_000 {
                let v = lfo.next_sample();
                assert!(v.is_finite());
                assert!((-1.0..=1.0).contains(&v), "{waveform:?} produced {v}");
            }
        }
    }

    #[test]
    fn retrigger_resets_phase() {
        let mut lfo = Lfo::new(params_with(LfoWaveform::SawUp), 1);
        lfo.prepare(48_000.0, 128);
        for _ in 0..1000 {
            lfo.next_sample();
        }
        assert!(lfo.phase > 0.0);
        lfo.note_on();
        assert_eq!(lfo.phase, 0.0);
    }

    #[test]
    fn no_retrigger_keeps_phase() {
        let mut params = params_with(LfoWaveform::SawUp);
        params.retrigger = false;
        let mut lfo = Lfo::new(params, 1);
        lfo.prepare(48_000.0, 128);
        for _ in 0..1000 {
            lfo.next_sample();
        }
        let phase_before = lfo.phase;
        lfo.note_on();
        assert_eq!(lfo.phase, phase_before);
    }

    #[test]
    fn random_hold_changes_about_once_per_cycle() {
        let mut params = params_with(LfoWaveform::RandomHold);
        params.rate_hz = 10.0;
        let mut lfo = Lfo::new(params, 99);
        lfo.prepare(48_000.0, 128);

        let samples_per_cycle = (48_000.0 / 10.0) as usize;
        let mut values = Vec::new();
        let mut last = f32::NAN;
        for _ in 0..(samples_per_cycle * 2) {
            let v = lfo.next_sample();
            if v != last {
                values.push(v);
                last = v;
            }
        }
        // One held value per cycle => ~2 distinct values across 2 cycles
        // (allow the boundary sample to add at most one extra).
        assert!((1..=3).contains(&values.len()), "values = {values:?}");
    }

    #[test]
    fn target_variants_are_distinct() {
        assert_ne!(LfoTarget::Gain, LfoTarget::PitchSemitone);
        assert_ne!(LfoTarget::EqLowFreq, LfoTarget::EqMidFreq);
        assert_ne!(LfoTarget::EqMidFreq, LfoTarget::EqHighFreq);
    }

    #[test]
    fn enabled_defaults_to_true() {
        assert!(LfoParams::default().enabled);
    }

    #[test]
    fn disabled_lfo_holds_phase_and_outputs_zero() {
        let mut params = params_with(LfoWaveform::SawUp);
        params.enabled = false;
        let mut lfo = Lfo::new(params, 1);
        lfo.prepare(48_000.0, 128);

        for _ in 0..1000 {
            assert_eq!(lfo.next_sample(), 0.0);
        }
        assert_eq!(lfo.phase, 0.0);
        assert_eq!(lfo.last_value(), 0.0);
    }

    #[test]
    fn re_enabling_lfo_resumes_from_held_phase() {
        let mut params = params_with(LfoWaveform::SawUp);
        let mut lfo = Lfo::new(params, 1);
        lfo.prepare(48_000.0, 128);

        for _ in 0..100 {
            lfo.next_sample();
        }
        let phase_at_disable = lfo.phase;

        params.enabled = false;
        lfo.set_params(params);
        for _ in 0..100 {
            lfo.next_sample();
        }
        assert_eq!(lfo.phase, phase_at_disable);

        params.enabled = true;
        lfo.set_params(params);
        assert!(lfo.next_sample().is_finite());
        assert!(lfo.phase > phase_at_disable);
    }

    #[test]
    fn lfo_waveform_param_value_round_trips() {
        for waveform in [
            LfoWaveform::Sine,
            LfoWaveform::Triangle,
            LfoWaveform::SawUp,
            LfoWaveform::SawDown,
            LfoWaveform::Square,
            LfoWaveform::RandomHold,
        ] {
            let encoded = waveform.to_param_value();
            assert_eq!(LfoWaveform::from_param_value(encoded), waveform);
        }
    }

    #[test]
    fn lfo_waveform_from_param_value_clamps_out_of_range() {
        assert_eq!(LfoWaveform::from_param_value(-1.0), LfoWaveform::Sine);
        assert_eq!(
            LfoWaveform::from_param_value(100.0),
            LfoWaveform::RandomHold
        );
    }

    #[test]
    fn lfo_waveform_from_param_value_rounds_to_nearest() {
        assert_eq!(LfoWaveform::from_param_value(1.4), LfoWaveform::Triangle);
        assert_eq!(LfoWaveform::from_param_value(1.6), LfoWaveform::SawUp);
    }

    #[test]
    fn lfo_target_param_value_round_trips() {
        for target in [
            LfoTarget::None,
            LfoTarget::Gain,
            LfoTarget::PitchSemitone,
            LfoTarget::EqLowFreq,
            LfoTarget::EqMidFreq,
            LfoTarget::EqHighFreq,
        ] {
            let encoded = target.to_param_value();
            assert_eq!(LfoTarget::from_param_value(encoded), target);
        }
    }

    #[test]
    fn lfo_target_from_param_value_clamps_out_of_range() {
        assert_eq!(LfoTarget::from_param_value(-1.0), LfoTarget::None);
        assert_eq!(LfoTarget::from_param_value(100.0), LfoTarget::EqHighFreq);
    }
}
