//! Audio effects: gain and the 3-band Butterworth EQ.

pub mod butterworth_eq;
pub mod gain;

pub use butterworth_eq::{BUTTERWORTH_Q, ButterworthBand, ButterworthKind, ThreeBandButterworthEq};
pub use gain::Gain;
