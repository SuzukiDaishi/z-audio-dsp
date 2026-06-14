//! `z-audio-dsp`: a pure DSP library providing audio-rate generators,
//! control-rate modulators, and stereo effects for building synthesizers.
//!
//! ## Core abstractions
//!
//! - [`Generator`]: produces an audio-rate mono signal.
//! - [`Modulator`]: produces a control-rate signal (e.g. an envelope or LFO).
//! - [`Effect`]: processes a stereo audio signal in place.
//!
//! All three share the same lifecycle: [`prepare`](Generator::prepare) is
//! called once (or whenever the sample rate / max block size changes),
//! [`reset`](Generator::reset) clears internal state (e.g. on note-on), and
//! the `process_*` method runs once per block. None of these methods may
//! allocate, panic, lock, or perform I/O — see
//! `docs/z-audio-dsp_dsp_plan/08_realtime_safety.md`.

#![warn(clippy::all)]

pub mod buffer;
pub mod context;
pub mod effects;
pub mod generators;
pub mod math;
pub mod modulators;
pub mod params;

pub use buffer::AudioBuffer;
pub use context::{EventKind, ProcessContext, SignalRate, TimedEvent};
pub use effects::{BUTTERWORTH_Q, ButterworthBand, ButterworthKind, Gain, ThreeBandButterworthEq};
pub use generators::{
    AnalogGenerator, AnalogWaveform, GeneratorInstance, GeneratorKind, GeneratorParams,
    NoiseGenerator,
};
pub use math::{flush_denormal, midi_note_to_hz};
pub use modulators::{
    Envelope, EnvelopeCurve, EnvelopeParams, EnvelopeState, Lfo, LfoParams, LfoTarget, LfoWaveform,
};
pub use params::{ParamId, ParamMetadata, ParamUnit};

/// Produces an audio-rate mono signal (oscillators, noise sources, etc.).
pub trait Generator {
    /// Called once before processing begins, and again whenever the sample
    /// rate or maximum block size changes. May allocate.
    fn prepare(&mut self, sample_rate: f32, max_block_size: usize);

    /// Resets internal state (e.g. oscillator phase) to its initial value.
    /// Must not allocate.
    fn reset(&mut self);

    /// Fills `out` with `out.len()` newly generated samples. Must not
    /// allocate, panic, lock, or perform I/O.
    fn process_mono(&mut self, ctx: &ProcessContext, out: &mut [f32]);
}

/// Produces a control-rate signal (envelopes, LFOs, etc.).
pub trait Modulator {
    /// Called once before processing begins, and again whenever the sample
    /// rate or maximum block size changes. May allocate.
    fn prepare(&mut self, sample_rate: f32, max_block_size: usize);

    /// Resets internal state to its initial value. Must not allocate.
    fn reset(&mut self);

    /// Fills `out` with `out.len()` newly generated control values. Must not
    /// allocate, panic, lock, or perform I/O.
    fn process_control(&mut self, ctx: &ProcessContext, out: &mut [f32]);
}

/// Processes a stereo audio signal in place (gain, filters, etc.).
pub trait Effect {
    /// Called once before processing begins, and again whenever the sample
    /// rate or maximum block size changes. May allocate.
    fn prepare(&mut self, sample_rate: f32, max_block_size: usize);

    /// Resets internal state (e.g. filter memory) to its initial value. Must
    /// not allocate.
    fn reset(&mut self);

    /// Processes `left` and `right` in place. `left.len() == right.len()`.
    /// Must not allocate, panic, lock, or perform I/O.
    fn process_stereo(&mut self, ctx: &ProcessContext, left: &mut [f32], right: &mut [f32]);
}
