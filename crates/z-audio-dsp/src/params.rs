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
    DEFAULT_HIGH_FREQ_HZ, DEFAULT_LOW_FREQ_HZ, DEFAULT_MID_FREQ_HZ, EQ_GAIN_DB_RANGE, EQ_Q_RANGE,
    HIGH_FREQ_RANGE, LOW_FREQ_RANGE, MID_FREQ_RANGE,
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
    EqLowGainDb = 49,
    EqLowQ = 50,
    EqMidGainDb = 51,
    EqMidQ = 52,
    EqHighGainDb = 53,
    EqHighQ = 54,

    // --- Formula synth (60-79) ---
    FormulaProgram = 60,
    FormulaMacro1 = 61,
    FormulaMacro2 = 62,
    FormulaMacro3 = 63,
    FormulaMacro4 = 64,
    FormulaMacro5 = 65,
    FormulaMacro6 = 66,
    FormulaMacro7 = 67,
    FormulaMacro8 = 68,
    FormulaOutputGain = 69,
    FormulaOversampling = 70,
    FormulaDcBlock = 71,

    // --- Formula piano (80-99) ---
    PianoTone = 80,
    PianoBrightness = 81,
    PianoHammerHardness = 82,
    PianoHammerNoise = 83,
    PianoInharmonicity = 84,
    PianoDecay = 85,
    PianoRelease = 86,
    PianoBodyAmount = 87,
    PianoStereoWidth = 88,
    PianoSympatheticAmount = 89,
    PianoPedalResonance = 90,
    PianoMasterGain = 91,

    // --- Parametric reverb (100-119) ---
    ReverbMix = 100,
    ReverbRoomSize = 101,
    ReverbDecay = 102,
    ReverbPreDelay = 103,
    ReverbDiffusion = 104,
    ReverbDamping = 105,
    ReverbLowCut = 106,
    ReverbHighCut = 107,
    ReverbModRate = 108,
    ReverbModDepth = 109,
    ReverbWidth = 110,
    ReverbEarlyLateMix = 111,
    ReverbOutputGain = 112,

    // --- Limiter (120-139) ---
    LimiterInputGain = 120,
    LimiterThreshold = 121,
    LimiterCeiling = 122,
    LimiterRelease = 123,
    LimiterLookahead = 124,
    LimiterStereoLink = 125,
    LimiterTruePeak = 126,
    LimiterOutputGain = 127,

    // --- Compressor (140-159) ---
    CompressorInputGain = 140,
    CompressorThreshold = 141,
    CompressorRatio = 142,
    CompressorKnee = 143,
    CompressorAttack = 144,
    CompressorRelease = 145,
    CompressorMakeupGain = 146,
    CompressorMix = 147,
    CompressorDetectorMode = 148,
    CompressorStereoLink = 149,
    CompressorSidechainHpf = 150,

    // --- Formula drum set (160-179) ---
    DrumKickLevel = 160,
    DrumSnareLevel = 161,
    DrumTomLevel = 162,
    DrumHatLevel = 163,
    DrumCymbalLevel = 164,
    DrumTuning = 165,
    DrumDecay = 166,
    DrumTone = 167,
    DrumSnareWire = 168,
    DrumRoomAmount = 169,
    DrumStereoWidth = 170,
    DrumMasterGain = 171,

    // --- VCSL sampler piano (180-199) ---
    VcslMasterGain = 180,
    VcslTone = 181,
    VcslVelocityCurve = 182,
    VcslReleaseLevel = 183,
    VcslReleaseTime = 184,
    VcslStereoWidth = 185,

    // --- Generic sampler (200-219) ---
    SamplerMasterGain = 200,
    SamplerRootNote = 201,
    SamplerTune = 202,
    SamplerOffset = 203,
    SamplerVelocityCurve = 204,
    SamplerReleaseTime = 205,
    SamplerStereoWidth = 206,
    SamplerLoopMode = 207,
    SamplerLoopStart = 208,
    SamplerLoopEnd = 209,
    SamplerLoopXfade = 210,
    SamplerUnisonVoices = 211,
    SamplerUnisonDetune = 212,
    SamplerUnisonSpread = 213,

    // --- Diffuser (220-239) ---
    DiffuserMix = 220,
    DiffuserDiffusion = 221,
    DiffuserSize = 222,
    DiffuserWidth = 223,
    DiffuserOutputGain = 224,
    DiffuserAllpassCount = 225,
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
    pub const ALL: [ParamId; 127] = [
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
        ParamId::EqLowGainDb,
        ParamId::EqLowQ,
        ParamId::EqMidGainDb,
        ParamId::EqMidQ,
        ParamId::EqHighGainDb,
        ParamId::EqHighQ,
        ParamId::FormulaProgram,
        ParamId::FormulaMacro1,
        ParamId::FormulaMacro2,
        ParamId::FormulaMacro3,
        ParamId::FormulaMacro4,
        ParamId::FormulaMacro5,
        ParamId::FormulaMacro6,
        ParamId::FormulaMacro7,
        ParamId::FormulaMacro8,
        ParamId::FormulaOutputGain,
        ParamId::FormulaOversampling,
        ParamId::FormulaDcBlock,
        ParamId::PianoTone,
        ParamId::PianoBrightness,
        ParamId::PianoHammerHardness,
        ParamId::PianoHammerNoise,
        ParamId::PianoInharmonicity,
        ParamId::PianoDecay,
        ParamId::PianoRelease,
        ParamId::PianoBodyAmount,
        ParamId::PianoStereoWidth,
        ParamId::PianoSympatheticAmount,
        ParamId::PianoPedalResonance,
        ParamId::PianoMasterGain,
        ParamId::ReverbMix,
        ParamId::ReverbRoomSize,
        ParamId::ReverbDecay,
        ParamId::ReverbPreDelay,
        ParamId::ReverbDiffusion,
        ParamId::ReverbDamping,
        ParamId::ReverbLowCut,
        ParamId::ReverbHighCut,
        ParamId::ReverbModRate,
        ParamId::ReverbModDepth,
        ParamId::ReverbWidth,
        ParamId::ReverbEarlyLateMix,
        ParamId::ReverbOutputGain,
        ParamId::LimiterInputGain,
        ParamId::LimiterThreshold,
        ParamId::LimiterCeiling,
        ParamId::LimiterRelease,
        ParamId::LimiterLookahead,
        ParamId::LimiterStereoLink,
        ParamId::LimiterTruePeak,
        ParamId::LimiterOutputGain,
        ParamId::CompressorInputGain,
        ParamId::CompressorThreshold,
        ParamId::CompressorRatio,
        ParamId::CompressorKnee,
        ParamId::CompressorAttack,
        ParamId::CompressorRelease,
        ParamId::CompressorMakeupGain,
        ParamId::CompressorMix,
        ParamId::CompressorDetectorMode,
        ParamId::CompressorStereoLink,
        ParamId::CompressorSidechainHpf,
        ParamId::DrumKickLevel,
        ParamId::DrumSnareLevel,
        ParamId::DrumTomLevel,
        ParamId::DrumHatLevel,
        ParamId::DrumCymbalLevel,
        ParamId::DrumTuning,
        ParamId::DrumDecay,
        ParamId::DrumTone,
        ParamId::DrumSnareWire,
        ParamId::DrumRoomAmount,
        ParamId::DrumStereoWidth,
        ParamId::DrumMasterGain,
        ParamId::VcslMasterGain,
        ParamId::VcslTone,
        ParamId::VcslVelocityCurve,
        ParamId::VcslReleaseLevel,
        ParamId::VcslReleaseTime,
        ParamId::VcslStereoWidth,
        ParamId::SamplerMasterGain,
        ParamId::SamplerRootNote,
        ParamId::SamplerTune,
        ParamId::SamplerOffset,
        ParamId::SamplerVelocityCurve,
        ParamId::SamplerReleaseTime,
        ParamId::SamplerStereoWidth,
        ParamId::SamplerLoopMode,
        ParamId::SamplerLoopStart,
        ParamId::SamplerLoopEnd,
        ParamId::SamplerLoopXfade,
        ParamId::SamplerUnisonVoices,
        ParamId::SamplerUnisonDetune,
        ParamId::SamplerUnisonSpread,
        ParamId::DiffuserMix,
        ParamId::DiffuserDiffusion,
        ParamId::DiffuserSize,
        ParamId::DiffuserWidth,
        ParamId::DiffuserOutputGain,
        ParamId::DiffuserAllpassCount,
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
                default: 0.0,
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
                default: 0.0,
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
                default: 0.0,
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
            ParamId::EqLowGainDb => ParamMetadata {
                id: self,
                name: "eq_low_gain_db",
                unit: ParamUnit::Linear,
                min: EQ_GAIN_DB_RANGE.0,
                max: EQ_GAIN_DB_RANGE.1,
                default: 0.0,
                step_count: None,
            },
            ParamId::EqLowQ => ParamMetadata {
                id: self,
                name: "eq_low_q",
                unit: ParamUnit::Linear,
                min: EQ_Q_RANGE.0,
                max: EQ_Q_RANGE.1,
                default: crate::BUTTERWORTH_Q,
                step_count: None,
            },
            ParamId::EqMidGainDb => ParamMetadata {
                id: self,
                name: "eq_mid_gain_db",
                unit: ParamUnit::Linear,
                min: EQ_GAIN_DB_RANGE.0,
                max: EQ_GAIN_DB_RANGE.1,
                default: 0.0,
                step_count: None,
            },
            ParamId::EqMidQ => ParamMetadata {
                id: self,
                name: "eq_mid_q",
                unit: ParamUnit::Linear,
                min: EQ_Q_RANGE.0,
                max: EQ_Q_RANGE.1,
                default: crate::BUTTERWORTH_Q,
                step_count: None,
            },
            ParamId::EqHighGainDb => ParamMetadata {
                id: self,
                name: "eq_high_gain_db",
                unit: ParamUnit::Linear,
                min: EQ_GAIN_DB_RANGE.0,
                max: EQ_GAIN_DB_RANGE.1,
                default: 0.0,
                step_count: None,
            },
            ParamId::EqHighQ => ParamMetadata {
                id: self,
                name: "eq_high_q",
                unit: ParamUnit::Linear,
                min: EQ_Q_RANGE.0,
                max: EQ_Q_RANGE.1,
                default: crate::BUTTERWORTH_Q,
                step_count: None,
            },
            ParamId::FormulaProgram => ParamMetadata {
                id: self,
                name: "formula_program",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: (crate::FormulaProgramId::VARIANT_COUNT - 1) as f32,
                default: crate::FormulaProgramId::default().to_param_value(),
                step_count: Some(crate::FormulaProgramId::VARIANT_COUNT),
            },
            ParamId::FormulaMacro1
            | ParamId::FormulaMacro2
            | ParamId::FormulaMacro3
            | ParamId::FormulaMacro4
            | ParamId::FormulaMacro5
            | ParamId::FormulaMacro6
            | ParamId::FormulaMacro7
            | ParamId::FormulaMacro8 => ParamMetadata {
                id: self,
                name: match self {
                    ParamId::FormulaMacro1 => "formula_macro_1",
                    ParamId::FormulaMacro2 => "formula_macro_2",
                    ParamId::FormulaMacro3 => "formula_macro_3",
                    ParamId::FormulaMacro4 => "formula_macro_4",
                    ParamId::FormulaMacro5 => "formula_macro_5",
                    ParamId::FormulaMacro6 => "formula_macro_6",
                    ParamId::FormulaMacro7 => "formula_macro_7",
                    _ => "formula_macro_8",
                },
                unit: ParamUnit::Linear,
                min: 0.0,
                max: 1.0,
                default: 0.0,
                step_count: None,
            },
            ParamId::FormulaOutputGain => ParamMetadata {
                id: self,
                name: "formula_output_gain",
                unit: ParamUnit::Linear,
                min: -24.0,
                max: 24.0,
                default: -6.0,
                step_count: None,
            },
            ParamId::FormulaOversampling => ParamMetadata {
                id: self,
                name: "formula_oversampling",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: 2.0,
                default: 0.0,
                step_count: Some(3),
            },
            ParamId::FormulaDcBlock => ParamMetadata {
                id: self,
                name: "formula_dc_block",
                unit: ParamUnit::Boolean,
                min: 0.0,
                max: 1.0,
                default: 1.0,
                step_count: Some(2),
            },
            ParamId::PianoTone => unit_param(self, "piano_tone", 0.0, 1.0, 0.5),
            ParamId::PianoBrightness => unit_param(self, "piano_brightness", 0.0, 1.0, 0.55),
            ParamId::PianoHammerHardness => {
                unit_param(self, "piano_hammer_hardness", 0.0, 1.0, 0.55)
            }
            ParamId::PianoHammerNoise => unit_param(self, "piano_hammer_noise", 0.0, 1.0, 0.08),
            ParamId::PianoInharmonicity => unit_param(self, "piano_inharmonicity", 0.0, 1.0, 0.45),
            ParamId::PianoDecay => ParamMetadata {
                id: self,
                name: "piano_decay",
                unit: ParamUnit::Seconds,
                min: 0.2,
                max: 8.0,
                default: 2.4,
                step_count: None,
            },
            ParamId::PianoRelease => ParamMetadata {
                id: self,
                name: "piano_release",
                unit: ParamUnit::Seconds,
                min: 0.05,
                max: 5.0,
                default: 0.8,
                step_count: None,
            },
            ParamId::PianoBodyAmount => unit_param(self, "piano_body_amount", 0.0, 1.0, 0.08),
            ParamId::PianoStereoWidth => unit_param(self, "piano_stereo_width", 0.0, 1.0, 0.75),
            ParamId::PianoSympatheticAmount => {
                unit_param(self, "piano_sympathetic_amount", 0.0, 1.0, 0.0)
            }
            ParamId::PianoPedalResonance => {
                unit_param(self, "piano_pedal_resonance", 0.0, 1.0, 0.0)
            }
            ParamId::PianoMasterGain => ParamMetadata {
                id: self,
                name: "piano_master_gain",
                unit: ParamUnit::Linear,
                min: -24.0,
                max: 12.0,
                default: -6.0,
                step_count: None,
            },
            ParamId::ReverbMix => unit_param(self, "reverb_mix", 0.0, 1.0, 0.35),
            ParamId::ReverbRoomSize => unit_param(self, "reverb_room_size", 0.0, 1.0, 0.55),
            ParamId::ReverbDecay => ParamMetadata {
                id: self,
                name: "reverb_decay",
                unit: ParamUnit::Seconds,
                min: 0.1,
                max: 20.0,
                default: 2.2,
                step_count: None,
            },
            ParamId::ReverbPreDelay => unit_param(self, "reverb_pre_delay", 0.0, 250.0, 18.0),
            ParamId::ReverbDiffusion => unit_param(self, "reverb_diffusion", 0.0, 1.0, 0.65),
            ParamId::ReverbDamping => unit_param(self, "reverb_damping", 0.0, 1.0, 0.35),
            ParamId::ReverbLowCut => ParamMetadata {
                id: self,
                name: "reverb_low_cut",
                unit: ParamUnit::Hertz,
                min: 20.0,
                max: 1000.0,
                default: 80.0,
                step_count: None,
            },
            ParamId::ReverbHighCut => ParamMetadata {
                id: self,
                name: "reverb_high_cut",
                unit: ParamUnit::Hertz,
                min: 1000.0,
                max: 20_000.0,
                default: 12_000.0,
                step_count: None,
            },
            ParamId::ReverbModRate => ParamMetadata {
                id: self,
                name: "reverb_mod_rate",
                unit: ParamUnit::Hertz,
                min: 0.0,
                max: 2.0,
                default: 0.0,
                step_count: None,
            },
            ParamId::ReverbModDepth => unit_param(self, "reverb_mod_depth", 0.0, 1.0, 0.0),
            ParamId::ReverbWidth => unit_param(self, "reverb_width", 0.0, 1.0, 0.9),
            ParamId::ReverbEarlyLateMix => {
                unit_param(self, "reverb_early_late_mix", 0.0, 1.0, 0.35)
            }
            ParamId::ReverbOutputGain => db_param(self, "reverb_output_gain", -24.0, 24.0, 0.0),
            ParamId::LimiterInputGain => db_param(self, "limiter_input_gain", -24.0, 24.0, 0.0),
            ParamId::LimiterThreshold => db_param(self, "limiter_threshold", -24.0, 0.0, -0.1),
            ParamId::LimiterCeiling => db_param(self, "limiter_ceiling", -24.0, 0.0, -0.1),
            ParamId::LimiterRelease => unit_param(self, "limiter_release", 1.0, 1000.0, 80.0),
            ParamId::LimiterLookahead => unit_param(self, "limiter_lookahead", 0.0, 10.0, 3.0),
            ParamId::LimiterStereoLink => unit_param(self, "limiter_stereo_link", 0.0, 1.0, 1.0),
            ParamId::LimiterTruePeak => ParamMetadata {
                id: self,
                name: "limiter_true_peak",
                unit: ParamUnit::Boolean,
                min: 0.0,
                max: 1.0,
                default: 0.0,
                step_count: Some(2),
            },
            ParamId::LimiterOutputGain => db_param(self, "limiter_output_gain", -24.0, 24.0, 0.0),
            ParamId::CompressorInputGain => {
                db_param(self, "compressor_input_gain", -24.0, 24.0, 0.0)
            }
            ParamId::CompressorThreshold => {
                db_param(self, "compressor_threshold", -60.0, 0.0, -18.0)
            }
            ParamId::CompressorRatio => unit_param(self, "compressor_ratio", 1.0, 20.0, 4.0),
            ParamId::CompressorKnee => unit_param(self, "compressor_knee", 0.0, 24.0, 0.0),
            ParamId::CompressorAttack => unit_param(self, "compressor_attack", 0.1, 200.0, 10.0),
            ParamId::CompressorRelease => {
                unit_param(self, "compressor_release", 5.0, 2000.0, 120.0)
            }
            ParamId::CompressorMakeupGain => {
                db_param(self, "compressor_makeup_gain", -24.0, 24.0, 0.0)
            }
            ParamId::CompressorMix => unit_param(self, "compressor_mix", 0.0, 1.0, 1.0),
            ParamId::CompressorDetectorMode => ParamMetadata {
                id: self,
                name: "compressor_detector_mode",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: (crate::DetectorMode::VARIANT_COUNT - 1) as f32,
                default: crate::DetectorMode::default().to_param_value(),
                step_count: Some(crate::DetectorMode::VARIANT_COUNT),
            },
            ParamId::CompressorStereoLink => {
                unit_param(self, "compressor_stereo_link", 0.0, 1.0, 1.0)
            }
            ParamId::CompressorSidechainHpf => {
                unit_param(self, "compressor_sidechain_hpf", 0.0, 500.0, 0.0)
            }
            ParamId::DrumKickLevel => unit_param(self, "drum_kick_level", 0.0, 1.0, 0.90),
            ParamId::DrumSnareLevel => unit_param(self, "drum_snare_level", 0.0, 1.0, 0.84),
            ParamId::DrumTomLevel => unit_param(self, "drum_tom_level", 0.0, 1.0, 0.78),
            ParamId::DrumHatLevel => unit_param(self, "drum_hat_level", 0.0, 1.0, 0.55),
            ParamId::DrumCymbalLevel => unit_param(self, "drum_cymbal_level", 0.0, 1.0, 0.62),
            ParamId::DrumTuning => db_param(self, "drum_tuning", -12.0, 12.0, 0.0),
            ParamId::DrumDecay => unit_param(self, "drum_decay", 0.25, 2.50, 1.0),
            ParamId::DrumTone => unit_param(self, "drum_tone", 0.0, 1.0, 0.55),
            ParamId::DrumSnareWire => unit_param(self, "drum_snare_wire", 0.0, 1.0, 0.70),
            ParamId::DrumRoomAmount => unit_param(self, "drum_room_amount", 0.0, 1.0, 0.18),
            ParamId::DrumStereoWidth => unit_param(self, "drum_stereo_width", 0.0, 1.0, 0.72),
            ParamId::DrumMasterGain => db_param(self, "drum_master_gain", -24.0, 12.0, -9.0),
            ParamId::VcslMasterGain => db_param(self, "vcsl_master_gain", -24.0, 12.0, 0.0),
            ParamId::VcslTone => unit_param(self, "vcsl_tone", 0.0, 1.0, 1.0),
            ParamId::VcslVelocityCurve => unit_param(self, "vcsl_velocity_curve", 0.0, 1.0, 0.5),
            ParamId::VcslReleaseLevel => db_param(self, "vcsl_release_level", -24.0, 12.0, 0.0),
            ParamId::VcslReleaseTime => ParamMetadata {
                id: self,
                name: "vcsl_release_time",
                unit: ParamUnit::Seconds,
                min: 0.05,
                max: 5.0,
                default: 0.35,
                step_count: None,
            },
            ParamId::VcslStereoWidth => unit_param(self, "vcsl_stereo_width", 0.0, 1.0, 1.0),
            ParamId::SamplerMasterGain => db_param(self, "sampler_master_gain", -48.0, 12.0, 0.0),
            ParamId::SamplerRootNote => ParamMetadata {
                id: self,
                name: "sampler_root_note",
                unit: ParamUnit::Linear,
                min: 0.0,
                max: 127.0,
                default: 60.0,
                step_count: None,
            },
            ParamId::SamplerTune => ParamMetadata {
                id: self,
                name: "sampler_tune",
                unit: ParamUnit::Linear,
                min: -100.0,
                max: 100.0,
                default: 0.0,
                step_count: None,
            },
            ParamId::SamplerOffset => unit_param(self, "sampler_offset", 0.0, 1.0, 0.0),
            ParamId::SamplerVelocityCurve => {
                unit_param(self, "sampler_velocity_curve", 0.0, 1.0, 0.5)
            }
            ParamId::SamplerReleaseTime => ParamMetadata {
                id: self,
                name: "sampler_release_time",
                unit: ParamUnit::Seconds,
                min: 0.01,
                max: 10.0,
                default: 0.2,
                step_count: None,
            },
            ParamId::SamplerStereoWidth => unit_param(self, "sampler_stereo_width", 0.0, 1.0, 1.0),
            ParamId::SamplerLoopMode => ParamMetadata {
                id: self,
                name: "sampler_loop_mode",
                unit: ParamUnit::Enum,
                min: 0.0,
                max: (crate::LoopMode::VARIANT_COUNT - 1) as f32,
                default: crate::LoopMode::default().to_param_value(),
                step_count: Some(crate::LoopMode::VARIANT_COUNT),
            },
            ParamId::SamplerLoopStart => unit_param(self, "sampler_loop_start", 0.0, 1.0, 0.0),
            ParamId::SamplerLoopEnd => unit_param(self, "sampler_loop_end", 0.0, 1.0, 1.0),
            ParamId::SamplerLoopXfade => ParamMetadata {
                id: self,
                name: "sampler_loop_xfade",
                unit: ParamUnit::Seconds,
                min: 0.0,
                max: 0.2,
                default: 0.01,
                step_count: None,
            },
            ParamId::SamplerUnisonVoices => ParamMetadata {
                id: self,
                name: "sampler_unison_voices",
                unit: ParamUnit::Linear,
                min: 1.0,
                max: 8.0,
                default: 1.0,
                step_count: None,
            },
            ParamId::SamplerUnisonDetune => ParamMetadata {
                id: self,
                name: "sampler_unison_detune",
                unit: ParamUnit::Linear,
                min: 0.0,
                max: 50.0,
                default: 10.0,
                step_count: None,
            },
            ParamId::SamplerUnisonSpread => {
                unit_param(self, "sampler_unison_spread", 0.0, 1.0, 0.5)
            }
            ParamId::DiffuserMix => unit_param(self, "diffuser_mix", 0.0, 1.0, 1.0),
            ParamId::DiffuserDiffusion => unit_param(self, "diffuser_diffusion", 0.0, 1.0, 0.04),
            ParamId::DiffuserSize => unit_param(self, "diffuser_size", 0.0, 1.0, 0.5),
            ParamId::DiffuserWidth => unit_param(self, "diffuser_width", 0.0, 1.0, 1.0),
            ParamId::DiffuserOutputGain => db_param(self, "diffuser_output_gain", -24.0, 24.0, 0.0),
            ParamId::DiffuserAllpassCount => {
                unit_param(self, "diffuser_allpass_count", 1.0, 100.0, 100.0)
            }
        }
    }
}

fn unit_param(id: ParamId, name: &'static str, min: f32, max: f32, default: f32) -> ParamMetadata {
    ParamMetadata {
        id,
        name,
        unit: ParamUnit::Linear,
        min,
        max,
        default,
        step_count: None,
    }
}

fn db_param(id: ParamId, name: &'static str, min: f32, max: f32, default: f32) -> ParamMetadata {
    ParamMetadata {
        id,
        name,
        unit: ParamUnit::Linear,
        min,
        max,
        default,
        step_count: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{ButterworthKind, EnvelopeParams, Gain, GeneratorParams, LfoParams};

    #[test]
    fn all_has_127_unique_variants() {
        assert_eq!(ParamId::ALL.len(), 127);
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
        // All three bands start disabled (pass-through). Enabling a 0 dB
        // band is also unity, so hosts can turn bands on without changing the
        // signal until the user applies boost or cut.
        assert_eq!(
            ParamId::EqLowEnabled.metadata().default,
            if eq.low.enabled { 1.0 } else { 0.0 }
        );
        assert_eq!(ParamId::EqLowFreq.metadata().default, eq.low.frequency_hz);
        assert_eq!(
            ParamId::EqLowType.metadata().default,
            eq.low.kind.to_param_value()
        );
        assert_eq!(ParamId::EqLowGainDb.metadata().default, eq.low.gain_db);
        assert_eq!(ParamId::EqLowQ.metadata().default, eq.low.q);
        assert_eq!(
            ParamId::EqMidEnabled.metadata().default,
            if eq.mid.enabled { 1.0 } else { 0.0 }
        );
        assert_eq!(ParamId::EqMidFreq.metadata().default, eq.mid.frequency_hz);
        assert_eq!(
            ParamId::EqMidType.metadata().default,
            eq.mid.kind.to_param_value()
        );
        assert_eq!(ParamId::EqMidGainDb.metadata().default, eq.mid.gain_db);
        assert_eq!(ParamId::EqMidQ.metadata().default, eq.mid.q);
        assert_eq!(
            ParamId::EqHighEnabled.metadata().default,
            if eq.high.enabled { 1.0 } else { 0.0 }
        );
        assert_eq!(ParamId::EqHighFreq.metadata().default, eq.high.frequency_hz);
        assert_eq!(
            ParamId::EqHighType.metadata().default,
            eq.high.kind.to_param_value()
        );
        assert_eq!(ParamId::EqHighGainDb.metadata().default, eq.high.gain_db);
        assert_eq!(ParamId::EqHighQ.metadata().default, eq.high.q);
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
    fn eq_gain_and_q_metadata_matches_ranges() {
        for id in [
            ParamId::EqLowGainDb,
            ParamId::EqMidGainDb,
            ParamId::EqHighGainDb,
        ] {
            let m = id.metadata();
            assert_eq!(m.min, EQ_GAIN_DB_RANGE.0);
            assert_eq!(m.max, EQ_GAIN_DB_RANGE.1);
            assert_eq!(m.default, 0.0);
        }

        for id in [ParamId::EqLowQ, ParamId::EqMidQ, ParamId::EqHighQ] {
            let m = id.metadata();
            assert_eq!(m.min, EQ_Q_RANGE.0);
            assert_eq!(m.max, EQ_Q_RANGE.1);
            assert_eq!(m.default, crate::BUTTERWORTH_Q);
        }
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
