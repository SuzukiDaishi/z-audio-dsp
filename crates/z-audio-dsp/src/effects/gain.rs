//! Simple stereo gain effect with smoothed parameter changes.

use crate::Effect;
use crate::context::ProcessContext;
use crate::math::SmoothedParam;

/// Time constant for gain smoothing, in seconds.
const SMOOTHING_TAU: f32 = 0.005;

/// A stereo gain stage. Gain changes are smoothed over [`SMOOTHING_TAU`]
/// seconds to avoid clicks.
pub struct Gain {
    gain: SmoothedParam,
}

impl Gain {
    /// Creates a new gain stage with an initial linear gain of `gain`.
    pub fn new(gain: f32) -> Self {
        let mut param = SmoothedParam::new(gain);
        param.configure(48_000.0, SMOOTHING_TAU);
        Self { gain: param }
    }

    /// Sets the target linear gain. The change is smoothed over
    /// [`SMOOTHING_TAU`] seconds.
    pub fn set_gain(&mut self, gain: f32) {
        self.gain.set_target(gain);
    }

    /// Returns the current (smoothed) linear gain.
    pub fn current_gain(&self) -> f32 {
        self.gain.current()
    }

    /// Returns the target linear gain set via [`Gain::set_gain`], ignoring
    /// in-progress smoothing.
    pub fn target_gain(&self) -> f32 {
        self.gain.target()
    }
}

impl Default for Gain {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl Effect for Gain {
    fn prepare(&mut self, sample_rate: f32, _max_block_size: usize) {
        debug_assert!(sample_rate > 0.0);
        self.gain.configure(sample_rate, SMOOTHING_TAU);
    }

    fn reset(&mut self) {
        self.gain.set_immediate(self.gain.target());
    }

    fn process_stereo(&mut self, _ctx: &ProcessContext, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let g = self.gain.tick();
            *l *= g;
            *r *= g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(gain: &mut Gain, left: &mut [f32], right: &mut [f32]) {
        let events = [];
        let ctx = ProcessContext::new(48_000.0, left.len(), 120.0, &events);
        gain.process_stereo(&ctx, left, right);
    }

    #[test]
    fn unity_gain_passes_through() {
        let mut gain = Gain::new(1.0);
        gain.prepare(48_000.0, 128);

        let mut left = [0.5_f32, -0.3, 1.0, -1.0];
        let mut right = left;
        process(&mut gain, &mut left, &mut right);

        for (sample, expected) in left.iter().zip([0.5_f32, -0.3, 1.0, -1.0]) {
            assert!((sample - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn target_gain_reflects_set_gain_immediately() {
        let mut gain = Gain::new(1.0);
        gain.prepare(48_000.0, 128);
        gain.set_gain(0.25);

        // Unlike `current_gain`, `target_gain` is not smoothed.
        assert_eq!(gain.target_gain(), 0.25);
    }

    #[test]
    fn gain_change_is_smoothed_not_instant() {
        let mut gain = Gain::new(1.0);
        gain.prepare(48_000.0, 128);
        gain.set_gain(0.0);

        let mut left = [1.0_f32];
        let mut right = [1.0_f32];
        process(&mut gain, &mut left, &mut right);

        // After just one sample, gain should not have jumped straight to 0.
        assert!(left[0] > 0.0);
    }

    #[test]
    fn zero_gain_eventually_silences_signal() {
        let mut gain = Gain::new(1.0);
        gain.prepare(48_000.0, 128);
        gain.set_gain(0.0);

        for _ in 0..2000 {
            let mut left = [1.0_f32];
            let mut right = [1.0_f32];
            process(&mut gain, &mut left, &mut right);
        }
        assert!(gain.current_gain() < 1e-3);
    }
}
