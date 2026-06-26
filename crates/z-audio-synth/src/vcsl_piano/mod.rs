//! Sample-based piano built from VCSL Keys SFZ/FLAC material.
//!
//! See `docs/VCSLサンプラーピアノ実装計画.md` in the root repository for the
//! full implementation plan. This module owns instrument-level state (note
//! handling, params, bank format); per-voice playback and mixing live in
//! `z_audio_dsp::sampler`.

mod bank;
mod params;
mod synth;

pub use bank::{BankError, VcslRegionSource, VcslSampleBank, build_bank_bytes, load_bank_bytes};
pub use params::VcslPianoParams;
pub use synth::{VcslPiano, VcslPianoConfig};
