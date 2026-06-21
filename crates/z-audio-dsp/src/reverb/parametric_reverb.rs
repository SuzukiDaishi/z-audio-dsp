//! Stereo parametric reverb using early taps, allpass diffusion, and an
//! 8-line feedback delay network.

use crate::Effect;
use crate::context::ProcessContext;
use crate::delay::{AllpassDelay, DelayLine};
use crate::math::{db_to_linear, flush_denormal, lerp};

const FDN_SIZE: usize = 8;
const FDN_BASE_MS: [f32; FDN_SIZE] = [29.7, 37.1, 41.1, 43.7, 53.9, 61.1, 67.7, 71.9];
const EARLY_TAPS: [(f32, f32, f32, f32); 6] = [
    (6.1, 7.3, 0.35, -0.20),
    (11.7, 13.9, -0.25, 0.30),
    (17.9, 19.3, 0.22, 0.18),
    (23.1, 29.9, -0.18, 0.22),
    (31.7, 37.1, 0.14, -0.14),
    (41.3, 43.9, 0.10, 0.12),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParametricReverbParams {
    pub mix: f32,
    pub room_size: f32,
    pub decay_time_sec: f32,
    pub pre_delay_ms: f32,
    pub diffusion: f32,
    pub damping: f32,
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    pub modulation_rate_hz: f32,
    pub modulation_depth: f32,
    pub width: f32,
    pub early_late_mix: f32,
    pub output_gain_db: f32,
}

impl Default for ParametricReverbParams {
    fn default() -> Self {
        Self {
            mix: 0.35,
            room_size: 0.55,
            decay_time_sec: 2.2,
            pre_delay_ms: 18.0,
            diffusion: 0.65,
            damping: 0.35,
            low_cut_hz: 80.0,
            high_cut_hz: 12_000.0,
            modulation_rate_hz: 0.0,
            modulation_depth: 0.0,
            width: 0.9,
            early_late_mix: 0.35,
            output_gain_db: 0.0,
        }
    }
}

pub struct ParametricReverb {
    sample_rate: f32,
    params: ParametricReverbParams,
    predelay: DelayLine,
    early_l: DelayLine,
    early_r: DelayLine,
    diffuser_l: [AllpassDelay; 4],
    diffuser_r: [AllpassDelay; 4],
    fdn: FdnReverb,
}

impl Default for ParametricReverb {
    fn default() -> Self {
        Self::new(ParametricReverbParams::default())
    }
}

impl ParametricReverb {
    pub fn new(params: ParametricReverbParams) -> Self {
        let mut reverb = Self {
            sample_rate: 48_000.0,
            params: sanitize(params),
            predelay: DelayLine::new(),
            early_l: DelayLine::new(),
            early_r: DelayLine::new(),
            diffuser_l: core::array::from_fn(|_| AllpassDelay::new()),
            diffuser_r: core::array::from_fn(|_| AllpassDelay::new()),
            fdn: FdnReverb::default(),
        };
        reverb.prepare(48_000.0, 512);
        reverb
    }

    pub fn set_params(&mut self, params: ParametricReverbParams) {
        self.params = sanitize(params);
    }

    pub fn params(&self) -> ParametricReverbParams {
        self.params
    }
}

impl Effect for ParametricReverb {
    fn prepare(&mut self, sample_rate: f32, _max_block_size: usize) {
        self.sample_rate = sample_rate.max(1.0);
        let max_predelay = (self.sample_rate * 0.3).ceil() as usize;
        let max_early = (self.sample_rate * 0.25).ceil() as usize;
        self.predelay.prepare(max_predelay);
        self.early_l.prepare(max_early);
        self.early_r.prepare(max_early);
        for (i, (l, r)) in self
            .diffuser_l
            .iter_mut()
            .zip(self.diffuser_r.iter_mut())
            .enumerate()
        {
            let base = [4.7, 6.9, 9.1, 12.3][i];
            l.prepare(ms_to_samples(base, self.sample_rate) + 4);
            r.prepare(ms_to_samples(base * 1.17, self.sample_rate) + 4);
        }
        self.fdn.prepare(self.sample_rate);
    }

    fn reset(&mut self) {
        self.predelay.clear();
        self.early_l.clear();
        self.early_r.clear();
        for delay in &mut self.diffuser_l {
            delay.clear();
        }
        for delay in &mut self.diffuser_r {
            delay.clear();
        }
        self.fdn.clear();
    }

    fn process_stereo(&mut self, _ctx: &ProcessContext, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        let params = self.params;
        let room_scale = lerp(0.55, 2.2, params.room_size);
        let pre_delay = ms_to_samples(params.pre_delay_ms, self.sample_rate);
        let diffusion_gain = lerp(0.35, 0.75, params.diffusion);
        let output_gain = db_to_linear(params.output_gain_db);

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let dry_l = *l;
            let dry_r = *r;
            let mono = (dry_l + dry_r) * 0.5;
            let pre = self.predelay.process_int(mono.tanh(), pre_delay);

            let early = early_reflections(
                &mut self.early_l,
                &mut self.early_r,
                pre,
                self.sample_rate,
                room_scale,
            );

            let mut diff_l = pre;
            let mut diff_r = pre;
            for (i, (dl, dr)) in self
                .diffuser_l
                .iter_mut()
                .zip(self.diffuser_r.iter_mut())
                .enumerate()
            {
                let base = [4.7, 6.9, 9.1, 12.3][i] * room_scale;
                diff_l = dl.process(
                    diff_l,
                    ms_to_samples(base, self.sample_rate),
                    diffusion_gain,
                );
                diff_r = dr.process(
                    diff_r,
                    ms_to_samples(base * 1.17, self.sample_rate),
                    diffusion_gain,
                );
            }

            let late = self.fdn.process(
                (diff_l + diff_r) * 0.5,
                self.sample_rate,
                params.decay_time_sec,
                params.room_size,
                params.damping,
                params.width,
            );

            let wet_l = early.0 * params.early_late_mix + late.0 * (1.0 - params.early_late_mix);
            let wet_r = early.1 * params.early_late_mix + late.1 * (1.0 - params.early_late_mix);
            let mix = params.mix;
            *l = flush_denormal((dry_l * (1.0 - mix) + wet_l * mix) * output_gain);
            *r = flush_denormal((dry_r * (1.0 - mix) + wet_r * mix) * output_gain);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct OnePoleLowpass {
    z: f32,
}

impl OnePoleLowpass {
    fn process(&mut self, input: f32, damping: f32, sample_rate: f32) -> f32 {
        let cutoff = lerp(18_000.0, 1800.0, damping).min(sample_rate * 0.45);
        let a = 1.0 - (-2.0 * core::f32::consts::PI * cutoff / sample_rate).exp();
        self.z += a * (input - self.z);
        flush_denormal(self.z)
    }

    fn clear(&mut self) {
        self.z = 0.0;
    }
}

#[derive(Debug, Clone)]
struct FdnReverb {
    delays: [DelayLine; FDN_SIZE],
    filters: [OnePoleLowpass; FDN_SIZE],
}

impl Default for FdnReverb {
    fn default() -> Self {
        Self {
            delays: core::array::from_fn(|_| DelayLine::new()),
            filters: [OnePoleLowpass { z: 0.0 }; FDN_SIZE],
        }
    }
}

impl FdnReverb {
    fn prepare(&mut self, sample_rate: f32) {
        for (i, delay) in self.delays.iter_mut().enumerate() {
            let max_delay = ms_to_samples(FDN_BASE_MS[i] * 2.8, sample_rate) + 8;
            delay.prepare(max_delay);
        }
    }

    fn clear(&mut self) {
        for delay in &mut self.delays {
            delay.clear();
        }
        for filter in &mut self.filters {
            filter.clear();
        }
    }

    fn process(
        &mut self,
        input: f32,
        sample_rate: f32,
        decay_time_sec: f32,
        room_size: f32,
        damping: f32,
        width: f32,
    ) -> (f32, f32) {
        let scale = lerp(0.65, 2.25, room_size);
        let mut v = [0.0_f32; FDN_SIZE];
        for i in 0..FDN_SIZE {
            let delay_samples = ms_to_samples(FDN_BASE_MS[i] * scale, sample_rate);
            v[i] = self.delays[i].read_int(delay_samples.max(1));
            v[i] = self.filters[i].process(v[i], damping, sample_rate);
        }

        hadamard8(&mut v);

        for i in 0..FDN_SIZE {
            let delay_sec = FDN_BASE_MS[i] * scale * 0.001;
            let feedback = 10.0_f32
                .powf(-3.0 * delay_sec / decay_time_sec.max(0.1))
                .clamp(0.0, 0.995);
            let polarity = if i & 1 == 0 { 1.0 } else { -1.0 };
            self.delays[i].push(input * 0.22 * polarity + v[i] * feedback);
        }

        let left = v[0] + v[2] - v[5] + v[7];
        let right = v[1] - v[3] + v[4] + v[6];
        let mid = (left + right) * 0.5;
        let side = (left - right) * 0.5 * width.clamp(0.0, 1.0);
        ((mid + side) * 0.35, (mid - side) * 0.35)
    }
}

fn early_reflections(
    early_l: &mut DelayLine,
    early_r: &mut DelayLine,
    input: f32,
    sample_rate: f32,
    room_scale: f32,
) -> (f32, f32) {
    let mut l = 0.0;
    let mut r = 0.0;
    early_l.push(input);
    early_r.push(input);
    for (delay_l_ms, delay_r_ms, gain_l, gain_r) in EARLY_TAPS {
        l += early_l.read_int(ms_to_samples(delay_l_ms * room_scale, sample_rate)) * gain_l;
        r += early_r.read_int(ms_to_samples(delay_r_ms * room_scale, sample_rate)) * gain_r;
    }
    (l, r)
}

fn hadamard8(x: &mut [f32; FDN_SIZE]) {
    let mut h = 1;
    while h < FDN_SIZE {
        let step = h * 2;
        let mut i = 0;
        while i < FDN_SIZE {
            for j in 0..h {
                let a = x[i + j];
                let b = x[i + j + h];
                x[i + j] = a + b;
                x[i + j + h] = a - b;
            }
            i += step;
        }
        h *= 2;
    }
    let scale = 1.0 / (FDN_SIZE as f32).sqrt();
    for v in x {
        *v *= scale;
    }
}

fn ms_to_samples(ms: f32, sample_rate: f32) -> usize {
    (ms.max(0.0) * 0.001 * sample_rate.max(1.0)).round() as usize
}

fn sanitize(params: ParametricReverbParams) -> ParametricReverbParams {
    ParametricReverbParams {
        mix: params.mix.clamp(0.0, 1.0),
        room_size: params.room_size.clamp(0.0, 1.0),
        decay_time_sec: params.decay_time_sec.clamp(0.1, 20.0),
        pre_delay_ms: params.pre_delay_ms.clamp(0.0, 250.0),
        diffusion: params.diffusion.clamp(0.0, 1.0),
        damping: params.damping.clamp(0.0, 1.0),
        low_cut_hz: params.low_cut_hz.clamp(20.0, 1000.0),
        high_cut_hz: params.high_cut_hz.clamp(1000.0, 20_000.0),
        modulation_rate_hz: params.modulation_rate_hz.clamp(0.0, 2.0),
        modulation_depth: params.modulation_depth.clamp(0.0, 1.0),
        width: params.width.clamp(0.0, 1.0),
        early_late_mix: params.early_late_mix.clamp(0.0, 1.0),
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
    fn mix_zero_is_dry() {
        let mut reverb = ParametricReverb::new(ParametricReverbParams {
            mix: 0.0,
            ..Default::default()
        });
        reverb.prepare(48_000.0, 128);
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        left[0] = 1.0;
        right[0] = -0.5;
        let dry_l = left;
        let dry_r = right;
        reverb.process_stereo(&ctx(128), &mut left, &mut right);
        assert_eq!(left, dry_l);
        assert_eq!(right, dry_r);
    }

    #[test]
    fn impulse_produces_tail() {
        let mut reverb = ParametricReverb::default();
        reverb.prepare(48_000.0, 128);
        let mut total = 0.0;
        for block in 0..200 {
            let mut left = [0.0_f32; 128];
            let mut right = [0.0_f32; 128];
            if block == 0 {
                left[0] = 1.0;
                right[0] = 1.0;
            }
            reverb.process_stereo(&ctx(128), &mut left, &mut right);
            total += left.iter().map(|s| s.abs()).sum::<f32>();
            total += right.iter().map(|s| s.abs()).sum::<f32>();
        }
        assert!(total > 1.0, "total={total}");
    }

    #[test]
    fn reset_clears_tail() {
        let mut reverb = ParametricReverb::default();
        reverb.prepare(48_000.0, 128);
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        left[0] = 1.0;
        right[0] = 1.0;
        reverb.process_stereo(&ctx(128), &mut left, &mut right);
        reverb.reset();
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        reverb.process_stereo(&ctx(128), &mut left, &mut right);
        assert!(left.iter().all(|s| s.abs() < 1.0e-6));
        assert!(right.iter().all(|s| s.abs() < 1.0e-6));
    }
}
