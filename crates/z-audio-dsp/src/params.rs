//! Stable parameter identifiers and metadata used by
//! [`crate::EventKind::Param`] automation events.
//!
//! Variants are grouped by component with gaps between groups (10 IDs per
//! group) to leave room for future additions without renumbering existing
//! IDs. [`ParamId::metadata`] describes each parameter's name, unit, valid
//! range, and default value, which a plugin wrapper can use to build its
//! parameter list and to normalize/clamp incoming automation values; see
//! `SimpleSynth::set_param` in `z-audio-synth` for the runtime dispatch.

use crate::effects::butterworth_eq::{
    DEFAULT_HIGH_FREQ_HZ, DEFAULT_LOW_FREQ_HZ, DEFAULT_MID_FREQ_HZ, HIGH_FREQ_RANGE,
    LOW_FREQ_RANGE, MID_FREQ_RANGE,
};

/// A stable identifier for an automatable parameter.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamId {
    // --- Synth (0-9) ---
    MasterGain = 0,
    MaxPolyphony = 1,
    GeneratorKind = 2,

    // --- Generator (10-19) ---
    GeneratorGain = 10,
    GeneratorPulseWidth = 11,
    GeneratorPhaseOffset = 12,
    GeneratorPan = 13,

    // --- Amp envelope (20-29) ---
    EnvAttack = 20,
    EnvDecay = 21,
    EnvSustain = 22,
    EnvRelease = 23,
    EnvCurve = 24,

    // --- LFO (30-39) ---
    LfoEnabled = 30,
    LfoWaveform = 31,
    LfoRateHz = 32,
    LfoAmount = 33,
    LfoTarget = 34,
    LfoRetrigger = 35,

    // --- 3-band EQ (40-59) ---
    EqLowEnabled = 40,
    EqLowFreq = 41,
    EqLowType = 42,
    EqMidEnabled = 43,
    EqMidFreq = 44,
    EqMidType = 45,
    EqHighEnabled = 46,
    EqHighFreq = 47,
    EqHighType = 48,
}

/// The physical interpretation of a [`ParamMetadata`] value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamUnit {
    /// A dimensionless linear scalar (gain multiplier, pan position, level,
    /// voice count, etc.).
    Linear,
    /// Frequency in Hertz.
    Hertz,
    /// Time in seconds.
    Seconds,
    /// `0.0` (false) or `1.0` (true); see [`ParamMetadata::step_count`].
    Boolean,
    /// An integer index in `0..step_count` selecting one of a fixed set of
    /// named variants, encoded as `f32` (see e.g.
    /// [`crate::GeneratorKind::from_param_value`]/`to_param_value`).
    Enum,
}

/// Static metadata describing a [`ParamId`]'s valid range, default value,
/// and unit.
///
/// `min..=max` is the range that `SimpleSynth::set_param`-style setters
/// clamp (continuous parameters) or round-and-clamp (`Enum`/`Boolean`
/// parameters, via the relevant `from_param_value`) incoming values to.
/// `step_count` is `Some(n)` for `Enum`/`Boolean` parameters, giving the
/// number of valid integer values `0..n` (encoded as `f32`, i.e.
/// `max == (n - 1) as f32`); it is `None` for continuous parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamMetadata {
    pub id: ParamId,
    pub name: &'static str,
    pub unit: ParamUnit,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub step_count: Option<u32>,
}

impl ParamId {
    /// Every [`ParamId`] variant, in declaration order.
    pub const ALL: [ParamId; 27] = [
        ParamId::MasterGain,
        ParamId::MaxPolyphony,
        ParamId::GeneratorKind,
        ParamId::GeneratorGain,
        ParamId::GeneratorPulseWidth,
        ParamId::GeneratorPhaseOffset,
        ParamId::GeneratorPan,
        ParamId::EnvAttack,
        ParamId::EnvDecay,
        ParamId::EnvSustain,
        ParamId::EnvRelease,
        ParamId::EnvCurve,
        ParamId::LfoEnabled,
        ParamId::LfoWaveform,
        ParamId::LfoRateHz,
        ParamId::LfoAmount,
        ParamId::LfoTarget,
        ParamId::LfoRetrigger,
        ParamId::EqLowEnabled,
        ParamId::EqLowFreq,
        ParamId::EqLowType,
        ParamId::EqMidEnabled,
        ParamId::EqMidFreq,
        ParamId::EqMidType,
        ParamId::EqHighEnabled,
        ParamId::EqHighFreq,
        ParamId::EqHighType,
    ];

    /// Returns this parameter's metadata (name, unit, range, default).
    pub fn metadata(self) -> ParamMetadata {
        match self {
            ParamId::MasterGain => ParamMetadata {
                id: self,
                name: "master_gain",
                unit: ParamUnit::Linear,
                min: 0.0,
                max: 2.0,
                default: 1.0,
                step_count: None,
            },
            ParamId::MaxPolyphony => ParamMetadata {
                id: self,
                name: "max_polyphony",
                unit: ParamUnit::Linear,
                min: 1.0,
                max: 64.0,
                default: 16.0,
                step_count: None,
            },
            ParamId::GeneratorKind => ParamMetadata {
                id: self,
                name: "generator_kind",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: (crate::GeneratorKind::VARIANT_COUNT - 1) as f32,
                default: crate::GeneratorKind::default().to_param_value(),
                step_count: Some(crate::GeneratorKind::VARIANT_COUNT),
            },
            ParamId::GeneratorGain => ParamMetadata {
                id: self,
                name: "generator_gain",
                unit: ParamUnit::Linear,
                min: 0.0,
                max: 2.0,
                default: 1.0,
                step_count: None,
            },
            ParamId::GeneratorPulseWidth => ParamMetadata {
                id: self,
                name: "generator_pulse_width",
                unit: ParamUnit::Linear,
                min: 0.05,
                max: 0.95,
                default: 0.5,
                step_count: None,
            },
            ParamId::GeneratorPhaseOffset => ParamMetadata {
                id: self,
                name: "generator_phase_offset",
                unit: ParamUnit::Linear,
                min: 0.0,
                max: 1.0,
                default: 0.0,
                step_count: None,
            },
            ParamId::GeneratorPan => ParamMetadata {
                id: self,
                name: "generator_pan",
                unit: ParamUnit::Linear,
                min: -1.0,
                max: 1.0,
                default: 0.0,
                step_count: None,
            },
            ParamId::EnvAttack => ParamMetadata {
                id: self,
                name: "env_attack",
                unit: ParamUnit::Seconds,
                min: 0.0,
                max: 10.0,
                default: 0.01,
                step_count: None,
            },
            ParamId::EnvDecay => ParamMetadata {
                id: self,
                name: "env_decay",
                unit: ParamUnit::Seconds,
                min: 0.0,
                max: 10.0,
                default: 0.1,
                step_count: None,
            },
            ParamId::EnvSustain => ParamMetadata {
                id: self,
                name: "env_sustain",
                unit: ParamUnit::Linear,
                min: 0.0,
                max: 1.0,
                default: 0.7,
                step_count: None,
            },
            ParamId::EnvRelease => ParamMetadata {
                id: self,
                name: "env_release",
                unit: ParamUnit::Seconds,
                min: 0.0,
                max: 10.0,
                default: 0.2,
                step_count: None,
            },
            ParamId::EnvCurve => ParamMetadata {
                id: self,
                name: "env_curve",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: (crate::EnvelopeCurve::VARIANT_COUNT - 1) as f32,
                default: crate::EnvelopeCurve::default().to_param_value(),
                step_count: Some(crate::EnvelopeCurve::VARIANT_COUNT),
            },
            ParamId::LfoEnabled => ParamMetadata {
                id: self,
                name: "lfo_enabled",
                unit: ParamUnit::Boolean,
                min: 0.0,
                max: 1.0,
                default: 1.0,
                step_count: Some(2),
            },
            ParamId::LfoWaveform => ParamMetadata {
                id: self,
                name: "lfo_waveform",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: (crate::LfoWaveform::VARIANT_COUNT - 1) as f32,
                default: crate::LfoWaveform::default().to_param_value(),
                step_count: Some(crate::LfoWaveform::VARIANT_COUNT),
            },
            ParamId::LfoRateHz => ParamMetadata {
                id: self,
                name: "lfo_rate_hz",
                unit: ParamUnit::Hertz,
                min: 0.01,
                max: 20.0,
                default: 5.0,
                step_count: None,
            },
            ParamId::LfoAmount => ParamMetadata {
                id: self,
                name: "lfo_amount",
                unit: ParamUnit::Linear,
                min: 0.0,
                max: 12.0,
                default: 0.0,
                step_count: None,
            },
            ParamId::LfoTarget => ParamMetadata {
                id: self,
                name: "lfo_target",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: (crate::LfoTarget::VARIANT_COUNT - 1) as f32,
                default: crate::LfoTarget::default().to_param_value(),
                step_count: Some(crate::LfoTarget::VARIANT_COUNT),
            },
            ParamId::LfoRetrigger => ParamMetadata {
                id: self,
                name: "lfo_retrigger",
                unit: ParamUnit::Boolean,
                min: 0.0,
                max: 1.0,
                default: 1.0,
                step_count: Some(2),
            },
            ParamId::EqLowEnabled => ParamMetadata {
                id: self,
                name: "eq_low_enabled",
                unit: ParamUnit::Boolean,
                min: 0.0,
                max: 1.0,
                default: 1.0,
                step_count: Some(2),
            },
            ParamId::EqLowFreq => ParamMetadata {
                id: self,
                name: "eq_low_freq",
                unit: ParamUnit::Hertz,
                min: LOW_FREQ_RANGE.0,
                max: LOW_FREQ_RANGE.1,
                default: DEFAULT_LOW_FREQ_HZ,
                step_count: None,
            },
            ParamId::EqLowType => ParamMetadata {
                id: self,
                name: "eq_low_type",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: (crate::ButterworthKind::VARIANT_COUNT - 1) as f32,
                default: crate::ButterworthKind::LowPass.to_param_value(),
                step_count: Some(crate::ButterworthKind::VARIANT_COUNT),
            },
            ParamId::EqMidEnabled => ParamMetadata {
                id: self,
                name: "eq_mid_enabled",
                unit: ParamUnit::Boolean,
                min: 0.0,
                max: 1.0,
                default: 1.0,
                step_count: Some(2),
            },
            ParamId::EqMidFreq => ParamMetadata {
                id: self,
                name: "eq_mid_freq",
                unit: ParamUnit::Hertz,
                min: MID_FREQ_RANGE.0,
                max: MID_FREQ_RANGE.1,
                default: DEFAULT_MID_FREQ_HZ,
                step_count: None,
            },
            ParamId::EqMidType => ParamMetadata {
                id: self,
                name: "eq_mid_type",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: (crate::ButterworthKind::VARIANT_COUNT - 1) as f32,
                default: crate::ButterworthKind::BandPass.to_param_value(),
                step_count: Some(crate::ButterworthKind::VARIANT_COUNT),
            },
            ParamId::EqHighEnabled => ParamMetadata {
                id: self,
                name: "eq_high_enabled",
                unit: ParamUnit::Boolean,
                min: 0.0,
                max: 1.0,
                default: 1.0,
                step_count: Some(2),
            },
            ParamId::EqHighFreq => ParamMetadata {
                id: self,
                name: "eq_high_freq",
                unit: ParamUnit::Hertz,
                min: HIGH_FREQ_RANGE.0,
                max: HIGH_FREQ_RANGE.1,
                default: DEFAULT_HIGH_FREQ_HZ,
                step_count: None,
            },
            ParamId::EqHighType => ParamMetadata {
                id: self,
                name: "eq_high_type",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: (crate::ButterworthKind::VARIANT_COUNT - 1) as f32,
                default: crate::ButterworthKind::HighPass.to_param_value(),
                step_count: Some(crate::ButterworthKind::VARIANT_COUNT),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{ButterworthKind, EnvelopeParams, Gain, GeneratorParams, LfoParams};

    #[test]
    fn all_has_27_unique_variants() {
        assert_eq!(ParamId::ALL.len(), 27);
        let mut seen = HashSet::new();
        for id in ParamId::ALL {
            assert!(seen.insert(id as u32), "duplicate ParamId in ALL: {id:?}");
        }
    }

    #[test]
    fn metadata_id_matches_self() {
        for id in ParamId::ALL {
            assert_eq!(id.metadata().id, id);
        }
    }

    #[test]
    fn metadata_min_is_less_than_max() {
        for id in ParamId::ALL {
            let m = id.metadata();
            assert!(m.min < m.max, "{}: min={} max={}", m.name, m.min, m.max);
        }
    }

    #[test]
    fn metadata_default_is_within_range() {
        for id in ParamId::ALL {
            let m = id.metadata();
            assert!(
                m.default >= m.min && m.default <= m.max,
                "{}: default={} not in [{}, {}]",
                m.name,
                m.default,
                m.min,
                m.max
            );
        }
    }

    #[test]
    fn metadata_names_are_unique_and_non_empty() {
        let mut seen = HashSet::new();
        for id in ParamId::ALL {
            let name = id.metadata().name;
            assert!(!name.is_empty());
            assert!(seen.insert(name), "duplicate metadata name: {name}");
        }
    }

    #[test]
    fn enum_and_boolean_params_have_consistent_step_count() {
        for id in ParamId::ALL {
            let m = id.metadata();
            match m.unit {
                ParamUnit::Enum | ParamUnit::Boolean => {
                    let steps = m
                        .step_count
                        .unwrap_or_else(|| panic!("{}: missing step_count", m.name));
                    assert!(steps >= 2, "{}: step_count={steps}", m.name);
                    assert_eq!(m.min, 0.0, "{}: enum/bool min must be 0", m.name);
                    assert_eq!(
                        m.max,
                        (steps - 1) as f32,
                        "{}: max doesn't match step_count",
                        m.name
                    );
                }
                ParamUnit::Linear | ParamUnit::Hertz | ParamUnit::Seconds => {
                    assert!(
                        m.step_count.is_none(),
                        "{}: continuous param has step_count",
                        m.name
                    );
                }
            }
        }
    }

    #[test]
    fn master_gain_metadata_matches_gain_default() {
        let m = ParamId::MasterGain.metadata();
        assert_eq!(m.default, Gain::default().current_gain());
        assert_eq!(m.unit, ParamUnit::Linear);
    }

    #[test]
    fn max_polyphony_metadata_default_is_sixteen() {
        // Mirrors `SimpleSynthConfig::default().max_polyphony` in
        // `z-audio-synth` (not depended on here).
        assert_eq!(ParamId::MaxPolyphony.metadata().default, 16.0);
    }

    #[test]
    fn generator_metadata_matches_generator_params_default() {
        let g = GeneratorParams::default();
        assert_eq!(
            ParamId::GeneratorKind.metadata().default,
            g.kind.to_param_value()
        );
        assert_eq!(ParamId::GeneratorGain.metadata().default, g.gain);
        assert_eq!(
            ParamId::GeneratorPulseWidth.metadata().default,
            g.pulse_width
        );
        assert_eq!(
            ParamId::GeneratorPhaseOffset.metadata().default,
            g.phase_offset
        );
        assert_eq!(ParamId::GeneratorPan.metadata().default, g.pan);
    }

    #[test]
    fn envelope_metadata_matches_envelope_params_default() {
        let e = EnvelopeParams::default();
        assert_eq!(ParamId::EnvAttack.metadata().default, e.attack);
        assert_eq!(ParamId::EnvDecay.metadata().default, e.decay);
        assert_eq!(ParamId::EnvSustain.metadata().default, e.sustain);
        assert_eq!(ParamId::EnvRelease.metadata().default, e.release);
        assert_eq!(
            ParamId::EnvCurve.metadata().default,
            e.curve.to_param_value()
        );
    }

    #[test]
    fn lfo_metadata_matches_lfo_params_default() {
        let l = LfoParams::default();
        assert_eq!(
            ParamId::LfoEnabled.metadata().default,
            if l.enabled { 1.0 } else { 0.0 }
        );
        assert_eq!(
            ParamId::LfoWaveform.metadata().default,
            l.waveform.to_param_value()
        );
        assert_eq!(ParamId::LfoRateHz.metadata().default, l.rate_hz);
        assert_eq!(ParamId::LfoAmount.metadata().default, l.amount);
        assert_eq!(
            ParamId::LfoTarget.metadata().default,
            l.target.to_param_value()
        );
        assert_eq!(
            ParamId::LfoRetrigger.metadata().default,
            if l.retrigger { 1.0 } else { 0.0 }
        );
    }

    #[test]
    fn eq_metadata_matches_eq_defaults() {
        let eq = crate::ThreeBandButterworthEq::new();
        assert_eq!(ParamId::EqLowEnabled.metadata().default, 1.0);
        assert_eq!(ParamId::EqLowFreq.metadata().default, eq.low.frequency_hz);
        assert_eq!(
            ParamId::EqLowType.metadata().default,
            eq.low.kind.to_param_value()
        );
        assert_eq!(ParamId::EqMidEnabled.metadata().default, 1.0);
        assert_eq!(ParamId::EqMidFreq.metadata().default, eq.mid.frequency_hz);
        assert_eq!(
            ParamId::EqMidType.metadata().default,
            eq.mid.kind.to_param_value()
        );
        assert_eq!(ParamId::EqHighEnabled.metadata().default, 1.0);
        assert_eq!(ParamId::EqHighFreq.metadata().default, eq.high.frequency_hz);
        assert_eq!(
            ParamId::EqHighType.metadata().default,
            eq.high.kind.to_param_value()
        );
    }

    #[test]
    fn eq_freq_metadata_matches_band_ranges() {
        assert_eq!(ParamId::EqLowFreq.metadata().min, LOW_FREQ_RANGE.0);
        assert_eq!(ParamId::EqLowFreq.metadata().max, LOW_FREQ_RANGE.1);
        assert_eq!(ParamId::EqMidFreq.metadata().min, MID_FREQ_RANGE.0);
        assert_eq!(ParamId::EqMidFreq.metadata().max, MID_FREQ_RANGE.1);
        assert_eq!(ParamId::EqHighFreq.metadata().min, HIGH_FREQ_RANGE.0);
        assert_eq!(ParamId::EqHighFreq.metadata().max, HIGH_FREQ_RANGE.1);
    }

    #[test]
    fn generator_pulse_width_metadata_matches_clamp_range() {
        let m = ParamId::GeneratorPulseWidth.metadata();
        assert_eq!(m.min, 0.05);
        assert_eq!(m.max, 0.95);
    }

    #[test]
    fn eq_type_defaults_match_per_band_filter_shapes() {
        assert_eq!(
            ParamId::EqLowType.metadata().default,
            ButterworthKind::LowPass.to_param_value()
        );
        assert_eq!(
            ParamId::EqMidType.metadata().default,
            ButterworthKind::BandPass.to_param_value()
        );
        assert_eq!(
            ParamId::EqHighType.metadata().default,
            ButterworthKind::HighPass.to_param_value()
        );
    }
}
