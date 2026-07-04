//! Audio effects: gain, EQ, and diffuser.

pub mod butterworth_eq;
pub mod diffuser;
pub mod gain;

pub use butterworth_eq::{BUTTERWORTH_Q, ButterworthBand, ButterworthKind, ThreeBandButterworthEq};
pub use diffuser::{Diffuser, DiffuserParams};
pub use gain::Gain;
