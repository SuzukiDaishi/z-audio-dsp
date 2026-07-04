//! Stereo Schroeder allpass diffuser.

use crate::Effect;
use crate::context::ProcessContext;
use crate::delay::DelayLine;
use crate::math::{SmoothedParam, TAU, db_to_linear, flush_denormal, lerp};

const BASE_STAGE_COUNT: usize = 4;
const MAX_ORDER: usize = 100;
const DEFAULT_DIFFUSION: f32 = BASE_STAGE_COUNT as f32 / MAX_ORDER as f32;
const LEFT_DELAYS_MS: [f32; BASE_STAGE_COUNT] = [4.7, 3.6, 12.7, 9.3];
const RIGHT_DELAYS_MS: [f32; BASE_STAGE_COUNT] = [5.1, 3.9, 11.9, 8.7];
const STAGE_GAINS: [f32; BASE_STAGE_COUNT] = [0.62, 0.59, 0.56, 0.53];
const SIZE_MIN_SCALE: f32 = 0.5;
const SIZE_MAX_SCALE: f32 = 1.5;
const PARAM_SMOOTHING_TAU: f32 = 0.008;
const DIFFUSION_SMOOTHING_TAU: f32 = 0.003;
const SIZE_SMOOTHING_TAU: f32 = 0.04;
const STAGE_COEFF_TAU: f32 = 0.003;
const LOOP_DAMPING_START_HZ: f32 = 9_000.0;
const LOOP_DAMPING_END_HZ: f32 = 5_200.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffuserParams {
    pub mix: f32,
    pub diffusion: f32,
    pub allpass_count: f32,
    pub size: f32,
    pub width: f32,
    pub output_gain_db: f32,
}

impl Default for DiffuserParams {
    fn default() -> Self {
        Self {
            mix: 1.0,
            diffusion: DEFAULT_DIFFUSION,
            allpass_count: MAX_ORDER as f32,
            size: 0.5,
            width: 1.0,
            output_gain_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SchroederAllpass {
    delay: DelayLine,
    feedback_lp: f32,
}

impl SchroederAllpass {
    fn prepare(&mut self, max_delay_samples: usize) {
        self.delay.prepare(max_delay_samples);
    }

    fn clear(&mut self) {
        self.delay.clear();
        self.feedback_lp = 0.0;
    }

    #[inline]
    fn process(&mut self, input: f32, delay_samples: f32, gain: f32, damping_coeff: f32) -> f32 {
        let delayed = self.delay.read_frac_lerp(delay_samples.max(1.0));
        let output = gain.mul_add(input, delayed);
        let feedback = input - gain * output;
        self.feedback_lp += (feedback - self.feedback_lp) * damping_coeff;
        self.delay.push(lerp(feedback, self.feedback_lp, 0.34));
        flush_denormal(output)
    }
}

#[derive(Debug, Clone)]
struct DiffuserSmoothing {
    mix: SmoothedParam,
    diffusion: SmoothedParam,
    size: SmoothedParam,
    width: SmoothedParam,
    output_gain_db: SmoothedParam,
}

impl DiffuserSmoothing {
    fn new(params: DiffuserParams) -> Self {
        Self {
            mix: SmoothedParam::new(params.mix),
            diffusion: SmoothedParam::new(params.diffusion),
            size: SmoothedParam::new(params.size),
            width: SmoothedParam::new(params.width),
            output_gain_db: SmoothedParam::new(params.output_gain_db),
        }
    }

    fn configure(&mut self, sample_rate: f32) {
        self.mix.configure(sample_rate, PARAM_SMOOTHING_TAU);
        self.diffusion
            .configure(sample_rate, DIFFUSION_SMOOTHING_TAU);
        self.size.configure(sample_rate, SIZE_SMOOTHING_TAU);
        self.width.configure(sample_rate, PARAM_SMOOTHING_TAU);
        self.output_gain_db
            .configure(sample_rate, PARAM_SMOOTHING_TAU);
    }

    fn set_immediate(&mut self, params: DiffuserParams) {
        self.mix.set_immediate(params.mix);
        self.diffusion.set_immediate(params.diffusion);
        self.size.set_immediate(params.size);
        self.width.set_immediate(params.width);
        self.output_gain_db.set_immediate(params.output_gain_db);
    }

    fn set_targets(&mut self, params: DiffuserParams) {
        self.mix.set_target(params.mix);
        self.diffusion.set_target(params.diffusion);
        self.size.set_target(params.size);
        self.width.set_target(params.width);
        self.output_gain_db.set_target(params.output_gain_db);
    }
}

pub struct Diffuser {
    sample_rate: f32,
    params: DiffuserParams,
    left: [SchroederAllpass; MAX_ORDER],
    right: [SchroederAllpass; MAX_ORDER],
    left_delay_ms: [f32; MAX_ORDER],
    right_delay_ms: [f32; MAX_ORDER],
    stage_gains: [f32; MAX_ORDER],
    damping_coeffs: [f32; MAX_ORDER],
    stage_coefficients: [SmoothedParam; MAX_ORDER],
    smoothing: DiffuserSmoothing,
    initialized: bool,
}

impl Default for Diffuser {
    fn default() -> Self {
        Self::new(DiffuserParams::default())
    }
}

impl Diffuser {
    pub fn new(params: DiffuserParams) -> Self {
        let params = sanitize(params);
        let initial_allpass_count = allpass_count_from_param(params.allpass_count);
        let initial_effective_order = effective_order(params.diffusion, initial_allpass_count);
        let mut diffuser = Self {
            sample_rate: 48_000.0,
            params,
            left: core::array::from_fn(|_| SchroederAllpass::default()),
            right: core::array::from_fn(|_| SchroederAllpass::default()),
            left_delay_ms: core::array::from_fn(|stage| stage_delay_ms(LEFT_DELAYS_MS, stage)),
            right_delay_ms: core::array::from_fn(|stage| stage_delay_ms(RIGHT_DELAYS_MS, stage)),
            stage_gains: core::array::from_fn(stage_gain),
            damping_coeffs: [1.0; MAX_ORDER],
            stage_coefficients: core::array::from_fn(|stage| {
                SmoothedParam::new(stage_activation(initial_effective_order, stage))
            }),
            smoothing: DiffuserSmoothing::new(params),
            initialized: false,
        };
        diffuser.prepare(48_000.0, 512);
        diffuser
    }

    pub fn set_params(&mut self, params: DiffuserParams) {
        self.params = sanitize(params);
    }

    pub fn params(&self) -> DiffuserParams {
        self.params
    }
}

impl Effect for Diffuser {
    fn prepare(&mut self, sample_rate: f32, _max_block_size: usize) {
        self.sample_rate = sample_rate.max(1.0);
        self.smoothing.configure(self.sample_rate);
        for coefficient in &mut self.stage_coefficients {
            coefficient.configure(self.sample_rate, STAGE_COEFF_TAU);
        }
        self.damping_coeffs = core::array::from_fn(|stage| damping_coeff(stage, self.sample_rate));
        let max_delay_ms = self
            .left_delay_ms
            .iter()
            .zip(self.right_delay_ms.iter())
            .map(|(&left, &right)| left.max(right))
            .fold(0.0_f32, f32::max)
            * SIZE_MAX_SCALE;
        let max_delay_samples = ms_to_samples(max_delay_ms, self.sample_rate) + 4;
        for stage in &mut self.left {
            stage.prepare(max_delay_samples);
        }
        for stage in &mut self.right {
            stage.prepare(max_delay_samples);
        }
        self.initialized = false;
    }

    fn reset(&mut self) {
        for stage in &mut self.left {
            stage.clear();
        }
        for stage in &mut self.right {
            stage.clear();
        }
        self.initialized = false;
    }

    fn process_stereo(&mut self, _ctx: &ProcessContext, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        let params = self.params;
        if !self.initialized {
            self.smoothing.set_immediate(params);
            let allpass_count = allpass_count_from_param(params.allpass_count);
            self.set_stage_coefficients_immediate(params.diffusion, allpass_count);
            self.initialized = true;
        } else {
            self.smoothing.set_targets(params);
        }

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let dry_l = *l;
            let dry_r = *r;
            let mix = self.smoothing.mix.tick().clamp(0.0, 1.0);
            let diffusion = self.smoothing.diffusion.tick().clamp(0.0, 1.0);
            let allpass_count = allpass_count_from_param(params.allpass_count);
            let size = self.smoothing.size.tick().clamp(0.0, 1.0);
            let width = self.smoothing.width.tick().clamp(0.0, 1.0);
            let output_gain = db_to_linear(self.smoothing.output_gain_db.tick());
            let scale = size_scale(size);
            let target_order = effective_order(diffusion, allpass_count);
            let wet_presence = wet_presence_from_order(target_order);

            let mut wet_l = dry_l;
            let mut wet_r = dry_r;
            for i in 0..allpass_count {
                let coefficient_target = stage_activation(target_order, i);
                self.stage_coefficients[i].set_target(coefficient_target);
                let coefficient = self.stage_coefficients[i].tick().clamp(0.0, 1.0);
                let processed_l = self.left[i].process(
                    wet_l,
                    ms_to_samples_f32(self.left_delay_ms[i] * scale, self.sample_rate),
                    self.stage_gains[i],
                    self.damping_coeffs[i],
                );
                let processed_r = self.right[i].process(
                    wet_r,
                    ms_to_samples_f32(self.right_delay_ms[i] * scale, self.sample_rate),
                    self.stage_gains[i],
                    self.damping_coeffs[i],
                );
                wet_l = lerp(wet_l, processed_l, coefficient);
                wet_r = lerp(wet_r, processed_r, coefficient);
            }

            let mid = (wet_l + wet_r) * 0.5;
            let side_width = lerp(1.0, width, wet_presence);
            let side = (wet_l - wet_r) * 0.5 * side_width;
            wet_l = mid + side;
            wet_r = mid - side;

            let wet_mix = mix * wet_presence;
            *l = flush_denormal(dry_l.mul_add(1.0 - wet_mix, wet_l * wet_mix) * output_gain);
            *r = flush_denormal(dry_r.mul_add(1.0 - wet_mix, wet_r * wet_mix) * output_gain);
        }
    }
}

impl Diffuser {
    fn set_stage_coefficients_immediate(&mut self, diffusion: f32, allpass_count: usize) {
        let target_order = effective_order(diffusion, allpass_count);
        for (stage, stage_coefficient) in self.stage_coefficients.iter_mut().enumerate() {
            stage_coefficient.set_immediate(stage_activation(target_order, stage));
        }
    }
}

fn size_scale(size: f32) -> f32 {
    lerp(SIZE_MIN_SCALE, SIZE_MAX_SCALE, size.clamp(0.0, 1.0))
}

fn allpass_count_from_param(allpass_count: f32) -> usize {
    allpass_count.round().clamp(1.0, MAX_ORDER as f32) as usize
}

fn effective_order(diffusion: f32, allpass_count: usize) -> f32 {
    diffusion.clamp(0.0, 1.0) * allpass_count as f32
}

fn stage_activation(effective_order: f32, stage: usize) -> f32 {
    smoothstep((effective_order - stage as f32).clamp(0.0, 1.0))
}

fn wet_presence_from_order(effective_order: f32) -> f32 {
    smoothstep(effective_order.clamp(0.0, 1.0))
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn stage_delay_ms(base_delays: [f32; BASE_STAGE_COUNT], stage: usize) -> f32 {
    let base = base_delays[stage % BASE_STAGE_COUNT];
    let lane = (stage % BASE_STAGE_COUNT) as f32;
    let cycle = (stage / BASE_STAGE_COUNT) as f32;
    let channel_seed = if base_delays[0] < 4.9 {
        0xA511_E9B3
    } else {
        0x63D8_2F17
    };
    let low_discrepancy = ((stage as f32 + lane * 0.37 + base * 0.071) * 0.618_034).fract();
    let jitter = lerp(-0.38, 0.38, hash01(stage, channel_seed));
    let spread = 0.72 + low_discrepancy * 0.82;
    let stretch = 1.0 + cycle * 0.032;
    (base * spread * stretch + jitter).clamp(2.1, 38.0)
}

fn stage_gain(stage: usize) -> f32 {
    let taper = (1.0 - stage as f32 * 0.0016).max(0.84);
    STAGE_GAINS[stage % BASE_STAGE_COUNT] * taper
}

fn damping_coeff(stage: usize, sample_rate: f32) -> f32 {
    let depth = (stage as f32 / 32.0).clamp(0.0, 1.0);
    let cutoff_hz = lerp(LOOP_DAMPING_START_HZ, LOOP_DAMPING_END_HZ, depth);
    let sample_rate = sample_rate.max(1.0);
    1.0 - (-(TAU * cutoff_hz / sample_rate)).exp()
}

fn hash01(stage: usize, seed: u32) -> f32 {
    let mut x = (stage as u32).wrapping_mul(0x9E37_79B9) ^ seed;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x as f32 / u32::MAX as f32
}

fn ms_to_samples(ms: f32, sample_rate: f32) -> usize {
    ms_to_samples_f32(ms, sample_rate).round() as usize
}

fn ms_to_samples_f32(ms: f32, sample_rate: f32) -> f32 {
    ms.max(0.0) * 0.001 * sample_rate.max(1.0)
}

fn sanitize(params: DiffuserParams) -> DiffuserParams {
    DiffuserParams {
        mix: params.mix.clamp(0.0, 1.0),
        diffusion: params.diffusion.clamp(0.0, 1.0),
        allpass_count: params.allpass_count.clamp(1.0, MAX_ORDER as f32),
        size: params.size.clamp(0.0, 1.0),
        width: params.width.clamp(0.0, 1.0),
        output_gain_db: params.output_gain_db.clamp(-24.0, 24.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(len: usize) -> ProcessContext<'static> {
        ProcessContext::new(48_000.0, len, 120.0, &[])
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

    fn process_one(diffuser: &mut Diffuser, left: f32, right: f32) -> (f32, f32) {
        let mut l = [left];
        let mut r = [right];
        diffuser.process_stereo(&ctx(1), &mut l, &mut r);
        (l[0], r[0])
    }

    fn max_adjacent_delta(samples: &[f32]) -> f32 {
        samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0_f32, f32::max)
    }

    #[test]
    fn mix_zero_is_dry() {
        let mut diffuser = Diffuser::new(DiffuserParams {
            mix: 0.0,
            ..Default::default()
        });
        diffuser.prepare(48_000.0, 128);
        let original = sine_signal(440.0, 48_000.0, 512);
        let mut left = original.clone();
        let mut right = original.clone();
        diffuser.process_stereo(&ctx(512), &mut left, &mut right);
        assert_eq!(left, original);
        assert_eq!(right, original);
    }

    #[test]
    fn diffusion_zero_is_dry_even_when_mix_is_full() {
        let mut diffuser = Diffuser::new(DiffuserParams {
            mix: 1.0,
            diffusion: 0.0,
            ..Default::default()
        });
        diffuser.prepare(48_000.0, 128);
        let original_l = sine_signal(440.0, 48_000.0, 512);
        let original_r = sine_signal(660.0, 48_000.0, 512);
        let mut left = original_l.clone();
        let mut right = original_r.clone();
        diffuser.process_stereo(&ctx(512), &mut left, &mut right);
        assert_eq!(left, original_l);
        assert_eq!(right, original_r);
    }

    #[test]
    fn max_diffusion_reaches_more_than_ten_times_the_original_four_stages() {
        let order = effective_order(1.0, allpass_count_from_param(100.0));
        assert_eq!(order, 100.0);
        assert!(order >= (BASE_STAGE_COUNT * 10) as f32);
    }

    #[test]
    fn allpass_count_caps_diffusion_work() {
        let count = allpass_count_from_param(12.0);
        assert_eq!(count, 12);
        assert_eq!(effective_order(1.0, count), 12.0);
        assert_eq!(effective_order(0.5, count), 6.0);
    }

    #[test]
    fn fractional_diffusion_crossfades_between_neighboring_stages() {
        let order = effective_order(0.525, allpass_count_from_param(20.0));
        assert_eq!(stage_activation(order, 9), 1.0);
        assert_eq!(stage_activation(order, 11), 0.0);
        let fractional = stage_activation(order, 10);
        assert!(fractional > 0.0, "fractional={fractional}");
        assert!(fractional < 1.0, "fractional={fractional}");
    }

    #[test]
    fn low_count_delays_do_not_repeat_the_four_stage_pattern() {
        let decorrelated_pairs = (0..16)
            .filter(|&stage| {
                let a = stage_delay_ms(LEFT_DELAYS_MS, stage);
                let b = stage_delay_ms(LEFT_DELAYS_MS, stage + BASE_STAGE_COUNT);
                (a - b).abs() > 0.2
            })
            .count();
        assert!(
            decorrelated_pairs >= 12,
            "decorrelated_pairs={decorrelated_pairs}"
        );
    }

    #[test]
    fn full_wet_keeps_sine_energy_reasonable() {
        let mut diffuser = Diffuser::new(DiffuserParams {
            mix: 1.0,
            diffusion: 0.4,
            ..Default::default()
        });
        diffuser.prepare(48_000.0, 128);
        let mut left = sine_signal(1_000.0, 48_000.0, 48_000);
        let mut right = left.clone();
        let input = rms(&left[24_000..]);
        diffuser.process_stereo(&ctx(left.len()), &mut left, &mut right);
        let output = rms(&left[24_000..]);
        assert!(output > input * 0.75, "input={input}, output={output}");
        assert!(output < input * 1.25, "input={input}, output={output}");
    }

    #[test]
    fn low_count_impulse_tail_decays_instead_of_ringing_up() {
        let mut diffuser = Diffuser::new(DiffuserParams {
            mix: 1.0,
            diffusion: 1.0,
            allpass_count: 20.0,
            ..Default::default()
        });
        diffuser.prepare(48_000.0, 128);
        let mut left = vec![0.0; 48_000];
        let mut right = vec![0.0; 48_000];
        left[0] = 1.0;
        right[0] = 1.0;
        diffuser.process_stereo(&ctx(left.len()), &mut left, &mut right);

        let early = rms(&left[256..12_000]);
        let late = rms(&left[36_000..]);
        assert!(late < early * 0.45, "early={early}, late={late}");
    }

    #[test]
    fn parameters_are_smoothed_while_running() {
        let mut diffuser = Diffuser::default();
        diffuser.prepare(48_000.0, 128);
        for sample in sine_signal(800.0, 48_000.0, 2048) {
            process_one(&mut diffuser, sample, sample);
        }

        diffuser.set_params(DiffuserParams {
            mix: 0.0,
            diffusion: 1.0,
            allpass_count: 100.0,
            size: 1.0,
            width: 0.0,
            output_gain_db: 24.0,
        });
        process_one(&mut diffuser, 0.25, -0.25);

        assert!(
            diffuser.smoothing.mix.current() > 0.95,
            "mix jumped to {}",
            diffuser.smoothing.mix.current()
        );
        assert!(
            diffuser.smoothing.size.current() < 0.55,
            "size jumped to {}",
            diffuser.smoothing.size.current()
        );
        assert!(
            diffuser.smoothing.output_gain_db.current() < 1.0,
            "output gain jumped to {} dB",
            diffuser.smoothing.output_gain_db.current()
        );
    }

    #[test]
    fn abrupt_changes_do_not_create_large_sample_steps() {
        let mut diffuser = Diffuser::default();
        diffuser.prepare(48_000.0, 128);
        let warm = sine_signal(932.0, 48_000.0, 4096);
        for sample in warm {
            process_one(&mut diffuser, sample, -sample);
        }

        let input = sine_signal(932.0, 48_000.0, 257);
        let mut samples = Vec::with_capacity(input.len());
        samples.push(process_one(&mut diffuser, input[0], -input[0]).0);
        diffuser.set_params(DiffuserParams {
            mix: 0.25,
            diffusion: 1.0,
            allpass_count: 100.0,
            size: 1.0,
            width: 0.0,
            output_gain_db: 12.0,
        });
        for &sample in &input[1..] {
            samples.push(process_one(&mut diffuser, sample, -sample).0);
        }

        let max_delta = max_adjacent_delta(&samples);
        assert!(max_delta < 0.25, "max_delta={max_delta}");
    }

    #[test]
    fn diffusion_changes_remain_finite() {
        let mut diffuser = Diffuser::default();
        diffuser.prepare(48_000.0, 128);
        for sample in sine_signal(932.0, 48_000.0, 4096) {
            process_one(&mut diffuser, sample, -sample);
        }

        diffuser.set_params(DiffuserParams {
            diffusion: 1.0,
            ..Default::default()
        });

        for sample in sine_signal(932.0, 48_000.0, 512) {
            let (left, right) = process_one(&mut diffuser, sample, -sample);
            assert!(left.is_finite(), "left={left}");
            assert!(right.is_finite(), "right={right}");
        }
    }

    #[test]
    fn stable_and_finite_at_multiple_sample_rates() {
        for &sample_rate in &[44_100.0_f32, 48_000.0, 96_000.0] {
            let mut diffuser = Diffuser::default();
            diffuser.prepare(sample_rate, 128);
            let mut left = sine_signal(1_000.0, sample_rate, 4096);
            let mut right = sine_signal(1_330.0, sample_rate, 4096);
            diffuser.process_stereo(&ctx(left.len()), &mut left, &mut right);
            for &sample in left.iter().chain(right.iter()) {
                assert!(
                    sample.is_finite(),
                    "sample_rate={sample_rate}, sample={sample}"
                );
            }
        }
    }
}
