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
/// Time constant for gain, Q, and bypass smoothing, in seconds.
const PARAM_SMOOTHING_TAU: f32 = 0.006;
/// Crossfade length for discrete filter-shape changes, in seconds.
const KIND_XFADE_SECONDS: f32 = 0.006;
const WET_EPSILON: f32 = 1.0e-4;

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
/// changes are smoothed internally to avoid zipper noise. `enabled` changes
/// crossfade between dry and filtered signal, and filter-shape changes use a
/// short crossfade to avoid hard discontinuities while audio is running.
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
    smoothed_q: SmoothedParam,
    smoothed_gain_db: SmoothedParam,
    wet: SmoothedParam,
    biquad: Biquad,
    fading_biquad: Option<Biquad>,
    active_kind: ButterworthKind,
    kind_xfade_pos: usize,
    kind_xfade_len: usize,
    min_freq_hz: f32,
    max_freq_hz: f32,
    initialized: bool,
}

impl BandState {
    fn new(
        initial_freq_hz: f32,
        initial_kind: ButterworthKind,
        (min_freq_hz, max_freq_hz): (f32, f32),
    ) -> Self {
        let mut smoothed_freq = SmoothedParam::new(initial_freq_hz);
        smoothed_freq.configure(48_000.0, FREQ_SMOOTHING_TAU);
        let mut smoothed_q = SmoothedParam::new(BUTTERWORTH_Q);
        smoothed_q.configure(48_000.0, PARAM_SMOOTHING_TAU);
        let mut smoothed_gain_db = SmoothedParam::new(0.0);
        smoothed_gain_db.configure(48_000.0, PARAM_SMOOTHING_TAU);
        let mut wet = SmoothedParam::new(0.0);
        wet.configure(48_000.0, PARAM_SMOOTHING_TAU);
        Self {
            smoothed_freq,
            smoothed_q,
            smoothed_gain_db,
            wet,
            biquad: Biquad::identity(),
            fading_biquad: None,
            active_kind: initial_kind,
            kind_xfade_pos: 0,
            kind_xfade_len: 0,
            min_freq_hz,
            max_freq_hz,
            initialized: false,
        }
    }

    fn prepare(&mut self, sample_rate: f32) {
        self.smoothed_freq
            .configure(sample_rate, FREQ_SMOOTHING_TAU);
        self.smoothed_q.configure(sample_rate, PARAM_SMOOTHING_TAU);
        self.smoothed_gain_db
            .configure(sample_rate, PARAM_SMOOTHING_TAU);
        self.wet.configure(sample_rate, PARAM_SMOOTHING_TAU);
        self.initialized = false;
    }

    fn reset(&mut self) {
        self.biquad.reset_state();
        self.fading_biquad = None;
        self.kind_xfade_pos = 0;
        self.kind_xfade_len = 0;
        self.initialized = false;
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
        let target_freq = params
            .frequency_hz
            .clamp(self.min_freq_hz, self.max_freq_hz);
        let target_q = params.q.clamp(EQ_Q_RANGE.0, EQ_Q_RANGE.1);
        let target_gain_db = params.gain_db.clamp(EQ_GAIN_DB_RANGE.0, EQ_GAIN_DB_RANGE.1);
        let target_wet = if params.enabled { 1.0 } else { 0.0 };

        if !self.initialized {
            self.smoothed_freq.set_immediate(target_freq);
            self.smoothed_q.set_immediate(target_q);
            self.smoothed_gain_db.set_immediate(target_gain_db);
            self.wet.set_immediate(target_wet);
            self.active_kind = params.kind;
            self.fading_biquad = None;
            self.kind_xfade_pos = 0;
            self.kind_xfade_len = 0;
            self.initialized = true;
        } else {
            self.smoothed_freq.set_target(target_freq);
            self.smoothed_q.set_target(target_q);
            self.smoothed_gain_db.set_target(target_gain_db);
            self.wet.set_target(target_wet);
            self.update_kind(params.kind, sample_rate);
        }

        if self.wet.target() <= WET_EPSILON && self.wet.current() <= WET_EPSILON {
            self.smoothed_freq.set_immediate(target_freq);
            self.smoothed_q.set_immediate(target_q);
            self.smoothed_gain_db.set_immediate(target_gain_db);
            self.active_kind = params.kind;
            self.biquad.reset_state();
            self.fading_biquad = None;
            self.kind_xfade_pos = 0;
            self.kind_xfade_len = 0;
            return (left, right);
        }

        let freq = self.smoothed_freq.tick().min(sample_rate * 0.45);
        let q = self.smoothed_q.tick();
        let gain = db_to_gain(self.smoothed_gain_db.tick());
        let wet = self.wet.tick().clamp(0.0, 1.0);

        set_biquad_coefficients(&mut self.biquad, self.active_kind, freq, q, sample_rate);
        let (wet_l, wet_r) = self.process_wet(left, right);
        let filtered_l = wet_l * gain;
        let filtered_r = wet_r * gain;
        let dry = 1.0 - wet;

        if self.wet.target() <= WET_EPSILON && wet <= WET_EPSILON {
            self.wet.set_immediate(0.0);
            self.biquad.reset_state();
            self.fading_biquad = None;
        }

        (
            left.mul_add(dry, filtered_l * wet),
            right.mul_add(dry, filtered_r * wet),
        )
    }

    fn update_kind(&mut self, target_kind: ButterworthKind, sample_rate: f32) {
        if self.active_kind == target_kind {
            return;
        }

        if self.wet.current() <= WET_EPSILON {
            self.active_kind = target_kind;
            self.biquad.reset_state();
            self.fading_biquad = None;
            self.kind_xfade_pos = 0;
            self.kind_xfade_len = 0;
            return;
        }

        self.fading_biquad = Some(self.biquad);
        self.active_kind = target_kind;
        self.biquad.reset_state();
        self.kind_xfade_pos = 0;
        self.kind_xfade_len = ((sample_rate * KIND_XFADE_SECONDS).round() as usize).max(1);
    }

    fn process_wet(&mut self, left: f32, right: f32) -> (f32, f32) {
        let (new_l, new_r) = self.biquad.process(left, right);
        let Some(fading_biquad) = self.fading_biquad.as_mut() else {
            return (new_l, new_r);
        };

        let (old_l, old_r) = fading_biquad.process(left, right);
        let t = (self.kind_xfade_pos as f32 / self.kind_xfade_len as f32).clamp(0.0, 1.0);
        let old_gain = 1.0 - t;
        let out_l = old_l.mul_add(old_gain, new_l * t);
        let out_r = old_r.mul_add(old_gain, new_r * t);

        self.kind_xfade_pos += 1;
        if self.kind_xfade_pos >= self.kind_xfade_len {
            self.fading_biquad = None;
            self.kind_xfade_pos = 0;
            self.kind_xfade_len = 0;
        }

        (out_l, out_r)
    }
}

fn set_biquad_coefficients(
    biquad: &mut Biquad,
    kind: ButterworthKind,
    freq: f32,
    q: f32,
    sample_rate: f32,
) {
    let (b0, b1, b2, a1, a2) = match kind {
        ButterworthKind::LowPass => lowpass_coefficients(freq, q, sample_rate),
        ButterworthKind::BandPass => bandpass_coefficients(freq, q, sample_rate),
        ButterworthKind::HighPass => highpass_coefficients(freq, q, sample_rate),
    };
    biquad.set_coefficients(b0, b1, b2, a1, a2);
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
            low_state: BandState::new(
                DEFAULT_LOW_FREQ_HZ,
                ButterworthKind::LowPass,
                LOW_FREQ_RANGE,
            ),
            mid_state: BandState::new(
                DEFAULT_MID_FREQ_HZ,
                ButterworthKind::BandPass,
                MID_FREQ_RANGE,
            ),
            high_state: BandState::new(
                DEFAULT_HIGH_FREQ_HZ,
                ButterworthKind::HighPass,
                HIGH_FREQ_RANGE,
            ),
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

    fn process_one(eq: &mut ThreeBandButterworthEq, sample: f32) -> f32 {
        let mut left = [sample];
        let mut right = [sample];
        process(eq, &mut left, &mut right);
        left[0]
    }

    fn max_adjacent_delta(samples: &[f32]) -> f32 {
        samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0_f32, f32::max)
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
    fn gain_q_and_bypass_changes_are_smoothed_while_running() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.mid.enabled = true;
        eq.prepare(48_000.0, 128);

        for sample in sine_signal(1_000.0, 48_000.0, 2048) {
            process_one(&mut eq, sample);
        }

        eq.mid.gain_db = 24.0;
        process_one(&mut eq, 0.25);
        assert!(
            eq.mid_state.smoothed_gain_db.current() < 1.0,
            "gain jumped to {} dB",
            eq.mid_state.smoothed_gain_db.current()
        );

        eq.mid.q = 10.0;
        process_one(&mut eq, 0.25);
        assert!(
            eq.mid_state.smoothed_q.current() < 1.0,
            "Q jumped to {}",
            eq.mid_state.smoothed_q.current()
        );

        eq.mid.enabled = false;
        process_one(&mut eq, 0.25);
        assert!(
            eq.mid_state.wet.current() > 0.9,
            "bypass jumped to wet={}",
            eq.mid_state.wet.current()
        );
    }

    #[test]
    fn filter_kind_changes_crossfade_while_running() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.mid.enabled = true;
        eq.mid.kind = ButterworthKind::BandPass;
        eq.prepare(48_000.0, 128);

        for sample in sine_signal(1_000.0, 48_000.0, 2048) {
            process_one(&mut eq, sample);
        }

        eq.mid.kind = ButterworthKind::HighPass;
        process_one(&mut eq, 0.25);

        assert_eq!(eq.mid_state.active_kind, ButterworthKind::HighPass);
        assert!(eq.mid_state.fading_biquad.is_some());
        assert!(eq.mid_state.kind_xfade_len > 1);
    }

    #[test]
    fn abrupt_eq_changes_do_not_create_large_sample_steps() {
        let mut eq = ThreeBandButterworthEq::new();
        eq.low.enabled = true;
        eq.low.kind = ButterworthKind::LowPass;
        eq.prepare(48_000.0, 128);

        for _ in 0..4096 {
            process_one(&mut eq, 0.2);
        }

        let mut samples = Vec::with_capacity(257);
        samples.push(process_one(&mut eq, 0.2));

        eq.low.kind = ButterworthKind::HighPass;
        eq.low.gain_db = 24.0;
        eq.low.q = 10.0;
        eq.low.enabled = false;

        for _ in 0..256 {
            samples.push(process_one(&mut eq, 0.2));
        }

        let max_delta = max_adjacent_delta(&samples);
        assert!(max_delta < 0.2, "max_delta={max_delta}");
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
        eq.low.enabled = true;
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
