//! Allocation-free polyphonic sample playback core.
//!
//! This module holds the low-level building blocks for sample-based
//! instruments (SFZ-style region maps): [`SampleBuffer`] (preloaded PCM),
//! [`SampleRegion`] (key/velocity range + envelope/tuning metadata), and
//! [`SamplerEngine`] (a fixed-size voice pool with linear interpolation and
//! voice stealing). Loading sample data, decoding, and manifest parsing must
//! happen before `process` is ever called; nothing here allocates, performs
//! file I/O, or logs.

use std::sync::Arc;

use crate::math::{db_to_linear, flush_denormal, midi_note_to_hz};

/// Preloaded mono or stereo PCM at its native sample rate.
#[derive(Clone)]
pub struct SampleBuffer {
    sample_rate: f32,
    channels: u8,
    data: Arc<[f32]>,
}

impl SampleBuffer {
    /// `data` is interleaved if `channels == 2`, otherwise mono.
    pub fn new(sample_rate: f32, channels: u8, data: Vec<f32>) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            channels: channels.max(1),
            data: data.into(),
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u8 {
        self.channels
    }

    pub fn frames(&self) -> usize {
        self.data.len() / self.channels as usize
    }

    #[inline]
    fn frame(&self, index: usize) -> (f32, f32) {
        if self.channels == 1 {
            let s = self.data[index];
            (s, s)
        } else {
            let base = index * self.channels as usize;
            (self.data[base], self.data[base + 1])
        }
    }
}

/// Whether a region plays on `NoteOn` (the held/sustain layer) or `NoteOff`
/// (a short release/hammer-off sample).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    Attack,
    Release,
}

/// One SFZ-style region: a key/velocity range mapped to a sample plus the
/// tuning and envelope metadata needed to play it back.
#[derive(Clone)]
pub struct SampleRegion {
    pub lokey: u8,
    pub hikey: u8,
    pub lovel: u8,
    pub hivel: u8,
    pub pitch_keycenter: u8,
    /// Fine tuning in cents.
    pub tune_cents: f32,
    /// Region gain in dB (already combined with `global_volume`).
    pub volume_db: f32,
    /// `amp_veltrack` as a 0..1 fraction (SFZ's 0..100 percent / 100).
    pub amp_veltrack: f32,
    pub offset_frames: usize,
    pub trigger: TriggerKind,
    pub ampeg_attack: f32,
    pub ampeg_decay: f32,
    pub ampeg_sustain: f32,
    pub ampeg_release: f32,
    pub sample: SampleBuffer,
}

impl SampleRegion {
    pub fn matches(&self, note: u8, velocity_127: u8) -> bool {
        note >= self.lokey
            && note <= self.hikey
            && velocity_127 >= self.lovel
            && velocity_127 <= self.hivel
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// A note-on/note-off request captured while [`SamplerVoice::force_retrigger`]
/// fades out a voice that's being stolen; applied once that fade reaches
/// silence so the new sound never starts with an audible step.
struct PendingTrigger {
    sample_rate: f32,
    activation_id: u64,
    region: Arc<SampleRegion>,
    note: u8,
    velocity01: f32,
    gain_scale: f32,
    release_time_scale: f32,
}

/// How quickly a stolen voice is faded out before the new sound replaces it.
/// Long enough to be inaudible as a click, short enough not to noticeably
/// delay the new note.
const STEAL_KILL_SECONDS: f32 = 0.006;
/// How long the tail of a sample is faded out before it ends, so a sample
/// that's truncated (or otherwise doesn't decay to silence on its own)
/// doesn't produce an audible click when playback reaches its last frame.
const END_OF_SAMPLE_FADE_SECONDS: f32 = 0.015;

struct SamplerVoice {
    region: Option<Arc<SampleRegion>>,
    note: u8,
    is_release_voice: bool,
    position: f64,
    pitch_ratio: f64,
    base_gain: f32,
    stage: Stage,
    env: f32,
    has_decay: bool,
    attack_rate: f32,
    decay_rate: f32,
    sustain_level: f32,
    release_rate: f32,
    activation_id: u64,
    active: bool,
    pending: Option<PendingTrigger>,
}

impl SamplerVoice {
    fn silent() -> Self {
        Self {
            region: None,
            note: 0,
            is_release_voice: false,
            position: 0.0,
            pitch_ratio: 1.0,
            base_gain: 0.0,
            stage: Stage::Idle,
            env: 0.0,
            has_decay: false,
            attack_rate: 1.0,
            decay_rate: 0.0,
            sustain_level: 1.0,
            release_rate: 1.0,
            activation_id: 0,
            active: false,
            pending: None,
        }
    }

    fn is_releasing(&self) -> bool {
        self.active && self.stage == Stage::Release
    }

    fn begin_release(&mut self) {
        if self.active && !self.is_release_voice && self.stage != Stage::Release {
            self.stage = Stage::Release;
        }
    }

    /// Starts playing `pending` immediately, discarding whatever this voice
    /// was doing. Only safe to call on a voice that is silent (inactive, or
    /// whose envelope has just faded to zero); otherwise use
    /// [`SamplerVoice::force_retrigger`] to avoid a click.
    fn start(&mut self, pending: PendingTrigger) {
        let PendingTrigger {
            sample_rate,
            activation_id,
            region,
            note,
            velocity01,
            gain_scale,
            release_time_scale,
        } = pending;

        let note_hz = midi_note_to_hz(note as f32 + region.tune_cents / 100.0);
        let root_hz = midi_note_to_hz(region.pitch_keycenter as f32);
        let pitch_ratio =
            (note_hz / root_hz) as f64 * (region.sample.sample_rate() / sample_rate) as f64;

        let veltrack = region.amp_veltrack.clamp(0.0, 1.0);
        let vel_gain = (1.0 - veltrack + veltrack * velocity01.clamp(0.0, 1.0)).max(0.0);

        self.is_release_voice = region.trigger == TriggerKind::Release;
        self.note = note;
        self.position = region.offset_frames as f64;
        self.pitch_ratio = pitch_ratio.max(1.0e-6);
        self.base_gain = db_to_linear(region.volume_db) * vel_gain * gain_scale;
        self.env = 0.0;
        self.attack_rate = 1.0 / (region.ampeg_attack.max(0.001) * sample_rate);
        self.has_decay = region.ampeg_decay > 0.0;
        if self.has_decay {
            let sustain = region.ampeg_sustain.clamp(0.0, 1.0);
            self.decay_rate = (1.0 - sustain) / (region.ampeg_decay.max(0.001) * sample_rate);
            self.sustain_level = sustain;
        } else {
            self.decay_rate = 0.0;
            self.sustain_level = 1.0;
        }
        let release_seconds =
            (region.ampeg_release.max(0.01) * release_time_scale.max(0.01)).max(0.01);
        self.release_rate = 1.0 / (release_seconds * sample_rate);
        self.stage = Stage::Attack;
        self.activation_id = activation_id;
        self.active = true;
        self.pending = None;
        self.region = Some(region);
    }

    /// Starts playing `pending`, fading this voice out first over
    /// [`STEAL_KILL_SECONDS`] if it's currently sounding, instead of cutting
    /// it off mid-sample (which produces an audible click/pop).
    fn force_retrigger(&mut self, sample_rate: f32, pending: PendingTrigger) {
        if !self.active {
            self.start(pending);
            return;
        }
        let kill_rate = 1.0 / (STEAL_KILL_SECONDS * sample_rate);
        if self.stage != Stage::Release || self.release_rate < kill_rate {
            self.stage = Stage::Release;
            self.release_rate = kill_rate;
        }
        self.pending = Some(pending);
    }

    #[inline]
    fn next_sample(&mut self) -> (f32, f32) {
        if !self.active {
            return (0.0, 0.0);
        }
        let Some(region) = self.region.as_ref() else {
            self.active = false;
            return (0.0, 0.0);
        };
        let frames = region.sample.frames();
        let idx = self.position as usize;
        if frames < 2 || idx + 1 >= frames {
            self.active = false;
            if let Some(pending) = self.pending.take() {
                self.start(pending);
            }
            return (0.0, 0.0);
        }
        let frac = (self.position - idx as f64) as f32;
        let (l0, r0) = region.sample.frame(idx);
        let (l1, r1) = region.sample.frame(idx + 1);
        let l = l0 + (l1 - l0) * frac;
        let r = r0 + (r1 - r0) * frac;

        let fade_frames = ((region.sample.sample_rate() * END_OF_SAMPLE_FADE_SECONDS) as usize).max(1);
        let remaining = frames - 1 - idx;
        let end_fade = if remaining < fade_frames {
            remaining as f32 / fade_frames as f32
        } else {
            1.0
        };

        match self.stage {
            Stage::Attack => {
                self.env += self.attack_rate;
                if self.env >= 1.0 {
                    self.env = 1.0;
                    self.stage = if self.has_decay {
                        Stage::Decay
                    } else {
                        Stage::Sustain
                    };
                }
            }
            Stage::Decay => {
                self.env -= self.decay_rate;
                if self.env <= self.sustain_level {
                    self.env = self.sustain_level;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {}
            Stage::Release => {
                self.env -= self.release_rate;
                if self.env <= 0.0005 {
                    self.env = 0.0;
                    self.active = false;
                    if let Some(pending) = self.pending.take() {
                        self.start(pending);
                    }
                }
            }
            Stage::Idle => {}
        }

        self.position += self.pitch_ratio;
        let gain = flush_denormal(self.env * self.base_gain * end_fade);
        (flush_denormal(l * gain), flush_denormal(r * gain))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SamplerEngineConfig {
    pub sample_rate: f32,
    pub max_voices: usize,
}

impl Default for SamplerEngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            max_voices: 32,
        }
    }
}

/// A fixed-size pool of [`SamplerVoice`]s. Does not own a region map; the
/// caller looks up matching [`SampleRegion`]s and calls [`SamplerEngine::trigger`].
pub struct SamplerEngine {
    sample_rate: f32,
    voices: Vec<SamplerVoice>,
    next_activation_id: u64,
}

impl SamplerEngine {
    pub fn new(config: SamplerEngineConfig) -> Self {
        let voices = (0..config.max_voices.max(1))
            .map(|_| SamplerVoice::silent())
            .collect();
        Self {
            sample_rate: config.sample_rate.max(1.0),
            voices,
            next_activation_id: 0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    /// Starts a new voice playing `region` for `note`. `velocity01` (0..1)
    /// scales loudness via the region's `amp_veltrack`; `gain_scale` is an
    /// extra linear multiplier (e.g. a "release level" control); `release_time_scale`
    /// multiplies the region's `ampeg_release` for attack-trigger voices.
    pub fn trigger(
        &mut self,
        region: Arc<SampleRegion>,
        note: u8,
        velocity01: f32,
        gain_scale: f32,
        release_time_scale: f32,
    ) {
        let index = self.find_voice_slot();
        self.next_activation_id = self.next_activation_id.wrapping_add(1);
        let pending = PendingTrigger {
            sample_rate: self.sample_rate,
            activation_id: self.next_activation_id,
            region,
            note,
            velocity01,
            gain_scale,
            release_time_scale,
        };
        self.voices[index].force_retrigger(self.sample_rate, pending);
    }

    /// Begins the release fade for any active attack-trigger voice(s)
    /// currently playing `note`. Release-trigger voices are unaffected; they
    /// finish naturally when their sample ends.
    pub fn note_off(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                voice.begin_release();
            }
        }
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        left.fill(0.0);
        right.fill(0.0);
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }
            for i in 0..left.len() {
                let (l, r) = voice.next_sample();
                left[i] += l;
                right[i] += r;
                if !voice.active {
                    break;
                }
            }
        }
    }

    fn find_voice_slot(&self) -> usize {
        if let Some(index) = self.voices.iter().position(|v| !v.active) {
            return index;
        }
        self.voices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_releasing())
            .min_by_key(|(_, v)| v.activation_id)
            .or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.activation_id)
            })
            .map(|(index, _)| index)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(
        trigger: TriggerKind,
        lokey: u8,
        hikey: u8,
        pitch_keycenter: u8,
        frames: usize,
    ) -> Arc<SampleRegion> {
        let data = (0..frames)
            .map(|i| (i as f32 / frames as f32) * 2.0 - 1.0)
            .collect::<Vec<_>>();
        Arc::new(SampleRegion {
            lokey,
            hikey,
            lovel: 0,
            hivel: 127,
            pitch_keycenter,
            tune_cents: 0.0,
            volume_db: 0.0,
            amp_veltrack: 1.0,
            offset_frames: 0,
            trigger,
            ampeg_attack: 0.001,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.05,
            sample: SampleBuffer::new(48_000.0, 1, data),
        })
    }

    #[test]
    fn note_on_produces_signal_without_allocation_panics() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 4,
        });
        let r = region(TriggerKind::Attack, 60, 60, 60, 48_000);
        engine.trigger(r, 60, 1.0, 1.0, 1.0);
        let mut left = [0.0_f32; 256];
        let mut right = [0.0_f32; 256];
        engine.process(&mut left, &mut right);
        assert!(left.iter().any(|s| s.abs() > 0.0));
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    }

    #[test]
    fn pitch_ratio_follows_root_key() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 4,
        });
        let r = region(TriggerKind::Attack, 0, 127, 60, 480_000);
        engine.trigger(r, 72, 1.0, 1.0, 1.0); // one octave up -> 2x playback speed
        let mut left = [0.0_f32; 1];
        let mut right = [0.0_f32; 1];
        for _ in 0..100 {
            engine.process(&mut left, &mut right);
        }
        // After 100 samples at ~2x speed the voice should still be active and finite.
        assert!(left[0].is_finite());
    }

    #[test]
    fn note_off_releases_attack_voice() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 4,
        });
        let r = region(TriggerKind::Attack, 60, 60, 60, 480_000);
        engine.trigger(r, 60, 1.0, 1.0, 1.0);
        engine.note_off(60);
        let mut left = [0.0_f32; 64];
        let mut right = [0.0_f32; 64];
        for _ in 0..200 {
            engine.process(&mut left, &mut right);
        }
        assert_eq!(engine.active_voice_count(), 0);
    }

    #[test]
    fn release_trigger_voice_unaffected_by_note_off() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 4,
        });
        let r = region(TriggerKind::Release, 60, 60, 60, 480_000);
        engine.trigger(r, 60, 1.0, 1.0, 1.0);
        engine.note_off(60); // should be a no-op for release-trigger voices
        assert_eq!(engine.active_voice_count(), 1);
    }

    #[test]
    fn voice_stealing_caps_polyphony() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 2,
        });
        for note in [60, 62, 64] {
            let r = region(TriggerKind::Attack, note, note, note, 480_000);
            engine.trigger(r, note, 1.0, 1.0, 1.0);
        }
        assert!(engine.active_voice_count() <= 2);
    }

    #[test]
    fn out_of_range_read_does_not_panic_at_sample_end() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        let r = region(TriggerKind::Attack, 60, 60, 60, 8);
        engine.trigger(r, 60, 1.0, 1.0, 1.0);
        let mut left = [0.0_f32; 64];
        let mut right = [0.0_f32; 64];
        engine.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
        assert_eq!(engine.active_voice_count(), 0);
    }

    fn max_step(samples: &[f32]) -> f32 {
        samples
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0_f32, f32::max)
    }

    #[test]
    fn stealing_a_loud_voice_does_not_click() {
        // One voice slot; the region's data ramps up to full scale (+1.0),
        // so stealing it mid-playback with no fade would produce a large
        // sample-to-sample jump back toward the new voice's near-zero
        // attack start.
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        let r = region(TriggerKind::Attack, 60, 60, 60, 480_000);
        engine.trigger(r, 60, 1.0, 1.0, 1.0);

        // Run the first voice well past its attack ramp so it's at full
        // volume and playing loud (non-silent) material.
        let mut left = [0.0_f32; 4096];
        let mut right = [0.0_f32; 4096];
        engine.process(&mut left, &mut right);
        assert!(max_step(&left) < 0.05, "warm-up should already be smooth");

        // Steal the only voice with a new note while the old one is loud.
        let r2 = region(TriggerKind::Attack, 67, 67, 67, 480_000);
        engine.trigger(r2, 67, 1.0, 1.0, 1.0);

        let mut steal_left = [0.0_f32; 2048];
        let mut steal_right = [0.0_f32; 2048];
        engine.process(&mut steal_left, &mut steal_right);
        assert!(steal_left.iter().chain(steal_right.iter()).all(|s| s.is_finite()));
        assert!(
            max_step(&steal_left) < 0.05,
            "stealing a loud voice should fade, not jump: max_step={}",
            max_step(&steal_left)
        );
    }

    #[test]
    fn sample_end_fades_out_instead_of_clicking() {
        // `region()`'s data ramps linearly up to +1.0 right at the last
        // frame, i.e. playback would otherwise hit full scale then cut to
        // silence in a single sample without the end-of-sample fade.
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        let r = region(TriggerKind::Attack, 60, 60, 60, 2048);
        engine.trigger(r, 60, 1.0, 1.0, 1.0);

        let mut left = [0.0_f32; 2048];
        let mut right = [0.0_f32; 2048];
        engine.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
        assert!(
            max_step(&left) < 0.1,
            "sample end should fade, not cut: max_step={}",
            max_step(&left)
        );
        assert_eq!(engine.active_voice_count(), 0);
    }
}
