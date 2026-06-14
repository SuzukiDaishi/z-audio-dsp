//! Shared per-block processing context and event types.

use crate::params::ParamId;

/// Per-block context passed to [`crate::Generator`], [`crate::Modulator`],
/// and [`crate::Effect`] implementations.
pub struct ProcessContext<'a> {
    pub sample_rate: f32,
    pub block_size: usize,
    pub tempo_bpm: f32,
    pub events: &'a [TimedEvent],
}

impl<'a> ProcessContext<'a> {
    /// Creates a new process context.
    pub fn new(
        sample_rate: f32,
        block_size: usize,
        tempo_bpm: f32,
        events: &'a [TimedEvent],
    ) -> Self {
        Self {
            sample_rate,
            block_size,
            tempo_bpm,
            events,
        }
    }
}

/// An event scheduled to occur at a specific sample offset within a block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedEvent {
    pub sample_offset: usize,
    pub kind: EventKind,
}

/// The kind of event carried by a [`TimedEvent`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventKind {
    NoteOn { note: u8, velocity: f32 },
    NoteOff { note: u8, velocity: f32 },
    Param { id: ParamId, value: f32 },
}

/// The rate at which a signal is produced/consumed. Reserved for future use,
/// e.g. distinguishing audio-rate from control-rate buffers in a node graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalRate {
    Audio,
    Control,
    Event,
}
