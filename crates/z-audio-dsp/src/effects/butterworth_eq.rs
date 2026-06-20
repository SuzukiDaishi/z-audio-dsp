//! 3-band Butterworth EQ (low / mid / high), connected in series.

use core::f32::consts::FRAC_1_SQRT_2;

use crate::Effect;
use crate::context::ProcessContext;
use crate::math::{
    Biquad, SmoothedParam, bandpass_coefficients, highpass_coefficients, lowpass_coefficients,
};

/// Default Q for all bands: a 2nd-order Butterworth response (`1 / sqrt(2)`).
pub const BUTTERWORTH_Q: f32 = FRAC_1_SQRT_2;

/// Time constant for frequency-change smoothing, in seconds.
const FREQ_SMOOTHING_TAU: f32 = 0.02;

/// Valid range for [`ButterworthBand::frequency_hz`] on the low band, also
/// used as [`crate::params::ParamId::EqLowFreq`]'s metadata range.
pub(crate) const LOW_FREQ_RANGE: (f32, f32) = (20.0, 2_000.0);
/// Valid range for [`ButterworthBand::frequency_hz`] on the mid band, also
/// used as [`crate::params::ParamId::EqMidFreq`]'s metadata range.
pub(crate) const MID_FREQ_RANGE: (f32, f32) = (80.0, 8_000.0);
/// Valid range for [`ButterworthBand::frequency_hz`] on the high band, also
/// used as [`crate::params::ParamId::EqHighFreq`]'s metadata range.
pub(crate) const HIGH_FREQ_RANGE: (f32, f32) = (1_000.0, 20_000.0);

pub(crate) const DEFAULT_LOW_FREQ_HZ: f32 = 200.0;
pub(crate) const DEFAULT_MID_FREQ_HZ: f32 = 1_000.0;
pub(crate) const DEFAULT_HIGH_FREQ_HZ: f32 = 5_000.0;
pub(crate) const EQ_GAIN_DB_RANGE: (f32, f32) = (-24.0, 24.0);
pub(crate) const EQ_Q_RANGE: (f32, f32) = (0.1, 10.0);

/// The filter shape used by a single [`ButterworthBand`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButterworthKind {
    LowPass,
    BandPass,
    HighPass,
}

impl ButterworthKind {
    /// Number of valid `ParamId::Eq{Low,Mid,High}Type` automation values.
    pub const VARIANT_COUNT: u32 = 3;

    /// Decodes an `ParamId::Eq{Low,Mid,High}Type` automation value, rounding
    /// to the nearest integer and clamping to `0..VARIANT_COUNT - 1`.
    pub fn from_param_value(value: f32) -> Self {
        match value.round().clamp(0.0, (Self::VARIANT_COUNT - 1) as f32) as u32 {
            0 => Self::LowPass,
            1 => Self::BandPass,
            _ => Self::HighPass,
        }
    }

    /// Encodes this filter shape as a `ParamId::Eq{Low,Mid,High}Type`
    /// automation value.
    pub fn to_param_value(self) -> f32 {
        match self {
            Self::LowPass => 0.0,
            Self::BandPass => 1.0,
            Self::HighPass => 2.0,
        }
    }
}

/// User-facing configuration for one band of a [`ThreeBandButterworthEq`].
///
/// Fields are read every sample by the EQ's [`Effect::process_stereo`]
/// implementation, so they can be mutated directly (e.g.
/// `eq.low.frequency_hz = 180.0;`) without any extra setter calls. Frequency
/// changes are smoothed internally to avoid zipper noise; `enabled` toggles
/// take effect immediately but produce an identity (passthrough) filter, so
/// they do not click.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButterworthBand {
    pub enabled: bool,
    pub kind: ButterworthKind,
    pub frequency_hz: f32,
    pub gain_db: f32,
    pub q: f32,
}

/// Internal per-band smoothing + filter state, kept separate from the
/// user-facing [`ButterworthBand`] configuration.
struct BandState {
    smoothed_freq: SmoothedParam,
    biquad: Biquad,
    min_freq_hz: f32,
    max_freq_hz: f32,
}

impl BandState {
    fn new(initial_freq_hz: f32, (min_freq_hz, max_freq_hz): (f32, f32)) -> Self {
        let mut smoothed_freq = SmoothedParam::new(initial_freq_hz);
        smoothed_freq.configure(48_000.0, FREQ_SMOOTHING_TAU);
        Self {
            smoothed_freq,
            biquad: Biquad::identity(),
            min_freq_hz,
            max_freq_hz,
        }
    }

    fn prepare(&mut self, sample_rate: f32) {
        self.smoothed_freq
            .configure(sample_rate, FREQ_SMOOTHING_TAU);
    }

    fn reset(&mut self) {
        self.biquad.reset_state();
    }

    /// Updates smoothing/coefficients from `params` and processes one
    /// stereo sample.
    fn process(
        &mut self,
        params: &ButterworthBand,
        sample_rate: f32,
        left: f32,
        right: f32,
    ) -> (f32, f32) {
        if !params.enabled {
            self.biquad.reset_state();
            return (left, right);
        }

        let target = params
            .frequency_hz
            .clamp(self.min_freq_hz, self.max_freq_hz);
        self.smoothed_freq.set_target(target);
        let freq = self.smoothed_freq.tick().min(sample_rate * 0.45);

        let q = params.q.clamp(EQ_Q_RANGE.0, EQ_Q_RANGE.1);
        let (b0, b1, b2, a1, a2) = match params.kind {
            ButterworthKind::LowPass => lowpass_coefficients(freq, q, sample_rate),
            ButterworthKind::BandPass => bandpass_coefficients(freq, q, sample_rate),
            ButterworthKind::HighPass => highpass_coefficients(freq, q, sample_rate),
        };
        self.biquad.set_coefficients(b0, b1, b2, a1, a2);
        let gain = db_to_gain(params.gain_db.clamp(EQ_GAIN_DB_RANGE.0, EQ_GAIN_DB_RANGE.1));
        let (out_l, out_r) = self.biquad.process(left, right);
        (out_l * gain, out_r * gain)
    }
}

fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// A 3-band Butterworth EQ: low-band, mid-band, and high-band filters
/// connected in series (`input -> low -> mid -> high -> output`).
///
/// Defaults: `low` is a low-pass at 200 Hz, `mid` is a band-pass at 1 kHz,
/// and `high` is a high-pass at 5 kHz, each with `q == BUTTERWORTH_Q`, but
/// **all three start disabled** (pass-through). Because the bands are
/// cascaded in series rather than summed in parallel, enabling the default
/// low-pass(200Hz) and high-pass(5kHz) simultaneously carves out almost the
/// entire musical range *between* them (each is a 2nd-order/-12dB-per-octave
/// filter, so a note a few octaves into either band's stopband is crushed by
/// -50dB or more) — clearly wrong as an out-of-the-box default for an EQ.
/// Bands stay available for the user/host to enable explicitly.
pub struct ThreeBandButterworthEq {
    pub low: ButterworthBand,
    pub mid: ButterworthBand,
    pub high: ButterworthBand,
    low_state: BandState,
    mid_state: BandState,
    high_state: BandState,
    sample_rate: f32,
}

impl ThreeBandButterworthEq {
    /// Creates a new EQ with the default band configuration.
    pub fn new() -> Self {
        Self {
            low: ButterworthBand {
                enabled: false,
                kind: ButterworthKind::LowPass,
                frequency_hz: DEFAULT_LOW_FREQ_HZ,
                gain_db: 0.0,
                q: BUTTERWORTH_Q,
            },
            mid: ButterworthBand {
                enabled: false,
                kind: ButterworthKind::BandPass,
                frequency_hz: DEFAULT_MID_FREQ_HZ,
                gain_db: 0.0,
                q: BUTTERWORTH_Q,
            },
            high: ButterworthBand {
                enabled: false,
                kind: ButterworthKind::HighPass,
                frequency_hz: DEFAULT_HIGH_FREQ_HZ,
                gain_db: 0.0,
                q: BUTTERWORTH_Q,
            },
            low_state: BandState::new(DEFAULT_LOW_FREQ_HZ, LOW_FREQ_RANGE),
            mid_state: BandState::new(DEFAULT_MID_FREQ_HZ, MID_FREQ_RANGE),
            high_state: BandState::new(DEFAULT_HIGH_FREQ_HZ, HIGH_FREQ_RANGE),
            sample_rate: 48_000.0,
        }
    }
}

impl Default for ThreeBandButterworthEq {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for ThreeBandButterworthEq {
    fn prepare(&mut self, sample_rate: f32, _max_block_size: usize) {
        debug_assert!(sample_rate > 0.0);
        self.sample_rate = sample_rate;
        self.low_state.prepare(sample_rate);
        self.mid_state.prepare(sample_rate);
        self.high_state.prepare(sample_rate);
    }

    fn reset(&mut self) {
        self.low_state.reset();
        self.mid_state.reset();
        self.high_state.reset();
    }

    fn process_stereo(&mut self, _ctx: &ProcessContext, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let (l1, r1) = self.low_state.process(&self.low, self.sample_rate, *l, *r);
            let (l2, r2) = self.mid_state.process(&self.mid, self.sample_rate, l1, r1);
            let (l3, r3) = self
                .high_state
                .process(&self.high, self.sample_rate, l2, r2);
            *l = l3;
            *r = r3;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(eq: &mut ThreeBandButterworthEq, left: &mut [f32], right: &mut [f32]) {
        let events = [];
        let ctx = ProcessContext::new(48_000.0, left.len(), 120.0, &events);
        eq.process_stereo(&ctx, left, right);
    }

    fn sine_signal(frequency_hz: f32, sample_rate: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (crate::math::TAU * frequency_hz * i as f32 / sample_rate).sin())
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }

    #[test]
    fn low_pass_attenuates_high_frequency_signal() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.low.enabled = true;
        eq.mid.enabled = false;
        eq.high.enabled = false;
        eq.prepare(48_000.0, 128);

        let mut left = sine_signal(5_000.0, 48_000.0, 4800);
        let mut right = left.clone();
        let input_rms = rms(&left);

        process(&mut eq, &mut left, &mut right);
        let output_rms = rms(&left);

        assert!(
            output_rms < input_rms * 0.1,
            "input={input_rms}, output={output_rms}"
        );
    }

    #[test]
    fn band_pass_attenuates_low_frequency_signal() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.low.enabled = false;
        eq.mid.enabled = true;
        eq.high.enabled = false;
        eq.prepare(48_000.0, 128);

        let mut left = sine_signal(50.0, 48_000.0, 4800);
        let mut right = left.clone();
        let input_rms = rms(&left);

        process(&mut eq, &mut left, &mut right);
        let output_rms = rms(&left);

        assert!(
            output_rms < input_rms * 0.1,
            "input={input_rms}, output={output_rms}"
        );
    }

    #[test]
    fn high_pass_attenuates_low_frequency_signal() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.low.enabled = false;
        eq.mid.enabled = false;
        eq.high.enabled = true;
        eq.prepare(48_000.0, 128);

        let mut left = sine_signal(100.0, 48_000.0, 4800);
        let mut right = left.clone();
        let input_rms = rms(&left);

        process(&mut eq, &mut left, &mut right);
        let output_rms = rms(&left);

        assert!(
            output_rms < input_rms * 0.1,
            "input={input_rms}, output={output_rms}"
        );
    }

    #[test]
    fn disabled_bands_are_identity() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.low.enabled = false;
        eq.mid.enabled = false;
        eq.high.enabled = false;
        eq.prepare(48_000.0, 128);

        let original = sine_signal(440.0, 48_000.0, 256);
        let mut left = original.clone();
        let mut right = original.clone();
        process(&mut eq, &mut left, &mut right);

        for (a, b) in left.iter().zip(original.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
        for (a, b) in right.iter().zip(original.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn disabled_bands_ignore_gain_q_frequency_and_type() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.low.enabled = false;
        eq.low.kind = ButterworthKind::HighPass;
        eq.low.frequency_hz = 1_800.0;
        eq.low.gain_db = 24.0;
        eq.low.q = 10.0;
        eq.mid.enabled = false;
        eq.mid.kind = ButterworthKind::LowPass;
        eq.mid.frequency_hz = 80.0;
        eq.mid.gain_db = -24.0;
        eq.mid.q = 0.1;
        eq.high.enabled = false;
        eq.high.kind = ButterworthKind::BandPass;
        eq.high.frequency_hz = 12_000.0;
        eq.high.gain_db = 18.0;
        eq.high.q = 8.0;
        eq.prepare(48_000.0, 128);

        let original = sine_signal(440.0, 48_000.0, 512);
        let mut left = original.clone();
        let mut right = original.clone();
        process(&mut eq, &mut left, &mut right);

        assert_eq!(left, original);
        assert_eq!(right, original);
    }

    #[test]
    fn enabled_band_gain_db_scales_output_rms() {
        let input = sine_signal(1_000.0, 48_000.0, 48_000);
        let input_rms = rms(&input);

        let mut boost = ThreeBandButterworthEq::new();
        boost.mid.enabled = true;
        boost.mid.gain_db = 6.0;
        boost.prepare(48_000.0, 128);
        let mut boosted = input.clone();
        let mut boosted_r = input.clone();
        process(&mut boost, &mut boosted, &mut boosted_r);
        let boosted_rms = rms(&boosted[4_800..]);

        let mut cut = ThreeBandButterworthEq::new();
        cut.mid.enabled = true;
        cut.mid.gain_db = -12.0;
        cut.prepare(48_000.0, 128);
        let mut cut_left = input.clone();
        let mut cut_right = input.clone();
        process(&mut cut, &mut cut_left, &mut cut_right);
        let cut_rms = rms(&cut_left[4_800..]);

        assert!(
            boosted_rms > input_rms * 1.8,
            "input={input_rms}, boosted={boosted_rms}"
        );
        assert!(
            cut_rms < input_rms * 0.35,
            "input={input_rms}, cut={cut_rms}"
        );
    }

    #[test]
    fn q_changes_band_pass_width() {
        let input = sine_signal(700.0, 48_000.0, 48_000);

        let mut wide = ThreeBandButterworthEq::new();
        wide.mid.enabled = true;
        wide.mid.q = 0.5;
        wide.prepare(48_000.0, 128);
        let mut wide_left = input.clone();
        let mut wide_right = input.clone();
        process(&mut wide, &mut wide_left, &mut wide_right);
        let wide_rms = rms(&wide_left[4_800..]);

        let mut narrow = ThreeBandButterworthEq::new();
        narrow.mid.enabled = true;
        narrow.mid.q = 8.0;
        narrow.prepare(48_000.0, 128);
        let mut narrow_left = input.clone();
        let mut narrow_right = input.clone();
        process(&mut narrow, &mut narrow_left, &mut narrow_right);
        let narrow_rms = rms(&narrow_left[4_800..]);

        assert!(
            wide_rms > narrow_rms * 2.0,
            "wide={wide_rms}, narrow={narrow_rms}"
        );
    }

    #[test]
    fn stable_and_finite_at_multiple_sample_rates() {
        for &sample_rate in &[44_100.0_f32, 48_000.0, 96_000.0] {
            let mut eq = ThreeBandButterworthEq::new();
            eq.prepare(sample_rate, 128);

            let mut left = sine_signal(1_000.0, sample_rate, 2048);
            let mut right = left.clone();
            process(&mut eq, &mut left, &mut right);

            for &s in left.iter().chain(right.iter()) {
                assert!(s.is_finite(), "sample_rate={sample_rate}, sample={s}");
            }
        }
    }

    #[test]
    fn extreme_frequency_settings_do_not_produce_nan() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.low.frequency_hz = 1.0e6;
        eq.mid.frequency_hz = -100.0;
        eq.high.frequency_hz = 0.0;
        eq.prepare(48_000.0, 128);

        let mut left = sine_signal(1_000.0, 48_000.0, 2048);
        let mut right = left.clone();
        process(&mut eq, &mut left, &mut right);

        for &s in left.iter().chain(right.iter()) {
            assert!(s.is_finite());
        }
    }

    #[test]
    fn frequency_is_clamped_to_band_range() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.low.enabled = true;
        eq.low.frequency_hz = 1.0e6; // far above LOW_FREQ_RANGE.1
        eq.mid.enabled = true;
        eq.mid.frequency_hz = -100.0; // below MID_FREQ_RANGE.0
        eq.high.enabled = true;
        eq.high.frequency_hz = 0.0; // below HIGH_FREQ_RANGE.0
        eq.prepare(48_000.0, 128);

        let mut left = vec![0.0_f32; 1];
        let mut right = vec![0.0_f32; 1];
        process(&mut eq, &mut left, &mut right);

        assert_eq!(eq.low_state.smoothed_freq.target(), LOW_FREQ_RANGE.1);
        assert_eq!(eq.mid_state.smoothed_freq.target(), MID_FREQ_RANGE.0);
        assert_eq!(eq.high_state.smoothed_freq.target(), HIGH_FREQ_RANGE.0);
    }

    #[test]
    fn butterworth_kind_param_value_round_trips() {
        for kind in [
            ButterworthKind::LowPass,
            ButterworthKind::BandPass,
            ButterworthKind::HighPass,
        ] {
            let encoded = kind.to_param_value();
            assert_eq!(ButterworthKind::from_param_value(encoded), kind);
        }
    }

    #[test]
    fn butterworth_kind_from_param_value_clamps_out_of_range() {
        assert_eq!(
            ButterworthKind::from_param_value(-1.0),
            ButterworthKind::LowPass
        );
        assert_eq!(
            ButterworthKind::from_param_value(100.0),
            ButterworthKind::HighPass
        );
    }

    #[test]
    fn butterworth_kind_from_param_value_rounds_to_nearest() {
        assert_eq!(
            ButterworthKind::from_param_value(0.4),
            ButterworthKind::LowPass
        );
        assert_eq!(
            ButterworthKind::from_param_value(0.6),
            ButterworthKind::BandPass
        );
    }

    #[test]
    fn frequency_change_is_smoothed_not_instant() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.mid.enabled = false;
        eq.high.enabled = false;
        eq.prepare(48_000.0, 128);

        // Settle at the initial frequency first.
        let mut warm_left = sine_signal(1_000.0, 48_000.0, 256);
        let mut warm_right = warm_left.clone();
        process(&mut eq, &mut warm_left, &mut warm_right);

        eq.low.frequency_hz = 1_800.0;

        let mut left = sine_signal(1_000.0, 48_000.0, 1);
        let mut right = left.clone();
        process(&mut eq, &mut left, &mut right);

        // The smoothed frequency should not have jumped all the way to the
        // new target after a single sample.
        assert!((eq.low_state.smoothed_freq.current() - 1_800.0).abs() > 1.0);
    }
}
