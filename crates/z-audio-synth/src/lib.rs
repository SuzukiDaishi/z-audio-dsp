//! `z-audio-synth`: a simple synth runtime (voice management, note handling)
//! built on top of `z-audio-dsp`.
//!
//! Fixed signal chain (Phase 1):
//!
//! ```text
//! MIDI/Event Input -> VoiceManager -> Voice Sum -> 3-band Butterworth EQ -> master gain -> Output
//! ```

#![warn(clippy::all)]

pub mod formula_synth;
pub mod formula_voice;
pub mod midi;
pub mod piano;
pub mod simple_synth;
pub mod voice;
pub mod voice_manager;

pub use formula_synth::{FormulaSynth, FormulaSynthConfig};
pub use formula_voice::{FormulaVoice, FormulaVoiceState};
pub use midi::midi_note_to_hz;
pub use piano::{FormulaPiano, FormulaPianoConfig, PianoParams, PianoVoice, PianoVoiceState};
pub use simple_synth::{SimpleSynth, SimpleSynthConfig};
pub use voice::{Voice, VoiceState};
pub use voice_manager::{VoiceManager, VoiceStealPolicy};
pub use z_audio_dsp::{ParamId, ParamMetadata, ParamUnit};
