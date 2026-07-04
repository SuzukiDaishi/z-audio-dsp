//! Biquad filter implementation (Transposed Direct Form II) plus RBJ "Audio
//! EQ Cookbook" coefficient calculators for filter and EQ shapes used by the
//! DSP effects.

use core::f32::consts::PI;

use super::flush_denormal;

/// A second-order IIR (biquad) filter, normalized so that `a0 == 1`. Holds
/// independent state for a stereo (left/right) pair of channels but shares a
/// single set of coefficients between them.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1_l: f32,
    z2_l: f32,
    z1_r: f32,
    z2_r: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self::identity()
    }
}

impl Biquad {
    /// Creates a passthrough (identity) filter: `y[n] = x[n]`.
    pub fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1_l: 0.0,
            z2_l: 0.0,
            z1_r: 0.0,
            z2_r: 0.0,
        }
    }

    /// Replaces the filter coefficients (`a0` is assumed to already be
    /// normalized to `1.0`). Filter state (`z1`/`z2`) is left untouched so
    /// that coefficient updates don't cause output discontinuities.
    pub fn set_coefficients(&mut self, b0: f32, b1: f32, b2: f32, a1: f32, a2: f32) {
        self.b0 = b0;
        self.b1 = b1;
        self.b2 = b2;
        self.a1 = a1;
        self.a2 = a2;
    }

    /// Resets internal filter state (does not change coefficients).
    pub fn reset_state(&mut self) {
        self.z1_l = 0.0;
        self.z2_l = 0.0;
        self.z1_r = 0.0;
        self.z2_r = 0.0;
    }

    /// Processes one stereo sample using Transposed Direct Form II.
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        let out_l = self.b0 * left + self.z1_l;
        self.z1_l = flush_denormal(self.b1 * left + self.z2_l - self.a1 * out_l);
        self.z2_l = flush_denormal(self.b2 * left - self.a2 * out_l);

        let out_r = self.b0 * right + self.z1_r;
        self.z1_r = flush_denormal(self.b1 * right + self.z2_r - self.a1 * out_r);
        self.z2_r = flush_denormal(self.b2 * right - self.a2 * out_r);

        (out_l, out_r)
    }
}

/// RBJ Audio EQ Cookbook coefficients for a 2nd-order Butterworth low-pass
/// filter, normalized so that `a0 == 1`. Returns `(b0, b1, b2, a1, a2)`.
pub fn lowpass_coefficients(
    frequency_hz: f32,
    q: f32,
    sample_rate: f32,
) -> (f32, f32, f32, f32, f32) {
    let w0 = 2.0 * PI * frequency_hz / sample_rate;
    let cos_w0 = w0.cos();
    let alpha = w0.sin() / (2.0 * q);

    let b0 = (1.0 - cos_w0) / 2.0;
    let b1 = 1.0 - cos_w0;
    let b2 = (1.0 - cos_w0) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
}

/// RBJ Audio EQ Cookbook coefficients for a 2nd-order Butterworth high-pass
/// filter, normalized so that `a0 == 1`. Returns `(b0, b1, b2, a1, a2)`.
pub fn highpass_coefficients(
    frequency_hz: f32,
    q: f32,
    sample_rate: f32,
) -> (f32, f32, f32, f32, f32) {
    let w0 = 2.0 * PI * frequency_hz / sample_rate;
    let cos_w0 = w0.cos();
    let alpha = w0.sin() / (2.0 * q);

    let b0 = (1.0 + cos_w0) / 2.0;
    let b1 = -(1.0 + cos_w0);
    let b2 = (1.0 + cos_w0) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
}

/// RBJ Audio EQ Cookbook coefficients for a 2nd-order band-pass filter with
/// constant 0 dB peak gain, normalized so that `a0 == 1`. Returns
/// `(b0, b1, b2, a1, a2)`.
pub fn bandpass_coefficients(
    frequency_hz: f32,
    q: f32,
    sample_rate: f32,
) -> (f32, f32, f32, f32, f32) {
    let w0 = 2.0 * PI * frequency_hz / sample_rate;
    let cos_w0 = w0.cos();
    let alpha = w0.sin() / (2.0 * q);

    let b0 = alpha;
    let b1 = 0.0;
    let b2 = -alpha;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
}

/// RBJ Audio EQ Cookbook coefficients for a peaking EQ band, normalized so
/// that `a0 == 1`. Returns `(b0, b1, b2, a1, a2)`.
pub fn peaking_eq_coefficients(
    frequency_hz: f32,
    q: f32,
    gain_db: f32,
    sample_rate: f32,
) -> (f32, f32, f32, f32, f32) {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * frequency_hz / sample_rate;
    let cos_w0 = w0.cos();
    let alpha = w0.sin() / (2.0 * q);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos_w0;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha / a;

    (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
}

/// RBJ Audio EQ Cookbook coefficients for a low-shelf EQ band, normalized so
/// that `a0 == 1`. Returns `(b0, b1, b2, a1, a2)`.
pub fn low_shelf_coefficients(
    frequency_hz: f32,
    q: f32,
    gain_db: f32,
    sample_rate: f32,
) -> (f32, f32, f32, f32, f32) {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let sqrt_a = a.sqrt();
    let w0 = 2.0 * PI * frequency_hz / sample_rate;
    let cos_w0 = w0.cos();
    let alpha = w0.sin() / (2.0 * q);
    let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;

    let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
    let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
    let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
    let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
    let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
    let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

    (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
}

/// RBJ Audio EQ Cookbook coefficients for a high-shelf EQ band, normalized so
/// that `a0 == 1`. Returns `(b0, b1, b2, a1, a2)`.
pub fn high_shelf_coefficients(
    frequency_hz: f32,
    q: f32,
    gain_db: f32,
    sample_rate: f32,
) -> (f32, f32, f32, f32, f32) {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let sqrt_a = a.sqrt();
    let w0 = 2.0 * PI * frequency_hz / sample_rate;
    let cos_w0 = w0.cos();
    let alpha = w0.sin() / (2.0 * q);
    let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;

    let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
    let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
    let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
    let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
    let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
    let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

    (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passes_through_unchanged() {
        let mut bq = Biquad::identity();
        for x in [0.0_f32, 0.5, -0.3, 1.0, -1.0] {
            let (l, r) = bq.process(x, x);
            assert_eq!(l, x);
            assert_eq!(r, x);
        }
    }

    #[test]
    fn lowpass_coefficients_are_finite() {
        let (b0, b1, b2, a1, a2) =
            lowpass_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 48_000.0);
        for c in [b0, b1, b2, a1, a2] {
            assert!(c.is_finite());
        }
    }

    #[test]
    fn highpass_coefficients_are_finite() {
        let (b0, b1, b2, a1, a2) =
            highpass_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 48_000.0);
        for c in [b0, b1, b2, a1, a2] {
            assert!(c.is_finite());
        }
    }

    #[test]
    fn bandpass_coefficients_are_finite() {
        let (b0, b1, b2, a1, a2) =
            bandpass_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 48_000.0);
        for c in [b0, b1, b2, a1, a2] {
            assert!(c.is_finite());
        }
    }

    #[test]
    fn eq_coefficients_are_finite() {
        for (b0, b1, b2, a1, a2) in [
            low_shelf_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 6.0, 48_000.0),
            peaking_eq_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, -6.0, 48_000.0),
            high_shelf_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 6.0, 48_000.0),
        ] {
            for c in [b0, b1, b2, a1, a2] {
                assert!(c.is_finite());
            }
        }
    }

    #[test]
    fn zero_gain_eq_coefficients_are_unity_response() {
        for (b0, b1, b2, a1, a2) in [
            low_shelf_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 0.0, 48_000.0),
            peaking_eq_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 0.0, 48_000.0),
            high_shelf_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 0.0, 48_000.0),
        ] {
            assert!((b0 - 1.0).abs() < 1.0e-6, "b0={b0}");
            assert!((b1 - a1).abs() < 1.0e-6, "b1={b1}, a1={a1}");
            assert!((b2 - a2).abs() < 1.0e-6, "b2={b2}, a2={a2}");
        }
    }

    #[test]
    fn lowpass_impulse_response_is_finite() {
        let (b0, b1, b2, a1, a2) =
            lowpass_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 48_000.0);
        let mut bq = Biquad::identity();
        bq.set_coefficients(b0, b1, b2, a1, a2);

        let mut input = vec![0.0_f32; 256];
        input[0] = 1.0;
        for &x in &input {
            let (l, _r) = bq.process(x, x);
            assert!(l.is_finite());
        }
    }

    #[test]
    fn highpass_impulse_response_is_finite() {
        let (b0, b1, b2, a1, a2) =
            highpass_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 48_000.0);
        let mut bq = Biquad::identity();
        bq.set_coefficients(b0, b1, b2, a1, a2);

        let mut input = vec![0.0_f32; 256];
        input[0] = 1.0;
        for &x in &input {
            let (l, _r) = bq.process(x, x);
            assert!(l.is_finite());
        }
    }

    #[test]
    fn bandpass_impulse_response_is_finite() {
        let (b0, b1, b2, a1, a2) =
            bandpass_coefficients(1000.0, core::f32::consts::FRAC_1_SQRT_2, 48_000.0);
        let mut bq = Biquad::identity();
        bq.set_coefficients(b0, b1, b2, a1, a2);

        let mut input = vec![0.0_f32; 256];
        input[0] = 1.0;
        for &x in &input {
            let (l, _r) = bq.process(x, x);
            assert!(l.is_finite());
        }
    }
}
