//! Feed-forward compressor.

use crate::Effect;
use crate::context::ProcessContext;
use crate::dynamics::{BallisticsFilter, DetectorMode, LevelDetector};
use crate::math::{db_to_linear, flush_denormal, linear_to_db};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressorParams {
    pub input_gain_db: f32,
    pub threshold_db: f32,
    pub ratio: f32,
    pub knee_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_gain_db: f32,
    pub mix: f32,
    pub detector_mode: DetectorMode,
    pub stereo_link: f32,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            input_gain_db: 0.0,
            threshold_db: -18.0,
            ratio: 4.0,
            knee_db: 0.0,
            attack_ms: 10.0,
            release_ms: 120.0,
            makeup_gain_db: 0.0,
            mix: 1.0,
            detector_mode: DetectorMode::Peak,
            stereo_link: 1.0,
        }
    }
}

pub struct Compressor {
    sample_rate: f32,
    params: CompressorParams,
    detector: LevelDetector,
    envelope_l: BallisticsFilter,
    envelope_r: BallisticsFilter,
    gain_l: BallisticsFilter,
    gain_r: BallisticsFilter,
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new(CompressorParams::default())
    }
}

impl Compressor {
    pub fn new(params: CompressorParams) -> Self {
        let mut compressor = Self {
            sample_rate: 48_000.0,
            params,
            detector: LevelDetector::default(),
            envelope_l: BallisticsFilter::default(),
            envelope_r: BallisticsFilter::default(),
            gain_l: BallisticsFilter::default(),
            gain_r: BallisticsFilter::default(),
        };
        compressor.prepare(48_000.0, 512);
        compressor
    }

    pub fn set_params(&mut self, params: CompressorParams) {
        self.params = sanitize(params);
        self.configure_filters();
    }

    pub fn params(&self) -> CompressorParams {
        self.params
    }

    fn configure_filters(&mut self) {
        self.detector
            .configure(self.sample_rate, self.params.detector_mode, 25.0);
        self.envelope_l.configure(
            self.sample_rate,
            self.params.attack_ms,
            self.params.release_ms,
        );
        self.envelope_r.configure(
            self.sample_rate,
            self.params.attack_ms,
            self.params.release_ms,
        );
        self.gain_l
            .configure(self.sample_rate, 1.0, self.params.release_ms);
        self.gain_r
            .configure(self.sample_rate, 1.0, self.params.release_ms);
    }
}

impl Effect for Compressor {
    fn prepare(&mut self, sample_rate: f32, _max_block_size: usize) {
        self.sample_rate = sample_rate.max(1.0);
        self.params = sanitize(self.params);
        self.configure_filters();
    }

    fn reset(&mut self) {
        self.detector.reset();
        self.envelope_l.reset();
        self.envelope_r.reset();
        self.gain_l.reset_to(1.0);
        self.gain_r.reset_to(1.0);
    }

    fn process_stereo(&mut self, _ctx: &ProcessContext, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        let input_gain = db_to_linear(self.params.input_gain_db);
        let makeup = db_to_linear(self.params.makeup_gain_db);
        let mix = self.params.mix.clamp(0.0, 1.0);

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let dry_l = *l;
            let dry_r = *r;
            let in_l = dry_l * input_gain;
            let in_r = dry_r * input_gain;

            let (det_l, det_r) = self
                .detector
                .process_stereo(in_l, in_r, self.params.stereo_link);
            let env_l = self.envelope_l.process(det_l);
            let env_r = self.envelope_r.process(det_r);
            let target_l = db_to_linear(compressor_gain_db(
                linear_to_db(env_l),
                self.params.threshold_db,
                self.params.ratio,
                self.params.knee_db,
            ));
            let target_r = db_to_linear(compressor_gain_db(
                linear_to_db(env_r),
                self.params.threshold_db,
                self.params.ratio,
                self.params.knee_db,
            ));
            let gain_l = if target_l < self.gain_l.state() {
                self.gain_l.reset_to(target_l);
                target_l
            } else {
                self.gain_l.process(target_l)
            };
            let gain_r = if target_r < self.gain_r.state() {
                self.gain_r.reset_to(target_r);
                target_r
            } else {
                self.gain_r.process(target_r)
            };

            let wet_l = in_l * gain_l * makeup;
            let wet_r = in_r * gain_r * makeup;
            *l = flush_denormal(dry_l * (1.0 - mix) + wet_l * mix);
            *r = flush_denormal(dry_r * (1.0 - mix) + wet_r * mix);
        }
    }
}

pub fn compressor_gain_db(level_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    let ratio = ratio.max(1.0);
    if ratio <= 1.0 {
        return 0.0;
    }

    let knee = knee_db.max(0.0);
    if knee <= 0.0 {
        if level_db <= threshold_db {
            0.0
        } else {
            threshold_db + (level_db - threshold_db) / ratio - level_db
        }
    } else {
        let x = level_db - threshold_db;
        if x <= -knee * 0.5 {
            0.0
        } else if x >= knee * 0.5 {
            threshold_db + x / ratio - level_db
        } else {
            (1.0 / ratio - 1.0) * (x + knee * 0.5) * (x + knee * 0.5) / (2.0 * knee)
        }
    }
}

fn sanitize(params: CompressorParams) -> CompressorParams {
    CompressorParams {
        input_gain_db: params.input_gain_db.clamp(-24.0, 24.0),
        threshold_db: params.threshold_db.clamp(-60.0, 0.0),
        ratio: params.ratio.clamp(1.0, 20.0),
        knee_db: params.knee_db.clamp(0.0, 24.0),
        attack_ms: params.attack_ms.clamp(0.1, 200.0),
        release_ms: params.release_ms.clamp(5.0, 2000.0),
        makeup_gain_db: params.makeup_gain_db.clamp(-24.0, 24.0),
        mix: params.mix.clamp(0.0, 1.0),
        detector_mode: params.detector_mode,
        stereo_link: params.stereo_link.clamp(0.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(len: usize) -> ProcessContext<'static> {
        ProcessContext::new(48_000.0, len, 120.0, &[])
    }

    #[test]
    fn ratio_one_is_unity() {
        assert_eq!(compressor_gain_db(-6.0, -18.0, 1.0, 0.0), 0.0);
    }

    #[test]
    fn threshold_above_reduces_gain() {
        let gain = compressor_gain_db(-6.0, -18.0, 4.0, 0.0);
        assert!(gain < -8.0 && gain > -10.0, "gain={gain}");
    }

    #[test]
    fn mix_zero_is_dry() {
        let mut comp = Compressor::new(CompressorParams {
            threshold_db: -60.0,
            ratio: 20.0,
            mix: 0.0,
            ..Default::default()
        });
        comp.prepare(48_000.0, 128);
        let mut left = [1.0_f32; 128];
        let mut right = [1.0_f32; 128];
        comp.process_stereo(&ctx(128), &mut left, &mut right);
        assert!(left.iter().all(|s| (*s - 1.0).abs() < 1.0e-6));
    }

    #[test]
    fn above_threshold_is_reduced_and_finite() {
        let mut comp = Compressor::new(CompressorParams {
            threshold_db: -24.0,
            ratio: 8.0,
            attack_ms: 0.1,
            ..Default::default()
        });
        comp.prepare(48_000.0, 128);
        let mut left = [1.0_f32; 512];
        let mut right = [1.0_f32; 512];
        comp.process_stereo(&ctx(512), &mut left, &mut right);
        for s in left.iter().chain(right.iter()) {
            assert!(s.is_finite());
        }
        assert!(left[511] < 0.4, "last={}", left[511]);
    }
}
