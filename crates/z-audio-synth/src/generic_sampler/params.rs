//! Realtime-controllable parameters for [`super::GenericSampler`].

use z_audio_dsp::LoopMode;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenericSamplerParams {
    pub master_gain_db: f32,
    /// Root (basic pitch) MIDI note of the loaded sample.
    pub root_note: f32,
    pub tune_cents: f32,
    /// Playback start position, normalized `0..1` of the sample length.
    pub offset01: f32,
    pub velocity_curve: f32,
    pub release_time_s: f32,
    pub stereo_width: f32,
    pub loop_mode: LoopMode,
    /// Loop window start, normalized `0..1` of the sample length.
    pub loop_start01: f32,
    /// Loop window end, normalized `0..1` of the sample length.
    pub loop_end01: f32,
    pub loop_xfade_s: f32,
    /// Number of unison sub-voices triggered per note, `1..=8`. `1` (the
    /// default) is a single voice with no detune/pan spread, identical to
    /// pre-unison behavior.
    pub unison_voices: f32,
    pub unison_detune_cents: f32,
    /// `0..1`: how far sub-voices are panned across the stereo field.
    pub unison_spread: f32,
}

impl Default for GenericSamplerParams {
    fn default() -> Self {
        Self {
            master_gain_db: 0.0,
            root_note: 60.0,
            tune_cents: 0.0,
            offset01: 0.0,
            velocity_curve: 0.5,
            release_time_s: 0.2,
            stereo_width: 1.0,
            loop_mode: LoopMode::Off,
            loop_start01: 0.0,
            loop_end01: 1.0,
            loop_xfade_s: 0.01,
            unison_voices: 1.0,
            unison_detune_cents: 10.0,
            unison_spread: 0.5,
        }
    }
}
