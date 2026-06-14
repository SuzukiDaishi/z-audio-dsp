//! Control-rate modulators: amplitude envelope and LFO.
//!
//! Phase 1 uses fixed routing only (no arbitrary modulation graph): each
//! voice has one amp envelope and one LFO, and recursive modulation
//! (Env -> Env, LFO -> Env, LFO -> LFO) is out of scope.

pub mod envelope;
pub mod lfo;

pub use envelope::{Envelope, EnvelopeCurve, EnvelopeParams, EnvelopeState};
pub use lfo::{Lfo, LfoParams, LfoTarget, LfoWaveform};
