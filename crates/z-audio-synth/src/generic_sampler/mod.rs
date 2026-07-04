//! Single-sample general-purpose sampler instrument.
//!
//! Unlike [`crate::vcsl_piano`] (a fixed SFZ-derived region map), this module
//! plays back exactly one loaded sample across the whole keyboard, with
//! root note, tune, offset, velocity response, release time, and stereo
//! width exposed as realtime-automatable parameters. See
//! `docs/汎用サンプラー実装計画.md` in the root repository for the full plan.

mod bank;
mod params;
mod synth;

pub use bank::{BankError, SamplerBank, build_bank_bytes, load_bank_bytes};
pub use params::GenericSamplerParams;
pub use synth::{GenericSampler, GenericSamplerConfig};
