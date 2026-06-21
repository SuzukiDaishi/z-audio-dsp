//! Dynamics processors and shared detector/ballistics utilities.

pub mod ballistics;
pub mod compressor;
pub mod detector;
pub mod limiter;

pub use ballistics::BallisticsFilter;
pub use compressor::{Compressor, CompressorParams, compressor_gain_db};
pub use detector::{DetectorMode, LevelDetector};
pub use limiter::{Limiter, LimiterParams, limiter_gain_db};
