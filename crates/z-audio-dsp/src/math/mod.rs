//! Math utilities: smoothing, interpolation, biquad filters, and shared
//! helper functions used throughout the DSP core.

pub mod biquad;
pub mod interpolation;
pub mod smoothing;

pub use biquad::{
    Biquad, bandpass_coefficients, high_shelf_coefficients, highpass_coefficients,
    low_shelf_coefficients, lowpass_coefficients, peaking_eq_coefficients,
};
pub use interpolation::{clamp, lerp};
pub use smoothing::SmoothedParam;

/// Full turn in radians (`2 * PI`), used for phase-to-radians conversions.
pub const TAU: f32 = core::f32::consts::TAU;

/// Converts a (possibly fractional) MIDI note number to a frequency in Hz,
/// using A4 = MIDI note 69 = 440 Hz.
pub fn midi_note_to_hz(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}

/// Flushes denormal floating point numbers to zero. Used on filter feedback
/// state to avoid CPU slowdowns from subnormal arithmetic.
pub fn flush_denormal(x: f32) -> f32 {
    if x.abs() < 1.0e-20 { 0.0 } else { x }
}

/// Converts decibels to a linear amplitude multiplier.
pub fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Converts a linear amplitude value to decibels, clamping silence to a
/// practical floor to keep dynamics processors numerically stable.
pub fn linear_to_db(linear: f32) -> f32 {
    20.0 * linear.abs().max(1.0e-12).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_note_to_hz_a4_is_440() {
        assert!((midi_note_to_hz(69.0) - 440.0).abs() < 1e-4);
    }

    #[test]
    fn midi_note_to_hz_octave_doubles_frequency() {
        let a4 = midi_note_to_hz(69.0);
        let a5 = midi_note_to_hz(81.0);
        assert!((a5 - a4 * 2.0).abs() < 1e-3);
    }

    #[test]
    fn flush_denormal_zeroes_tiny_values() {
        assert_eq!(flush_denormal(1.0e-25), 0.0);
        assert_eq!(flush_denormal(1.0), 1.0);
        assert_eq!(flush_denormal(-1.0e-25), 0.0);
    }

    #[test]
    fn db_linear_round_trip() {
        for db in [-60.0_f32, -12.0, 0.0, 6.0, 24.0] {
            let round_trip = linear_to_db(db_to_linear(db));
            assert!(
                (round_trip - db).abs() < 1.0e-4,
                "db={db}, got={round_trip}"
            );
        }
    }
}
