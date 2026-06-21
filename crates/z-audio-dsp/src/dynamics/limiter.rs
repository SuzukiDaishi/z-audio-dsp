//! Lookahead peak limiter.

use crate::Effect;
use crate::context::ProcessContext;
use crate::delay::DelayLine;
use crate::math::{db_to_linear, flush_denormal, linear_to_db};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimiterParams {
    pub input_gain_db: f32,
    pub ceiling_db: f32,
    pub threshold_db: f32,
    pub release_ms: f32,
    pub lookahead_ms: f32,
    pub stereo_link: f32,
    pub true_peak: bool,
    pub output_gain_db: f32,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            input_gain_db: 0.0,
            ceiling_db: -0.1,
            threshold_db: -0.1,
            release_ms: 80.0,
            lookahead_ms: 3.0,
            stereo_link: 1.0,
            true_peak: false,
            output_gain_db: 0.0,
        }
    }
}

pub struct Limiter {
    sample_rate: f32,
    params: LimiterParams,
    lookahead_l: DelayLine,
    lookahead_r: DelayLine,
    current_gain: f32,
    release_coeff: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new(LimiterParams::default())
    }
}

impl Limiter {
    pub fn new(params: LimiterParams) -> Self {
        let mut limiter = Self {
            sample_rate: 48_000.0,
            params: sanitize(params),
            lookahead_l: DelayLine::new(),
            lookahead_r: DelayLine::new(),
            current_gain: 1.0,
            release_coeff: 0.0,
        };
        limiter.prepare(48_000.0, 512);
        limiter
    }

    pub fn set_params(&mut self, params: LimiterParams) {
        self.params = sanitize(params);
        self.release_coeff = release_coeff(self.sample_rate, self.params.release_ms);
    }

    pub fn params(&self) -> LimiterParams {
        self.params
    }

    fn lookahead_samples(&self) -> usize {
        ((self.params.lookahead_ms * 0.001 * self.sample_rate).round() as usize)
            .min((self.sample_rate * 0.02).round() as usize)
    }
}

impl Effect for Limiter {
    fn prepare(&mut self, sample_rate: f32, _max_block_size: usize) {
        self.sample_rate = sample_rate.max(1.0);
        let max_lookahead = (self.sample_rate * 0.02).ceil() as usize;
        self.lookahead_l.prepare(max_lookahead);
        self.lookahead_r.prepare(max_lookahead);
        self.release_coeff = release_coeff(self.sample_rate, self.params.release_ms);
    }

    fn reset(&mut self) {
        self.lookahead_l.clear();
        self.lookahead_r.clear();
        self.current_gain = 1.0;
    }

    fn process_stereo(&mut self, _ctx: &ProcessContext, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        let input_gain = db_to_linear(self.params.input_gain_db);
        let output_gain = db_to_linear(self.params.output_gain_db);
        let ceiling = db_to_linear(self.params.ceiling_db);
        let lookahead = self.lookahead_samples();

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let in_l = *l * input_gain;
            let in_r = *r * input_gain;
            let linked_peak = in_l.abs().max(in_r.abs());
            let level_db = linear_to_db(linked_peak);
            let allowed_db = self.params.threshold_db.min(self.params.ceiling_db);
            let target = db_to_linear(limiter_gain_db(level_db, allowed_db));

            if target < self.current_gain {
                self.current_gain = target;
            } else {
                self.current_gain =
                    self.release_coeff * self.current_gain + (1.0 - self.release_coeff) * target;
            }

            let delayed_l = self.lookahead_l.process_int(in_l, lookahead);
            let delayed_r = self.lookahead_r.process_int(in_r, lookahead);
            let out_l = delayed_l * self.current_gain * output_gain;
            let out_r = delayed_r * self.current_gain * output_gain;
            *l = flush_denormal(out_l.clamp(-ceiling, ceiling));
            *r = flush_denormal(out_r.clamp(-ceiling, ceiling));
        }
    }
}

pub fn limiter_gain_db(level_db: f32, ceiling_db: f32) -> f32 {
    let excess = level_db - ceiling_db;
    if excess > 0.0 { -excess } else { 0.0 }
}

fn release_coeff(sample_rate: f32, release_ms: f32) -> f32 {
    (-1.0 / (release_ms.max(1.0) * 0.001 * sample_rate.max(1.0))).exp()
}

fn sanitize(params: LimiterParams) -> LimiterParams {
    LimiterParams {
        input_gain_db: params.input_gain_db.clamp(-24.0, 24.0),
        ceiling_db: params.ceiling_db.clamp(-24.0, 0.0),
        threshold_db: params.threshold_db.clamp(-24.0, 0.0),
        release_ms: params.release_ms.clamp(1.0, 1000.0),
        lookahead_ms: params.lookahead_ms.clamp(0.0, 10.0),
        stereo_link: params.stereo_link.clamp(0.0, 1.0),
        true_peak: params.true_peak,
        output_gain_db: params.output_gain_db.clamp(-24.0, 24.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(len: usize) -> ProcessContext<'static> {
        ProcessContext::new(48_000.0, len, 120.0, &[])
    }

    #[test]
    fn gain_computer_reduces_above_ceiling() {
        assert_eq!(limiter_gain_db(-6.0, 0.0), 0.0);
        assert!((limiter_gain_db(6.0, 0.0) + 6.0).abs() < 1.0e-6);
    }

    #[test]
    fn output_does_not_exceed_ceiling() {
        let mut limiter = Limiter::new(LimiterParams {
            ceiling_db: -6.0,
            threshold_db: -6.0,
            lookahead_ms: 0.0,
            release_ms: 1.0,
            ..Default::default()
        });
        limiter.prepare(48_000.0, 128);
        let mut left = [4.0_f32; 256];
        let mut right = [4.0_f32; 256];
        limiter.process_stereo(&ctx(256), &mut left, &mut right);
        let ceiling = db_to_linear(-6.0) + 1.0e-6;
        for s in left.iter().chain(right.iter()) {
            assert!(s.abs() <= ceiling, "sample={s}");
            assert!(s.is_finite());
        }
    }

    #[test]
    fn reset_clears_lookahead_tail() {
        let mut limiter = Limiter::default();
        limiter.prepare(48_000.0, 128);
        let mut left = [1.0_f32; 128];
        let mut right = [1.0_f32; 128];
        limiter.process_stereo(&ctx(128), &mut left, &mut right);
        limiter.reset();
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        limiter.process_stereo(&ctx(128), &mut left, &mut right);
        assert!(left.iter().all(|s| *s == 0.0));
    }
}
