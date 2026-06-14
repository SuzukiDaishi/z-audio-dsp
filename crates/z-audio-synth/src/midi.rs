//! MIDI note number helpers shared by `z-audio-synth` and its examples.

/// Re-exported for convenience so callers don't need to depend on
/// `z-audio-dsp` directly just to convert note numbers to frequencies.
pub use z_audio_dsp::midi_note_to_hz;

/// MIDI note number for C4 ("middle C").
pub const C4: u8 = 60;

/// MIDI note number for A4 (440 Hz, the standard tuning reference).
pub const A4: u8 = 69;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_is_440_hz() {
        assert!((midi_note_to_hz(A4 as f32) - 440.0).abs() < 1e-4);
    }

    #[test]
    fn c4_is_below_a4() {
        assert!(midi_note_to_hz(C4 as f32) < midi_note_to_hz(A4 as f32));
    }
}
