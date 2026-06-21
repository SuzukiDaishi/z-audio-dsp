//! Modal resonators for physical/modelled instruments.

pub mod biquad_resonator;
pub mod body;
pub mod modal_bank;

pub use biquad_resonator::BiquadResonator;
pub use body::BodyResonator;
pub use modal_bank::{ModalBank, ModalMode};
