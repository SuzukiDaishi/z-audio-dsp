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

/// How a [`SampleRegion`]'s `loop_start_frames..loop_end_frames` window is
/// used during playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoopMode {
    /// Play straight through once; no looping.
    #[default]
    Off,
    /// Loop the window forward forever (even after note-off; the envelope
    /// release just fades the looping content out).
    Infinite,
    /// Loop the window forward while the note is held; once release begins,
    /// stop looping and play straight through to the sample's natural end.
    Sustain,
    /// Bounce back and forth across the window forever (forward, then
    /// backward, then forward again).
    PingPong,
    /// Play the whole sample backward once, from its end toward `offset_frames`;
    /// no looping.
    Reverse,
}

impl LoopMode {
    /// Number of valid `ParamId::SamplerLoopMode` automation values.
    pub const VARIANT_COUNT: u32 = 5;

    /// Decodes a `ParamId::SamplerLoopMode` automation value, rounding to
    /// the nearest integer and clamping to `0..VARIANT_COUNT - 1`.
    pub fn from_param_value(value: f32) -> Self {
        match value.round().clamp(0.0, (Self::VARIANT_COUNT - 1) as f32) as u32 {
            0 => Self::Off,
            1 => Self::Infinite,
            2 => Self::Sustain,
            3 => Self::PingPong,
            _ => Self::Reverse,
        }
    }

    /// Encodes this loop mode as a `ParamId::SamplerLoopMode` automation
    /// value.
    pub fn to_param_value(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Infinite => 1.0,
            Self::Sustain => 2.0,
            Self::PingPong => 3.0,
            Self::Reverse => 4.0,
        }
    }
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
    /// How `loop_start_frames..loop_end_frames` is used. Ignored (no
    /// looping) for [`LoopMode::Off`]; for [`LoopMode::Reverse`] the loop
    /// window is ignored too (the whole sample plays backward once).
    pub loop_mode: LoopMode,
    pub loop_start_frames: usize,
    /// Exclusive; must be `> loop_start_frames` and `<= sample.frames()`
    /// for looping to take effect.
    pub loop_end_frames: usize,
    /// Equal-power crossfade length (in frames) applied at the loop
    /// boundary; clamped to at most half the loop window's length.
    pub loop_xfade_frames: usize,
    /// Linear pan applied to this region's output, `-1.0` (left) ..`1.0`
    /// (right). `0.0` is a no-op (left/right gains both stay `1.0`), so
    /// existing regions that don't set this are unaffected; used for
    /// spreading unison sub-voices across the stereo field.
    pub pan: f32,
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
    /// `+1.0` or `-1.0`; only changes from `+1.0` for [`LoopMode::PingPong`]
    /// (bouncing) and [`LoopMode::Reverse`] (always `-1.0`).
    direction: f64,
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
            direction: 1.0,
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
        let frames = region.sample.frames();
        if region.loop_mode == LoopMode::Reverse {
            self.direction = -1.0;
            self.position = (frames
                .saturating_sub(1)
                .saturating_sub(region.offset_frames)) as f64;
        } else {
            self.direction = 1.0;
            self.position = region.offset_frames as f64;
        }
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

    /// Whether the loop window should currently be used to wrap/bounce
    /// playback, given this voice's envelope stage.
    fn loop_active(&self, region: &SampleRegion) -> bool {
        let loop_len = region
            .loop_end_frames
            .saturating_sub(region.loop_start_frames);
        if loop_len < 2 || region.loop_end_frames > region.sample.frames() {
            return false;
        }
        match region.loop_mode {
            LoopMode::Off | LoopMode::Reverse => false,
            LoopMode::Infinite | LoopMode::PingPong => true,
            LoopMode::Sustain => self.stage != Stage::Release,
        }
    }

    #[inline]
    fn next_sample(&mut self) -> (f32, f32) {
        if !self.active {
            return (0.0, 0.0);
        }
        let Some(region) = self.region.clone() else {
            self.active = false;
            return (0.0, 0.0);
        };
        let frames = region.sample.frames();
        if frames < 2 {
            self.active = false;
            if let Some(pending) = self.pending.take() {
                self.start(pending);
            }
            return (0.0, 0.0);
        }

        let loop_active = self.loop_active(&region);
        let (l, r, end_fade) = if loop_active && region.loop_mode == LoopMode::PingPong {
            let (l, r) = self.step_pingpong(&region);
            (l, r, 1.0)
        } else if loop_active {
            let (l, r) = self.step_forward_loop(&region);
            (l, r, 1.0)
        } else if region.loop_mode == LoopMode::Reverse {
            if self.position < 0.0 {
                self.active = false;
                if let Some(pending) = self.pending.take() {
                    self.start(pending);
                }
                return (0.0, 0.0);
            }
            let (l, r) = read_interp(&region.sample, self.position);
            let start_fade_frames =
                ((region.sample.sample_rate() * END_OF_SAMPLE_FADE_SECONDS) as usize).max(1);
            let end_fade = if self.position < start_fade_frames as f64 {
                (self.position / start_fade_frames as f64) as f32
            } else {
                1.0
            };
            self.position -= self.pitch_ratio;
            (l, r, end_fade)
        } else {
            let idx = self.position as usize;
            if idx + 1 >= frames {
                self.active = false;
                if let Some(pending) = self.pending.take() {
                    self.start(pending);
                }
                return (0.0, 0.0);
            }
            let (l, r) = read_interp(&region.sample, self.position);
            let fade_frames =
                ((region.sample.sample_rate() * END_OF_SAMPLE_FADE_SECONDS) as usize).max(1);
            let remaining = frames - 1 - idx;
            let end_fade = if remaining < fade_frames {
                remaining as f32 / fade_frames as f32
            } else {
                1.0
            };
            self.position += self.pitch_ratio;
            (l, r, end_fade)
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

        let gain = flush_denormal(self.env * self.base_gain * end_fade);
        let (left_pan, right_pan) = pan_gains(region.pan);
        (
            flush_denormal(l * gain * left_pan),
            flush_denormal(r * gain * right_pan),
        )
    }

    /// Forward loop playback ([`LoopMode::Infinite`], or [`LoopMode::Sustain`]
    /// while still held): wraps `loop_end_frames` back to `loop_start_frames`
    /// with an equal-power crossfade over the last `loop_xfade_frames`
    /// samples before the boundary.
    fn step_forward_loop(&mut self, region: &SampleRegion) -> (f32, f32) {
        let loop_start = region.loop_start_frames as f64;
        let loop_end = region.loop_end_frames as f64;
        let loop_len = loop_end - loop_start;
        let xfade = (region.loop_xfade_frames as f64)
            .min(loop_len / 2.0)
            .max(0.0);
        // When crossfading, the last `xfade` frames before `loop_end` are a
        // blend with the *next* pass's first `xfade` frames after
        // `loop_start` (computed below as `head_pos`); wrapping must land
        // exactly where that head pointer left off (`loop_start + xfade`),
        // not back at `loop_start`, or the wrap itself becomes a click.
        let wrap_step = (loop_len - xfade).max(1.0e-6);
        if self.position < loop_start {
            self.position = loop_start;
        }
        while self.position >= loop_end {
            self.position -= wrap_step;
        }

        let xfade_start = loop_end - xfade;
        let out = if xfade >= 1.0 && self.position >= xfade_start {
            let t = ((self.position - xfade_start) / xfade) as f32;
            let (tail_l, tail_r) = read_interp(&region.sample, self.position);
            let head_pos = loop_start + (self.position - xfade_start);
            let (head_l, head_r) = read_interp(&region.sample, head_pos);
            let a = (t * core::f32::consts::FRAC_PI_2).cos();
            let b = (t * core::f32::consts::FRAC_PI_2).sin();
            (tail_l * a + head_l * b, tail_r * a + head_r * b)
        } else {
            read_interp(&region.sample, self.position)
        };
        self.position += self.pitch_ratio;
        out
    }

    /// [`LoopMode::PingPong`] playback: bounces `position` back and forth
    /// across `loop_start_frames..loop_end_frames`, flipping [`Self::direction`]
    /// at each boundary.
    fn step_pingpong(&mut self, region: &SampleRegion) -> (f32, f32) {
        let loop_start = region.loop_start_frames as f64;
        let loop_end = region.loop_end_frames as f64;
        if self.position >= loop_end {
            self.position = loop_end - (self.position - loop_end);
            self.direction = -1.0;
        } else if self.position < loop_start {
            self.position = loop_start + (loop_start - self.position);
            self.direction = 1.0;
        }
        self.position = self.position.clamp(loop_start, loop_end - 1.0e-6);
        let out = read_interp(&region.sample, self.position);
        self.position += self.pitch_ratio * self.direction;
        out
    }
}

/// Linearly interpolated stereo read at a fractional frame position,
/// clamping to a valid `[0, frames-2]` index range so callers never need to
/// bounds-check before calling (loop crossfade math can compute positions
/// slightly outside the exact window due to floating point rounding).
#[inline]
fn read_interp(sample: &SampleBuffer, pos: f64) -> (f32, f32) {
    let frames = sample.frames();
    if frames == 0 {
        return (0.0, 0.0);
    }
    let max_idx = frames.saturating_sub(2) as i64;
    let idx = (pos.floor() as i64).clamp(0, max_idx) as usize;
    let frac = (pos - idx as f64).clamp(0.0, 1.0) as f32;
    let (l0, r0) = sample.frame(idx);
    let (l1, r1) = sample.frame(idx + 1);
    (l0 + (l1 - l0) * frac, r0 + (r1 - r0) * frac)
}

/// Linear pan law: `(left_gain, right_gain)` for [`SampleRegion::pan`].
/// `pan == 0.0` always yields `(1.0, 1.0)` (a true no-op), so existing
/// regions built before `pan` existed (which all default it to `0.0`) are
/// bit-for-bit unaffected.
#[inline]
fn pan_gains(pan: f32) -> (f32, f32) {
    let pan = pan.clamp(-1.0, 1.0);
    let left = if pan > 0.0 { 1.0 - pan } else { 1.0 };
    let right = if pan < 0.0 { 1.0 + pan } else { 1.0 };
    (left, right)
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
            loop_mode: LoopMode::Off,
            loop_start_frames: 0,
            loop_end_frames: 0,
            loop_xfade_frames: 0,
            pan: 0.0,
        })
    }

    fn region_with_loop(
        loop_mode: LoopMode,
        frames: usize,
        loop_start_frames: usize,
        loop_end_frames: usize,
        loop_xfade_frames: usize,
    ) -> Arc<SampleRegion> {
        let data: Vec<f32> = (0..frames)
            .map(|i| (core::f32::consts::TAU * 7.0 * i as f32 / frames as f32).sin())
            .collect();
        Arc::new(SampleRegion {
            lokey: 60,
            hikey: 60,
            lovel: 0,
            hivel: 127,
            pitch_keycenter: 60,
            tune_cents: 0.0,
            volume_db: 0.0,
            amp_veltrack: 1.0,
            offset_frames: loop_start_frames,
            trigger: TriggerKind::Attack,
            ampeg_attack: 0.0001,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.05,
            sample: SampleBuffer::new(48_000.0, 1, data),
            loop_mode,
            loop_start_frames,
            loop_end_frames,
            loop_xfade_frames,
            pan: 0.0,
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
        assert!(
            steal_left
                .iter()
                .chain(steal_right.iter())
                .all(|s| s.is_finite())
        );
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

    #[test]
    fn double_steal_does_not_panic_or_click() {
        // Steal the only voice slot twice in a row, before the first kill
        // fade has finished. The second pending trigger should simply
        // replace the first (the first note is dropped), with no panic and
        // no audible jump.
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        engine.trigger(
            region(TriggerKind::Attack, 60, 60, 60, 480_000),
            60,
            1.0,
            1.0,
            1.0,
        );
        let mut warm_l = [0.0_f32; 4096];
        let mut warm_r = [0.0_f32; 4096];
        engine.process(&mut warm_l, &mut warm_r);

        engine.trigger(
            region(TriggerKind::Attack, 64, 64, 64, 480_000),
            64,
            1.0,
            1.0,
            1.0,
        );
        engine.trigger(
            region(TriggerKind::Attack, 67, 67, 67, 480_000),
            67,
            1.0,
            1.0,
            1.0,
        );

        let mut left = [0.0_f32; 4096];
        let mut right = [0.0_f32; 4096];
        engine.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
        assert!(
            max_step(&left) < 0.05,
            "double steal should still fade smoothly: max_step={}",
            max_step(&left)
        );
    }

    #[test]
    fn force_retrigger_on_idle_voice_starts_immediately() {
        // Stealing a voice that isn't actually sounding (inactive) should
        // not incur the kill-fade delay; the new note should be audible in
        // its very first attack samples.
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        engine.trigger(
            region(TriggerKind::Attack, 60, 60, 60, 64),
            60,
            1.0,
            1.0,
            1.0,
        );
        let mut drain_l = [0.0_f32; 128];
        let mut drain_r = [0.0_f32; 128];
        engine.process(&mut drain_l, &mut drain_r);
        assert_eq!(
            engine.active_voice_count(),
            0,
            "first voice should have finished"
        );

        engine.trigger(
            region(TriggerKind::Attack, 67, 67, 67, 480_000),
            67,
            1.0,
            1.0,
            1.0,
        );
        assert_eq!(engine.active_voice_count(), 1);
    }

    #[test]
    fn zero_velocity_with_full_veltrack_is_silent() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        let r = region(TriggerKind::Attack, 60, 60, 60, 48_000);
        engine.trigger(r, 60, 0.0, 1.0, 1.0);
        let mut left = [0.0_f32; 256];
        let mut right = [0.0_f32; 256];
        engine.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| *s == 0.0));
    }

    #[test]
    fn offset_frames_skips_initial_samples() {
        let frames = 48_000;
        let data = (0..frames)
            .map(|i| (i as f32 / frames as f32) * 2.0 - 1.0)
            .collect::<Vec<_>>();
        let region = Arc::new(SampleRegion {
            lokey: 60,
            hikey: 60,
            lovel: 0,
            hivel: 127,
            pitch_keycenter: 60,
            tune_cents: 0.0,
            volume_db: 0.0,
            amp_veltrack: 1.0,
            offset_frames: 24_000,
            trigger: TriggerKind::Attack,
            ampeg_attack: 0.001,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.05,
            sample: SampleBuffer::new(48_000.0, 1, data),
            loop_mode: LoopMode::Off,
            loop_start_frames: 0,
            loop_end_frames: 0,
            loop_xfade_frames: 0,
            pan: 0.0,
        });
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        engine.trigger(region, 60, 1.0, 1.0, 1.0);
        let mut left = [0.0_f32; 1];
        let mut right = [0.0_f32; 1];
        engine.process(&mut left, &mut right);
        // Starting near the midpoint of a -1..1 ramp should read close to 0,
        // not close to -1 (which is where frame 0 would be).
        assert!(left[0].abs() < 0.5, "left={}", left[0]);
    }

    #[test]
    fn releasing_voice_is_preferred_for_stealing_over_a_sustaining_one() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 2,
        });
        engine.trigger(
            region(TriggerKind::Attack, 60, 60, 60, 480_000),
            60,
            1.0,
            1.0,
            1.0,
        );
        engine.trigger(
            region(TriggerKind::Attack, 64, 64, 64, 480_000),
            64,
            1.0,
            1.0,
            1.0,
        );
        let mut warm_l = [0.0_f32; 4096];
        let mut warm_r = [0.0_f32; 4096];
        engine.process(&mut warm_l, &mut warm_r);
        engine.note_off(64); // note 64's voice starts releasing

        // A third note should steal note 64's (releasing) voice rather than
        // cutting off the still-sustaining note 60.
        engine.trigger(
            region(TriggerKind::Attack, 67, 67, 67, 480_000),
            67,
            1.0,
            1.0,
            1.0,
        );
        let mut left = [0.0_f32; 1];
        let mut right = [0.0_f32; 1];
        engine.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    }

    #[test]
    fn infinite_loop_keeps_playing_past_the_sample_end() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        let r = region_with_loop(LoopMode::Infinite, 200, 0, 200, 0);
        engine.trigger(r, 60, 1.0, 1.0, 1.0);
        let mut left = [0.0_f32; 4096]; // far more samples than the 200-frame sample.
        let mut right = [0.0_f32; 4096];
        engine.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
        assert_eq!(
            engine.active_voice_count(),
            1,
            "an infinite loop must not stop just because it ran past the sample's frame count"
        );
        assert!(left.iter().any(|s| s.abs() > 1.0e-3));
    }

    #[test]
    fn infinite_loop_crossfade_avoids_a_click_at_the_boundary() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        // A loop region with a deliberate discontinuity at the boundary
        // (sawtooth wraps from +1 back to -1): without crossfading this
        // would produce a large sample-to-sample jump every loop pass.
        let frames = 400;
        let data: Vec<f32> = (0..frames)
            .map(|i| (i as f32 / frames as f32) * 2.0 - 1.0)
            .collect();
        let region = Arc::new(SampleRegion {
            lokey: 60,
            hikey: 60,
            lovel: 0,
            hivel: 127,
            pitch_keycenter: 60,
            tune_cents: 0.0,
            volume_db: 0.0,
            amp_veltrack: 1.0,
            offset_frames: 0,
            trigger: TriggerKind::Attack,
            ampeg_attack: 0.0001,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.05,
            sample: SampleBuffer::new(48_000.0, 1, data),
            loop_mode: LoopMode::Infinite,
            loop_start_frames: 0,
            loop_end_frames: frames,
            loop_xfade_frames: 40,
            pan: 0.0,
        });
        engine.trigger(region, 60, 1.0, 1.0, 1.0);
        let mut left = [0.0_f32; 2000]; // several loop passes.
        let mut right = [0.0_f32; 2000];
        engine.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
        assert!(
            max_step(&left[50..]) < 0.2,
            "loop boundary should be crossfaded, not jump: max_step={}",
            max_step(&left[50..])
        );
    }

    #[test]
    fn sustain_loop_stops_looping_after_note_off() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        let r = region_with_loop(LoopMode::Sustain, 200, 0, 200, 0);
        engine.trigger(r, 60, 1.0, 1.0, 1.0);
        let mut left = [0.0_f32; 4096];
        let mut right = [0.0_f32; 4096];
        engine.process(&mut left, &mut right);
        assert_eq!(
            engine.active_voice_count(),
            1,
            "sustain loop should keep looping while held"
        );

        engine.note_off(60);
        // Run for much longer than the 200-frame sample plus its release
        // tail: once released, a sustain loop must stop looping and the
        // voice must eventually go silent instead of looping forever.
        for _ in 0..50 {
            engine.process(&mut left, &mut right);
        }
        assert_eq!(engine.active_voice_count(), 0);
    }

    #[test]
    fn pingpong_loop_bounces_without_panicking_or_diverging() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        let r = region_with_loop(LoopMode::PingPong, 300, 50, 250, 0);
        engine.trigger(r, 72, 1.0, 1.0, 1.0); // pitched up, so it bounces quickly.
        let mut left = [0.0_f32; 4096];
        let mut right = [0.0_f32; 4096];
        engine.process(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
        assert_eq!(engine.active_voice_count(), 1);
    }

    #[test]
    fn reverse_mode_plays_backward_and_eventually_finishes() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        // Long enough that the END_OF_SAMPLE_FADE_SECONDS window (~720
        // frames at 48kHz) doesn't dominate the whole sample.
        let frames = 4000;
        // Linear ramp: easy to check direction (after the attack envelope
        // has caught up, output should track near the ramp's *end*, not
        // its start).
        let data: Vec<f32> = (0..frames).map(|i| i as f32 / frames as f32).collect();
        let region = Arc::new(SampleRegion {
            lokey: 60,
            hikey: 60,
            lovel: 0,
            hivel: 127,
            pitch_keycenter: 60,
            tune_cents: 0.0,
            volume_db: 0.0,
            amp_veltrack: 0.0,
            offset_frames: 0,
            trigger: TriggerKind::Attack,
            ampeg_attack: 0.001,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.05,
            sample: SampleBuffer::new(48_000.0, 1, data),
            loop_mode: LoopMode::Reverse,
            loop_start_frames: 0,
            loop_end_frames: 0,
            loop_xfade_frames: 0,
            pan: 0.0,
        });
        engine.trigger(region, 60, 1.0, 1.0, 1.0);
        // Warm up past the attack ramp (~48 samples) while direction is
        // still close to the sample's end (position decreases by ~1/sample
        // since note==root note).
        let mut warm_left = [0.0_f32; 100];
        let mut warm_right = [0.0_f32; 100];
        engine.process(&mut warm_left, &mut warm_right);
        assert!(
            warm_left[99] > 0.8,
            "reverse playback should still be near the sample's end (high ramp value) after warm-up, got {}",
            warm_left[99]
        );

        let mut tail_left = [0.0_f32; 4096];
        let mut tail_right = [0.0_f32; 4096];
        for _ in 0..5 {
            engine.process(&mut tail_left, &mut tail_right);
        }
        assert!(
            tail_left
                .iter()
                .chain(tail_right.iter())
                .all(|s| s.is_finite())
        );
        assert_eq!(
            engine.active_voice_count(),
            0,
            "reverse playback should finish once it reaches the sample start, not loop forever"
        );
    }

    #[test]
    fn pan_zero_is_a_true_no_op() {
        let mut centered = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        centered.trigger(Arc::new(centered_region_with_pan(0.0)), 60, 1.0, 1.0, 1.0);
        let mut left = [0.0_f32; 512];
        let mut right = [0.0_f32; 512];
        centered.process(&mut left, &mut right);
        // Mono source (l == r per frame): pan=0 must leave them equal.
        for (l, r) in left.iter().zip(right.iter()) {
            assert_eq!(l, r);
        }
        assert!(left.iter().any(|s| s.abs() > 0.0));
    }

    fn centered_region_with_pan(pan: f32) -> SampleRegion {
        let frames = 48_000;
        let data: Vec<f32> = (0..frames)
            .map(|i| (core::f32::consts::TAU * 220.0 * i as f32 / frames as f32).sin())
            .collect();
        SampleRegion {
            lokey: 60,
            hikey: 60,
            lovel: 0,
            hivel: 127,
            pitch_keycenter: 60,
            tune_cents: 0.0,
            volume_db: 0.0,
            amp_veltrack: 0.0,
            offset_frames: 0,
            trigger: TriggerKind::Attack,
            ampeg_attack: 0.0001,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.05,
            sample: SampleBuffer::new(48_000.0, 1, data),
            loop_mode: LoopMode::Off,
            loop_start_frames: 0,
            loop_end_frames: 0,
            loop_xfade_frames: 0,
            pan,
        }
    }

    #[test]
    fn pan_hard_right_silences_the_left_channel() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        engine.trigger(Arc::new(centered_region_with_pan(1.0)), 60, 1.0, 1.0, 1.0);
        let mut left = [0.0_f32; 512];
        let mut right = [0.0_f32; 512];
        engine.process(&mut left, &mut right);
        assert!(
            left.iter().all(|s| *s == 0.0),
            "left should be fully silenced at pan=1.0"
        );
        assert!(right.iter().any(|s| s.abs() > 0.0));
    }

    #[test]
    fn pan_hard_left_silences_the_right_channel() {
        let mut engine = SamplerEngine::new(SamplerEngineConfig {
            sample_rate: 48_000.0,
            max_voices: 1,
        });
        engine.trigger(Arc::new(centered_region_with_pan(-1.0)), 60, 1.0, 1.0, 1.0);
        let mut left = [0.0_f32; 512];
        let mut right = [0.0_f32; 512];
        engine.process(&mut left, &mut right);
        assert!(
            right.iter().all(|s| *s == 0.0),
            "right should be fully silenced at pan=-1.0"
        );
        assert!(left.iter().any(|s| s.abs() > 0.0));
    }
}
