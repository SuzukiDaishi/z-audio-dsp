//! Sample-based (VCSL Keys) piano instrument.

use std::sync::Arc;

use z_audio_dsp::{
    EventKind, ParamId, ProcessContext, SamplerEngine, SamplerEngineConfig, TimedEvent,
    TriggerKind, db_to_linear, flush_denormal,
};

use super::bank::VcslSampleBank;
use super::params::VcslPianoParams;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VcslPianoConfig {
    pub sample_rate: f32,
    pub max_block_size: usize,
    pub max_polyphony: usize,
}

impl Default for VcslPianoConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            max_block_size: 512,
            max_polyphony: 32,
        }
    }
}

/// Polyphonic sampler piano driven by a [`VcslSampleBank`] (e.g. VCSL Keys
/// "Grand Piano, K" converted offline via `cargo xtask prepare-vcsl-piano`).
///
/// Until [`VcslPiano::load_bank`] is called, `note_on`/`note_off` are no-ops
/// and `process` emits silence; it never panics or allocates.
pub struct VcslPiano {
    sample_rate: f32,
    engine: SamplerEngine,
    bank: Option<Arc<VcslSampleBank>>,
    params: VcslPianoParams,
    last_velocity: [u8; 128],
    tone_lp: [f32; 2],
}

impl VcslPiano {
    pub fn new(config: VcslPianoConfig) -> Self {
        Self {
            sample_rate: config.sample_rate.max(1.0),
            engine: SamplerEngine::new(SamplerEngineConfig {
                sample_rate: config.sample_rate,
                max_voices: config.max_polyphony.max(1),
            }),
            bank: None,
            params: VcslPianoParams::default(),
            last_velocity: [100; 128],
            tone_lp: [0.0; 2],
        }
    }

    pub fn load_bank(&mut self, bank: Arc<VcslSampleBank>) {
        self.bank = Some(bank);
    }

    pub fn has_bank(&self) -> bool {
        self.bank.is_some()
    }

    pub fn active_voice_count(&self) -> usize {
        self.engine.active_voice_count()
    }

    pub fn set_param(&mut self, id: ParamId, value: f32) {
        let m = id.metadata();
        let clamped = value.clamp(m.min, m.max);
        match id {
            ParamId::VcslMasterGain => self.params.master_gain_db = clamped,
            ParamId::VcslTone => self.params.tone = clamped,
            ParamId::VcslVelocityCurve => self.params.velocity_curve = clamped,
            ParamId::VcslReleaseLevel => self.params.release_level_db = clamped,
            ParamId::VcslReleaseTime => self.params.release_time_s = clamped,
            ParamId::VcslStereoWidth => self.params.stereo_width = clamped,
            _ => {}
        }
    }

    pub fn param_value(&self, id: ParamId) -> f32 {
        match id {
            ParamId::VcslMasterGain => self.params.master_gain_db,
            ParamId::VcslTone => self.params.tone,
            ParamId::VcslVelocityCurve => self.params.velocity_curve,
            ParamId::VcslReleaseLevel => self.params.release_level_db,
            ParamId::VcslReleaseTime => self.params.release_time_s,
            ParamId::VcslStereoWidth => self.params.stereo_width,
            _ => id.metadata().default,
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: f32) {
        let velocity_127 = ((velocity.clamp(0.0, 1.0) * 127.0).round() as u8).max(1);
        self.last_velocity[(note & 0x7f) as usize] = velocity_127;
        let Some(bank) = self.bank.clone() else {
            return;
        };
        let velocity01 = shape_velocity(velocity_127 as f32 / 127.0, self.params.velocity_curve);
        if let Some(region) = bank
            .regions
            .iter()
            .find(|r| r.trigger == TriggerKind::Attack && r.matches(note, velocity_127))
        {
            self.engine.trigger(region.clone(), note, velocity01, 1.0, 1.0);
        }
    }

    pub fn note_off(&mut self, note: u8) {
        self.engine.note_off(note);
        let Some(bank) = self.bank.clone() else {
            return;
        };
        let velocity_127 = self.last_velocity[(note & 0x7f) as usize];
        let velocity01 = shape_velocity(velocity_127 as f32 / 127.0, self.params.velocity_curve);
        let release_gain = db_to_linear(self.params.release_level_db);
        if let Some(region) = bank
            .regions
            .iter()
            .find(|r| r.trigger == TriggerKind::Release && r.matches(note, velocity_127))
        {
            self.engine
                .trigger(region.clone(), note, velocity01, release_gain, 1.0);
        }
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let ctx = ProcessContext::new(self.sample_rate, left.len(), 120.0, &[]);
        self.process_with_context(&ctx, left, right);
    }

    pub fn process_with_context(
        &mut self,
        ctx: &ProcessContext,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        debug_assert_eq!(left.len(), right.len());
        let len = left.len();
        let mut start = 0usize;
        let mut event_index = 0usize;
        while start < len {
            let mut end = len;
            while event_index < ctx.events.len() {
                let offset = ctx.events[event_index].sample_offset.min(len);
                if offset <= start {
                    self.handle_event(ctx.events[event_index]);
                    event_index += 1;
                    continue;
                }
                end = offset;
                break;
            }
            if end > start {
                self.engine.process(&mut left[start..end], &mut right[start..end]);
                self.apply_tone_and_width(&mut left[start..end], &mut right[start..end]);
                let master = db_to_linear(self.params.master_gain_db);
                for sample in start..end {
                    left[sample] *= master;
                    right[sample] *= master;
                }
            }
            start = end;
        }
        while event_index < ctx.events.len() {
            self.handle_event(ctx.events[event_index]);
            event_index += 1;
        }
    }

    fn apply_tone_and_width(&mut self, left: &mut [f32], right: &mut [f32]) {
        let cutoff_hz = 800.0 + self.params.tone.clamp(0.0, 1.0) * (18_000.0 - 800.0);
        let dt = 1.0 / self.sample_rate;
        let rc = 1.0 / (core::f32::consts::TAU * cutoff_hz);
        let alpha = (dt / (rc + dt)).clamp(0.0, 1.0);
        let width = self.params.stereo_width.clamp(0.0, 1.0);
        for i in 0..left.len() {
            self.tone_lp[0] += alpha * (left[i] - self.tone_lp[0]);
            self.tone_lp[1] += alpha * (right[i] - self.tone_lp[1]);
            let l = flush_denormal(self.tone_lp[0]);
            let r = flush_denormal(self.tone_lp[1]);
            let mid = (l + r) * 0.5;
            let side = (l - r) * 0.5 * width;
            left[i] = mid + side;
            right[i] = mid - side;
        }
    }

    fn handle_event(&mut self, event: TimedEvent) {
        match event.kind {
            EventKind::NoteOn { note, velocity } => self.note_on(note, velocity),
            EventKind::NoteOff { note, .. } => self.note_off(note),
            EventKind::Param { id, value } => self.set_param(id, value),
        }
    }
}

fn shape_velocity(velocity01: f32, curve: f32) -> f32 {
    let exponent = 2.0 - curve.clamp(0.0, 1.0) * 2.0;
    velocity01.clamp(0.0, 1.0).powf(exponent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc as StdArc;
    use z_audio_dsp::{SampleBuffer, SampleRegion};

    fn sine_pcm(frames: usize, freq_hz: f32, sample_rate: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| (core::f32::consts::TAU * freq_hz * i as f32 / sample_rate).sin())
            .collect()
    }

    fn one_region_bank(trigger: TriggerKind) -> Arc<VcslSampleBank> {
        let pcm = sine_pcm(48_000 * 2, 440.0, 48_000.0);
        let sample = SampleBuffer::new(48_000.0, 1, pcm);
        let region = StdArc::new(SampleRegion {
            lokey: 0,
            hikey: 127,
            lovel: 0,
            hivel: 127,
            pitch_keycenter: 69,
            tune_cents: 0.0,
            volume_db: 0.0,
            amp_veltrack: 0.8,
            offset_frames: 0,
            trigger,
            ampeg_attack: 0.004,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.4,
            sample,
        });
        Arc::new(VcslSampleBank {
            regions: vec![region],
        })
    }

    #[test]
    fn silent_until_bank_loaded() {
        let mut piano = VcslPiano::new(VcslPianoConfig::default());
        piano.note_on(69, 1.0);
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        piano.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| *s == 0.0));
    }

    #[test]
    fn note_on_produces_signal_after_bank_loaded() {
        let mut piano = VcslPiano::new(VcslPianoConfig::default());
        piano.load_bank(one_region_bank(TriggerKind::Attack));
        piano.note_on(69, 1.0);
        let mut left = [0.0_f32; 512];
        let mut right = [0.0_f32; 512];
        piano.process(&mut left, &mut right);
        assert!(left.iter().any(|s| s.abs() > 0.0));
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    }

    #[test]
    fn note_off_triggers_release_region() {
        let mut piano = VcslPiano::new(VcslPianoConfig::default());
        let mut bank = (*one_region_bank(TriggerKind::Attack)).clone();
        let release = (*one_region_bank(TriggerKind::Release)).clone();
        bank.regions.extend(release.regions);
        piano.load_bank(Arc::new(bank));
        piano.note_on(69, 1.0);
        piano.note_off(69);
        assert!(piano.active_voice_count() >= 1);
    }

    #[test]
    fn master_gain_scales_output() {
        let mut quiet = VcslPiano::new(VcslPianoConfig::default());
        let mut loud = VcslPiano::new(VcslPianoConfig::default());
        quiet.load_bank(one_region_bank(TriggerKind::Attack));
        loud.load_bank(one_region_bank(TriggerKind::Attack));
        quiet.set_param(ParamId::VcslMasterGain, -24.0);
        loud.set_param(ParamId::VcslMasterGain, 0.0);
        quiet.note_on(69, 1.0);
        loud.note_on(69, 1.0);
        let mut ql = [0.0_f32; 1024];
        let mut qr = [0.0_f32; 1024];
        let mut ll = [0.0_f32; 1024];
        let mut lr = [0.0_f32; 1024];
        quiet.process(&mut ql, &mut qr);
        loud.process(&mut ll, &mut lr);
        let quiet_rms = (ql.iter().map(|s| s * s).sum::<f32>() / ql.len() as f32).sqrt();
        let loud_rms = (ll.iter().map(|s| s * s).sum::<f32>() / ll.len() as f32).sqrt();
        assert!(loud_rms > quiet_rms * 4.0, "quiet={quiet_rms}, loud={loud_rms}");
    }
}
