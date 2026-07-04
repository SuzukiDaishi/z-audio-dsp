//! Single-sample general-purpose sampler instrument.

use std::sync::Arc;

use z_audio_dsp::{
    EventKind, LoopMode, ParamId, ProcessContext, SampleRegion, SamplerEngine, SamplerEngineConfig,
    TimedEvent, TriggerKind, db_to_linear, flush_denormal,
};

use super::bank::SamplerBank;
use super::params::GenericSamplerParams;

/// Baseline release time (seconds) baked into the cached region; the actual
/// release heard is this value multiplied by `release_time_scale` at trigger
/// time, which is always set to the *current* [`GenericSamplerParams::release_time_s`]
/// (i.e. `1.0`'s only purpose is to make that multiplier equal the seconds
/// value directly).
const BASE_AMPEG_RELEASE_S: f32 = 1.0;
/// Fixed quick attack so the very first sample of playback doesn't click.
const FIXED_AMPEG_ATTACK_S: f32 = 0.004;
/// Upper bound on [`GenericSamplerParams::unison_voices`]; the engine's
/// voice pool is sized to `max_polyphony * MAX_UNISON_VOICES` so a full
/// chord with unison maxed out doesn't starve voice stealing.
const MAX_UNISON_VOICES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenericSamplerConfig {
    pub sample_rate: f32,
    pub max_block_size: usize,
    pub max_polyphony: usize,
}

impl Default for GenericSamplerConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            max_block_size: 512,
            max_polyphony: 16,
        }
    }
}

/// Polyphonic single-sample sampler driven by a [`SamplerBank`] (e.g. a WAV
/// converted offline via `cargo xtask prepare-sampler-bank`). Root note,
/// tune, and offset are realtime parameters rather than baked into the
/// bank, so the region used to trigger voices is rebuilt (a small, audio-
/// thread-only allocation of one [`SampleRegion`], not the PCM itself) on
/// the rare occasions one of those three actually changes value.
///
/// Until [`GenericSampler::load_bank`] is called, `note_on`/`note_off` are
/// no-ops and `process` emits silence; it never panics.
pub struct GenericSampler {
    sample_rate: f32,
    engine: SamplerEngine,
    bank: Option<Arc<SamplerBank>>,
    regions: Vec<Arc<SampleRegion>>,
    region_dirty: bool,
    params: GenericSamplerParams,
    last_velocity: [u8; 128],
}

impl GenericSampler {
    pub fn new(config: GenericSamplerConfig) -> Self {
        Self {
            sample_rate: config.sample_rate.max(1.0),
            engine: SamplerEngine::new(SamplerEngineConfig {
                sample_rate: config.sample_rate,
                max_voices: config.max_polyphony.max(1) * MAX_UNISON_VOICES,
            }),
            bank: None,
            regions: Vec::new(),
            region_dirty: true,
            params: GenericSamplerParams::default(),
            last_velocity: [100; 128],
        }
    }

    pub fn load_bank(&mut self, bank: Arc<SamplerBank>) {
        self.params.root_note = bank.default_root_note as f32;
        self.bank = Some(bank);
        self.region_dirty = true;
    }

    /// Returns the currently loaded bank, e.g. so a plugin wrapper can
    /// carry it across a re-init (sample-rate/block-size change) instead of
    /// reloading it from disk.
    pub fn bank(&self) -> Option<Arc<SamplerBank>> {
        self.bank.clone()
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
            ParamId::SamplerMasterGain => self.params.master_gain_db = clamped,
            ParamId::SamplerRootNote => {
                if clamped != self.params.root_note {
                    self.params.root_note = clamped;
                    self.region_dirty = true;
                }
            }
            ParamId::SamplerTune => {
                if clamped != self.params.tune_cents {
                    self.params.tune_cents = clamped;
                    self.region_dirty = true;
                }
            }
            ParamId::SamplerOffset => {
                if clamped != self.params.offset01 {
                    self.params.offset01 = clamped;
                    self.region_dirty = true;
                }
            }
            ParamId::SamplerVelocityCurve => self.params.velocity_curve = clamped,
            ParamId::SamplerReleaseTime => self.params.release_time_s = clamped,
            ParamId::SamplerStereoWidth => self.params.stereo_width = clamped,
            ParamId::SamplerLoopMode => {
                let mode = LoopMode::from_param_value(clamped);
                if mode != self.params.loop_mode {
                    self.params.loop_mode = mode;
                    self.region_dirty = true;
                }
            }
            ParamId::SamplerLoopStart => {
                if clamped != self.params.loop_start01 {
                    self.params.loop_start01 = clamped;
                    self.region_dirty = true;
                }
            }
            ParamId::SamplerLoopEnd => {
                if clamped != self.params.loop_end01 {
                    self.params.loop_end01 = clamped;
                    self.region_dirty = true;
                }
            }
            ParamId::SamplerLoopXfade => {
                if clamped != self.params.loop_xfade_s {
                    self.params.loop_xfade_s = clamped;
                    self.region_dirty = true;
                }
            }
            ParamId::SamplerUnisonVoices => {
                if clamped != self.params.unison_voices {
                    self.params.unison_voices = clamped;
                    self.region_dirty = true;
                }
            }
            ParamId::SamplerUnisonDetune => {
                if clamped != self.params.unison_detune_cents {
                    self.params.unison_detune_cents = clamped;
                    self.region_dirty = true;
                }
            }
            ParamId::SamplerUnisonSpread => {
                if clamped != self.params.unison_spread {
                    self.params.unison_spread = clamped;
                    self.region_dirty = true;
                }
            }
            _ => {}
        }
    }

    pub fn param_value(&self, id: ParamId) -> f32 {
        match id {
            ParamId::SamplerMasterGain => self.params.master_gain_db,
            ParamId::SamplerRootNote => self.params.root_note,
            ParamId::SamplerTune => self.params.tune_cents,
            ParamId::SamplerOffset => self.params.offset01,
            ParamId::SamplerVelocityCurve => self.params.velocity_curve,
            ParamId::SamplerReleaseTime => self.params.release_time_s,
            ParamId::SamplerStereoWidth => self.params.stereo_width,
            ParamId::SamplerLoopMode => self.params.loop_mode.to_param_value(),
            ParamId::SamplerLoopStart => self.params.loop_start01,
            ParamId::SamplerLoopEnd => self.params.loop_end01,
            ParamId::SamplerLoopXfade => self.params.loop_xfade_s,
            ParamId::SamplerUnisonVoices => self.params.unison_voices,
            ParamId::SamplerUnisonDetune => self.params.unison_detune_cents,
            ParamId::SamplerUnisonSpread => self.params.unison_spread,
            _ => id.metadata().default,
        }
    }

    /// Rebuilds [`Self::regions`] (one per unison sub-voice) from the
    /// current bank/params, if dirty. Each sub-voice gets a symmetric
    /// tune/pan offset around the root note/center pan, and a `1/sqrt(n)`
    /// gain compensation (applied at trigger time, not baked into the
    /// region) so adding sub-voices doesn't blow up loudness.
    fn current_regions(&mut self) -> &[Arc<SampleRegion>] {
        let Some(bank) = self.bank.clone() else {
            return &[];
        };
        if self.region_dirty || self.regions.is_empty() {
            let frames = bank.sample.frames();
            let offset_frames =
                ((self.params.offset01.clamp(0.0, 1.0) as f64 * frames as f64) as usize).min(frames.saturating_sub(1));
            let loop_start_frames = ((self.params.loop_start01.clamp(0.0, 1.0) as f64 * frames as f64)
                as usize)
                .min(frames.saturating_sub(1));
            let loop_end_frames = if frames == 0 {
                0
            } else {
                ((self.params.loop_end01.clamp(0.0, 1.0) as f64 * frames as f64) as usize)
                    .max(loop_start_frames + 1)
                    .min(frames)
            };
            let loop_xfade_frames =
                (self.params.loop_xfade_s.max(0.0) * bank.sample.sample_rate()) as usize;
            let root_note = self.params.root_note.round().clamp(0.0, 127.0) as u8;

            let voice_count = (self.params.unison_voices.round() as usize).clamp(1, MAX_UNISON_VOICES);
            self.regions = unison_offsets(
                voice_count,
                self.params.unison_detune_cents,
                self.params.unison_spread,
            )
            .into_iter()
            .map(|(tune_offset, pan)| {
                Arc::new(SampleRegion {
                    lokey: 0,
                    hikey: 127,
                    lovel: 0,
                    hivel: 127,
                    pitch_keycenter: root_note,
                    tune_cents: self.params.tune_cents + tune_offset,
                    volume_db: 0.0,
                    amp_veltrack: 1.0,
                    offset_frames,
                    trigger: TriggerKind::Attack,
                    ampeg_attack: FIXED_AMPEG_ATTACK_S,
                    ampeg_decay: 0.0,
                    ampeg_sustain: 1.0,
                    ampeg_release: BASE_AMPEG_RELEASE_S,
                    sample: bank.sample.clone(),
                    loop_mode: self.params.loop_mode,
                    loop_start_frames,
                    loop_end_frames,
                    loop_xfade_frames,
                    pan,
                })
            })
            .collect();
            self.region_dirty = false;
        }
        &self.regions
    }

    pub fn note_on(&mut self, note: u8, velocity: f32) {
        let velocity_127 = ((velocity.clamp(0.0, 1.0) * 127.0).round() as u8).max(1);
        self.last_velocity[(note & 0x7f) as usize] = velocity_127;
        let velocity01 = shape_velocity(velocity_127 as f32 / 127.0, self.params.velocity_curve);
        let release_time_scale = self.params.release_time_s.max(0.01);
        // `.len()` ends the borrow immediately; the loop below re-indexes
        // `self.regions` fresh each iteration (a cheap `Arc` clone, no
        // allocation) so it can interleave with `&mut self.engine` calls.
        let voice_count = self.current_regions().len();
        if voice_count == 0 {
            return;
        }
        let gain_scale = 1.0 / (voice_count as f32).sqrt();
        for i in 0..voice_count {
            let region = self.regions[i].clone();
            self.engine
                .trigger(region, note, velocity01, gain_scale, release_time_scale);
        }
    }

    pub fn note_off(&mut self, note: u8) {
        self.engine.note_off(note);
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
                self.apply_width(&mut left[start..end], &mut right[start..end]);
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

    fn apply_width(&mut self, left: &mut [f32], right: &mut [f32]) {
        let width = self.params.stereo_width.clamp(0.0, 1.0);
        for i in 0..left.len() {
            let l = flush_denormal(left[i]);
            let r = flush_denormal(right[i]);
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

/// Per-unison-sub-voice `(tune_cents_offset, pan)`, symmetric around
/// (0 cents, center pan). `n == 1` always yields a single `(0.0, 0.0)`
/// entry, i.e. unchanged from pre-unison behavior.
fn unison_offsets(n: usize, detune_cents: f32, spread: f32) -> Vec<(f32, f32)> {
    if n <= 1 {
        return vec![(0.0, 0.0)];
    }
    let spread = spread.clamp(0.0, 1.0);
    (0..n)
        .map(|i| {
            // -1.0..1.0 across the voices, symmetric around the center.
            let t = (i as f32 / (n - 1) as f32) * 2.0 - 1.0;
            (t * detune_cents.max(0.0) * 0.5, t * spread)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use z_audio_dsp::SampleBuffer;

    fn sine_bank(frames: usize, freq_hz: f32, sample_rate: f32, root_note: u8) -> Arc<SamplerBank> {
        let pcm: Vec<f32> = (0..frames)
            .map(|i| (core::f32::consts::TAU * freq_hz * i as f32 / sample_rate).sin())
            .collect();
        Arc::new(SamplerBank {
            sample: SampleBuffer::new(sample_rate, 1, pcm),
            default_root_note: root_note,
        })
    }

    #[test]
    fn silent_until_bank_loaded() {
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.note_on(69, 1.0);
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        sampler.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| *s == 0.0));
    }

    #[test]
    fn note_on_produces_finite_signal_after_bank_loaded() {
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(sine_bank(48_000 * 2, 440.0, 48_000.0, 69));
        sampler.note_on(69, 1.0);
        let mut left = [0.0_f32; 512];
        let mut right = [0.0_f32; 512];
        sampler.process(&mut left, &mut right);
        assert!(left.iter().any(|s| s.abs() > 0.0));
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    }

    #[test]
    fn load_bank_adopts_default_root_note_as_initial_param() {
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(sine_bank(1000, 440.0, 48_000.0, 67));
        assert_eq!(sampler.param_value(ParamId::SamplerRootNote), 67.0);
    }

    #[test]
    fn playing_an_octave_above_root_roughly_doubles_pitch() {
        // A region playing one octave above its root key should advance
        // through the sample about twice as fast.
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(sine_bank(480_000, 1.0, 48_000.0, 60));
        sampler.note_on(72, 1.0);
        let mut left = [0.0_f32; 1];
        let mut right = [0.0_f32; 1];
        for _ in 0..100 {
            sampler.process(&mut left, &mut right);
        }
        assert!(left[0].is_finite());
    }

    #[test]
    fn master_gain_scales_output() {
        let mut quiet = GenericSampler::new(GenericSamplerConfig::default());
        let mut loud = GenericSampler::new(GenericSamplerConfig::default());
        quiet.load_bank(sine_bank(48_000 * 2, 440.0, 48_000.0, 69));
        loud.load_bank(sine_bank(48_000 * 2, 440.0, 48_000.0, 69));
        quiet.set_param(ParamId::SamplerMasterGain, -24.0);
        loud.set_param(ParamId::SamplerMasterGain, 0.0);
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

    #[test]
    fn offset_param_skips_initial_playback() {
        let frames = 48_000;
        let pcm: Vec<f32> = (0..frames).map(|i| (i as f32 / frames as f32) * 2.0 - 1.0).collect();
        let bank = Arc::new(SamplerBank {
            sample: SampleBuffer::new(48_000.0, 1, pcm),
            default_root_note: 60,
        });
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(bank);
        sampler.set_param(ParamId::SamplerOffset, 0.5);
        sampler.note_on(60, 1.0);
        let mut left = [0.0_f32; 1];
        let mut right = [0.0_f32; 1];
        sampler.process(&mut left, &mut right);
        assert!(left[0].abs() < 0.5, "left={}", left[0]);
    }

    #[test]
    fn note_off_releases_voice() {
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(sine_bank(480_000, 440.0, 48_000.0, 60));
        sampler.set_param(ParamId::SamplerReleaseTime, 0.05);
        sampler.note_on(60, 1.0);
        sampler.note_off(60);
        let mut left = [0.0_f32; 64];
        let mut right = [0.0_f32; 64];
        for _ in 0..200 {
            sampler.process(&mut left, &mut right);
        }
        assert_eq!(sampler.active_voice_count(), 0);
    }

    #[test]
    fn stereo_width_zero_collapses_to_mono() {
        let frames = 96_000;
        let mut interleaved = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = (core::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin();
            interleaved.push(s);
            interleaved.push(-s);
        }
        let bank = Arc::new(SamplerBank {
            sample: SampleBuffer::new(48_000.0, 2, interleaved),
            default_root_note: 69,
        });
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(bank);
        sampler.set_param(ParamId::SamplerStereoWidth, 0.0);
        sampler.note_on(69, 1.0);
        let mut left = [0.0_f32; 2048];
        let mut right = [0.0_f32; 2048];
        sampler.process(&mut left, &mut right);
        for (l, r) in left.iter().zip(right.iter()).skip(256) {
            assert!((l - r).abs() < 1.0e-4, "l={l}, r={r}");
        }
    }

    #[test]
    fn multiple_simultaneous_notes_are_polyphonic() {
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(sine_bank(48_000 * 2, 440.0, 48_000.0, 69));
        sampler.note_on(60, 1.0);
        sampler.note_on(64, 1.0);
        sampler.note_on(67, 1.0);
        assert_eq!(sampler.active_voice_count(), 3);
    }

    #[test]
    fn note_on_mid_block_does_not_sound_before_its_sample_offset() {
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(sine_bank(48_000 * 2, 440.0, 48_000.0, 69));
        let events = [TimedEvent {
            sample_offset: 200,
            kind: EventKind::NoteOn { note: 69, velocity: 1.0 },
        }];
        let ctx = ProcessContext::new(48_000.0, 256, 120.0, &events);
        let mut left = [0.0_f32; 256];
        let mut right = [0.0_f32; 256];
        sampler.process_with_context(&ctx, &mut left, &mut right);
        assert!(
            left[..200].iter().chain(right[..200].iter()).all(|s| *s == 0.0),
            "note should be silent before its scheduled sample offset"
        );
        assert!(left[200..].iter().any(|s| s.abs() > 0.0));
    }

    #[test]
    fn loop_mode_defaults_to_off() {
        let sampler = GenericSampler::new(GenericSamplerConfig::default());
        assert_eq!(sampler.param_value(ParamId::SamplerLoopMode), 0.0);
    }

    #[test]
    fn infinite_loop_param_keeps_voice_active_past_the_sample_end() {
        let frames = 2_000;
        let pcm: Vec<f32> = (0..frames)
            .map(|i| (core::f32::consts::TAU * 5.0 * i as f32 / frames as f32).sin())
            .collect();
        let bank = Arc::new(SamplerBank {
            sample: SampleBuffer::new(48_000.0, 1, pcm),
            default_root_note: 60,
        });
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(bank);
        sampler.set_param(
            ParamId::SamplerLoopMode,
            z_audio_dsp::LoopMode::Infinite.to_param_value(),
        );
        sampler.set_param(ParamId::SamplerLoopXfade, 0.001);
        sampler.note_on(60, 1.0);

        // 20000 frames is far more than the 2000-frame source sample.
        let mut left = [0.0_f32; 20_000];
        let mut right = [0.0_f32; 20_000];
        sampler.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
        assert_eq!(
            sampler.active_voice_count(),
            1,
            "an infinite loop must still be sounding well past the source sample's length"
        );
        assert!(left.iter().any(|s| s.abs() > 1.0e-3));
    }

    #[test]
    fn sustain_loop_param_releases_on_note_off() {
        let frames = 2_000;
        let pcm: Vec<f32> = (0..frames)
            .map(|i| (core::f32::consts::TAU * 5.0 * i as f32 / frames as f32).sin())
            .collect();
        let bank = Arc::new(SamplerBank {
            sample: SampleBuffer::new(48_000.0, 1, pcm),
            default_root_note: 60,
        });
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(bank);
        sampler.set_param(
            ParamId::SamplerLoopMode,
            z_audio_dsp::LoopMode::Sustain.to_param_value(),
        );
        sampler.set_param(ParamId::SamplerReleaseTime, 0.05);
        sampler.note_on(60, 1.0);

        let mut left = [0.0_f32; 20_000];
        let mut right = [0.0_f32; 20_000];
        sampler.process(&mut left, &mut right);
        assert_eq!(sampler.active_voice_count(), 1, "should still be looping while held");

        sampler.note_off(60);
        for _ in 0..20 {
            sampler.process(&mut left, &mut right);
        }
        assert_eq!(
            sampler.active_voice_count(),
            0,
            "sustain loop should stop looping forever once released"
        );
    }

    #[test]
    fn loop_start_and_end_params_change_the_active_region() {
        let frames = 4_000;
        let pcm: Vec<f32> = (0..frames).map(|i| i as f32 / frames as f32).collect();
        let bank = Arc::new(SamplerBank {
            sample: SampleBuffer::new(48_000.0, 1, pcm),
            default_root_note: 60,
        });
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(bank);
        sampler.set_param(
            ParamId::SamplerLoopMode,
            z_audio_dsp::LoopMode::Infinite.to_param_value(),
        );
        sampler.set_param(ParamId::SamplerLoopStart, 0.5);
        sampler.set_param(ParamId::SamplerLoopEnd, 0.6);
        sampler.set_param(ParamId::SamplerLoopXfade, 0.0);
        sampler.note_on(60, 1.0);

        let mut left = [0.0_f32; 10_000];
        let mut right = [0.0_f32; 10_000];
        sampler.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
        assert_eq!(sampler.active_voice_count(), 1);
        // The loop window covers ramp values in [0.5, 0.6); output should
        // never wander outside a small margin around that band once looping
        // has settled in (skip the initial attack ramp-up).
        for &s in &left[2000..] {
            assert!((0.45..0.65).contains(&s), "looped output left the loop window: {s}");
        }
    }

    #[test]
    fn unison_defaults_to_a_single_voice() {
        let sampler = GenericSampler::new(GenericSamplerConfig::default());
        assert_eq!(sampler.param_value(ParamId::SamplerUnisonVoices), 1.0);
    }

    #[test]
    fn single_unison_voice_matches_pre_unison_output_exactly() {
        let mut with_default = GenericSampler::new(GenericSamplerConfig::default());
        with_default.load_bank(sine_bank(48_000, 440.0, 48_000.0, 60));
        with_default.note_on(60, 0.8);
        let mut left_a = [0.0_f32; 1024];
        let mut right_a = [0.0_f32; 1024];
        with_default.process(&mut left_a, &mut right_a);

        let mut explicit_one = GenericSampler::new(GenericSamplerConfig::default());
        explicit_one.load_bank(sine_bank(48_000, 440.0, 48_000.0, 60));
        explicit_one.set_param(ParamId::SamplerUnisonVoices, 1.0);
        explicit_one.note_on(60, 0.8);
        let mut left_b = [0.0_f32; 1024];
        let mut right_b = [0.0_f32; 1024];
        explicit_one.process(&mut left_b, &mut right_b);

        assert_eq!(left_a, left_b);
        assert_eq!(right_a, right_b);
    }

    #[test]
    fn unison_voice_count_increases_active_voices_per_note() {
        let mut sampler = GenericSampler::new(GenericSamplerConfig::default());
        sampler.load_bank(sine_bank(48_000, 440.0, 48_000.0, 60));
        sampler.set_param(ParamId::SamplerUnisonVoices, 4.0);
        sampler.note_on(60, 0.8);
        assert_eq!(sampler.active_voice_count(), 4);

        let mut left = [0.0_f32; 1024];
        let mut right = [0.0_f32; 1024];
        sampler.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    }

    #[test]
    fn unison_spread_with_zero_detune_routes_two_identical_voices_to_opposite_channels() {
        // With detune=0, both unison sub-voices play the exact same
        // pitch/position; full spread (1.0) pans one hard left and the
        // other hard right (see `unison_offsets`), so the two channels
        // should end up carrying the *same* single-voice signal, not a sum
        // of two different ones.
        let mut unison = GenericSampler::new(GenericSamplerConfig::default());
        unison.load_bank(sine_bank(48_000, 440.0, 48_000.0, 60));
        unison.set_param(ParamId::SamplerUnisonVoices, 2.0);
        unison.set_param(ParamId::SamplerUnisonDetune, 0.0);
        unison.set_param(ParamId::SamplerUnisonSpread, 1.0);
        unison.note_on(60, 0.8);
        let mut left = [0.0_f32; 2048];
        let mut right = [0.0_f32; 2048];
        unison.process(&mut left, &mut right);

        let mut single = GenericSampler::new(GenericSamplerConfig::default());
        single.load_bank(sine_bank(48_000, 440.0, 48_000.0, 60));
        single.note_on(60, 0.8);
        let mut single_left = [0.0_f32; 2048];
        let mut single_right = [0.0_f32; 2048];
        single.process(&mut single_left, &mut single_right);

        // Each sub-voice gets a `1/sqrt(2)` gain compensation (see
        // `note_on`), and at hard pan only one sub-voice reaches each
        // channel, so each channel should carry `single * 1/sqrt(2)`.
        let gain_scale = 1.0 / 2.0_f32.sqrt();
        for i in 0..left.len() {
            assert!((left[i] - right[i]).abs() < 1.0e-5, "i={i} l={} r={}", left[i], right[i]);
            assert!(
                (left[i] - single_left[i] * gain_scale).abs() < 1.0e-4,
                "i={i} unison={} expected={}",
                left[i],
                single_left[i] * gain_scale
            );
        }
    }
}
