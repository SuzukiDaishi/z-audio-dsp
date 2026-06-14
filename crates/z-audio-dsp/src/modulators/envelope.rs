//! ADSR amplitude envelope.

use crate::Modulator;
use crate::context::ProcessContext;

/// The current stage of an [`Envelope`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// The shape of an envelope stage's transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnvelopeCurve {
    Linear,
    #[default]
    Exponential,
}

impl EnvelopeCurve {
    /// Number of valid `ParamId::EnvCurve` automation values.
    pub const VARIANT_COUNT: u32 = 2;

    /// Decodes a `ParamId::EnvCurve` automation value, rounding to the
    /// nearest integer and clamping to `0..VARIANT_COUNT - 1`.
    pub fn from_param_value(value: f32) -> Self {
        match value.round().clamp(0.0, (Self::VARIANT_COUNT - 1) as f32) as u32 {
            0 => Self::Linear,
            _ => Self::Exponential,
        }
    }

    /// Encodes this curve as a `ParamId::EnvCurve` automation value.
    pub fn to_param_value(self) -> f32 {
        match self {
            Self::Linear => 0.0,
            Self::Exponential => 1.0,
        }
    }
}

/// ADSR envelope parameters. `attack`/`decay`/`release` are in seconds;
/// `sustain` is a level in `[0.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvelopeParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub curve: EnvelopeCurve,
}

impl Default for EnvelopeParams {
    fn default() -> Self {
        Self {
            attack: 0.01,
            decay: 0.1,
            sustain: 0.7,
            release: 0.2,
            curve: EnvelopeCurve::Exponential,
        }
    }
}

/// Distance from a stage's target below which the stage is considered
/// complete and the envelope advances to the next stage.
const STAGE_EPSILON: f32 = 1.0e-3;

/// Computes a one-pole coefficient such that, starting at distance `1.0` from
/// the target, the remaining distance shrinks to [`STAGE_EPSILON`] after
/// `time_seconds` (i.e. `coeff ^ (time_seconds * sample_rate) == STAGE_EPSILON`).
/// Returns `0.0` for non-positive durations, which causes the very next
/// sample to land exactly on the target.
fn stage_exp_coeff(time_seconds: f32, sample_rate: f32) -> f32 {
    if time_seconds <= 0.0 {
        0.0
    } else {
        let n = time_seconds * sample_rate;
        (STAGE_EPSILON.ln() / n).exp()
    }
}

/// A standard ADSR amplitude envelope.
///
/// `Idle -> (note_on) -> Attack -> Decay -> Sustain -> (note_off) -> Release -> Idle`
///
/// Re-triggering via [`Envelope::note_on`] while in [`EnvelopeState::Release`]
/// (or any other state) starts a new Attack stage from the *current* value,
/// avoiding discontinuities ("clicks").
pub struct Envelope {
    params: EnvelopeParams,
    sample_rate: f32,
    state: EnvelopeState,
    value: f32,
    release_start_value: f32,
    attack_exp_coeff: f32,
    decay_exp_coeff: f32,
    release_exp_coeff: f32,
    linear_increment: f32,
}

impl Envelope {
    /// Creates a new envelope, initially [`EnvelopeState::Idle`] with
    /// `value == 0.0`.
    pub fn new(params: EnvelopeParams) -> Self {
        let mut env = Self {
            params,
            sample_rate: 48_000.0,
            state: EnvelopeState::Idle,
            value: 0.0,
            release_start_value: 0.0,
            attack_exp_coeff: 0.0,
            decay_exp_coeff: 0.0,
            release_exp_coeff: 0.0,
            linear_increment: 0.0,
        };
        env.recompute_exp_coeffs();
        env
    }

    /// Replaces the envelope parameters and recomputes internal
    /// coefficients. Does not change the current stage or value.
    pub fn set_params(&mut self, params: EnvelopeParams) {
        self.params = params;
        self.recompute_exp_coeffs();
    }

    /// Returns the current stage.
    pub fn state(&self) -> EnvelopeState {
        self.state
    }

    /// Returns the current envelope value in `[0.0, 1.0]`.
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Starts (or restarts) the Attack stage from the current value.
    pub fn note_on(&mut self) {
        self.enter_stage(EnvelopeState::Attack, 1.0, self.params.attack);
    }

    /// Starts the Release stage from the current value.
    pub fn note_off(&mut self) {
        self.release_start_value = self.value;
        self.enter_stage(EnvelopeState::Release, 0.0, self.params.release);
    }

    fn recompute_exp_coeffs(&mut self) {
        self.attack_exp_coeff = stage_exp_coeff(self.params.attack, self.sample_rate);
        self.decay_exp_coeff = stage_exp_coeff(self.params.decay, self.sample_rate);
        self.release_exp_coeff = stage_exp_coeff(self.params.release, self.sample_rate);
    }

    /// Enters `state`, targeting `target` over `time_seconds`, computing the
    /// linear increment from the current value (used only for
    /// [`EnvelopeCurve::Linear`]).
    fn enter_stage(&mut self, state: EnvelopeState, target: f32, time_seconds: f32) {
        self.state = state;
        self.linear_increment = if time_seconds <= 0.0 {
            target - self.value
        } else {
            (target - self.value) / (time_seconds * self.sample_rate)
        };
    }

    /// Moves `value` one step toward `target` using the configured curve and
    /// the given exponential coefficient (ignored for [`EnvelopeCurve::Linear`]).
    fn advance_toward(&mut self, target: f32, exp_coeff: f32) {
        match self.params.curve {
            EnvelopeCurve::Exponential => {
                self.value = target + (self.value - target) * exp_coeff;
            }
            EnvelopeCurve::Linear => {
                self.value += self.linear_increment;
                let overshoot = (self.linear_increment >= 0.0 && self.value >= target)
                    || (self.linear_increment <= 0.0 && self.value <= target);
                if overshoot {
                    self.value = target;
                }
            }
        }
    }

    /// Advances the envelope by one sample and returns the new value.
    pub fn next_sample(&mut self) -> f32 {
        match self.state {
            EnvelopeState::Idle => {
                self.value = 0.0;
            }
            EnvelopeState::Attack => {
                self.advance_toward(1.0, self.attack_exp_coeff);
                if (self.value - 1.0).abs() <= STAGE_EPSILON {
                    self.value = 1.0;
                    let sustain = self.params.sustain;
                    let decay = self.params.decay;
                    self.enter_stage(EnvelopeState::Decay, sustain, decay);
                }
            }
            EnvelopeState::Decay => {
                let target = self.params.sustain;
                self.advance_toward(target, self.decay_exp_coeff);
                if (self.value - target).abs() <= STAGE_EPSILON {
                    self.value = target;
                    self.state = EnvelopeState::Sustain;
                }
            }
            EnvelopeState::Sustain => {
                self.value = self.params.sustain;
            }
            EnvelopeState::Release => {
                self.advance_toward(0.0, self.release_exp_coeff);
                if self.value.abs() <= STAGE_EPSILON {
                    self.value = 0.0;
                    self.state = EnvelopeState::Idle;
                }
            }
        }
        self.value
    }
}

impl Modulator for Envelope {
    fn prepare(&mut self, sample_rate: f32, _max_block_size: usize) {
        debug_assert!(sample_rate > 0.0);
        self.sample_rate = sample_rate;
        self.recompute_exp_coeffs();
    }

    fn reset(&mut self) {
        self.state = EnvelopeState::Idle;
        self.value = 0.0;
        self.release_start_value = 0.0;
        self.linear_increment = 0.0;
    }

    fn process_control(&mut self, _ctx: &ProcessContext, out: &mut [f32]) {
        for o in out.iter_mut() {
            *o = self.next_sample();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast_params(curve: EnvelopeCurve) -> EnvelopeParams {
        EnvelopeParams {
            attack: 0.001,
            decay: 0.001,
            sustain: 0.5,
            release: 0.001,
            curve,
        }
    }

    #[test]
    fn state_transitions_through_full_cycle() {
        let mut env = Envelope::new(fast_params(EnvelopeCurve::Exponential));
        env.prepare(48_000.0, 128);

        env.note_on();
        assert_eq!(env.state(), EnvelopeState::Attack);

        for _ in 0..1000 {
            env.next_sample();
            if env.state() == EnvelopeState::Decay {
                break;
            }
        }
        assert_eq!(env.state(), EnvelopeState::Decay);

        for _ in 0..1000 {
            env.next_sample();
            if env.state() == EnvelopeState::Sustain {
                break;
            }
        }
        assert_eq!(env.state(), EnvelopeState::Sustain);
        assert!((env.value() - 0.5).abs() < 1e-3);

        env.note_off();
        assert_eq!(env.state(), EnvelopeState::Release);

        for _ in 0..1000 {
            env.next_sample();
            if env.state() == EnvelopeState::Idle {
                break;
            }
        }
        assert_eq!(env.state(), EnvelopeState::Idle);
        assert_eq!(env.value(), 0.0);
    }

    #[test]
    fn renote_on_during_release_has_no_discontinuity() {
        let mut env = Envelope::new(EnvelopeParams {
            attack: 0.01,
            decay: 0.05,
            sustain: 0.6,
            release: 0.5,
            curve: EnvelopeCurve::Exponential,
        });
        env.prepare(48_000.0, 128);

        env.note_on();
        for _ in 0..10_000 {
            env.next_sample();
        }
        assert_eq!(env.state(), EnvelopeState::Sustain);

        env.note_off();
        for _ in 0..100 {
            env.next_sample();
        }
        let before = env.value();
        env.note_on();
        let after = env.next_sample();
        assert!(
            (after - before).abs() < 0.01,
            "before={before}, after={after}"
        );
        assert_eq!(env.state(), EnvelopeState::Attack);
    }

    #[test]
    fn linear_curve_reaches_targets() {
        let mut env = Envelope::new(EnvelopeParams {
            attack: 0.01,
            decay: 0.01,
            sustain: 0.3,
            release: 0.01,
            curve: EnvelopeCurve::Linear,
        });
        env.prepare(48_000.0, 128);
        env.note_on();
        for _ in 0..48_000 {
            env.next_sample();
        }
        assert_eq!(env.state(), EnvelopeState::Sustain);
        assert!((env.value() - 0.3).abs() < 1e-3);
    }

    #[test]
    fn envelope_curve_param_value_round_trips() {
        for curve in [EnvelopeCurve::Linear, EnvelopeCurve::Exponential] {
            let encoded = curve.to_param_value();
            assert_eq!(EnvelopeCurve::from_param_value(encoded), curve);
        }
    }

    #[test]
    fn envelope_curve_from_param_value_clamps_out_of_range() {
        assert_eq!(EnvelopeCurve::from_param_value(-1.0), EnvelopeCurve::Linear);
        assert_eq!(
            EnvelopeCurve::from_param_value(100.0),
            EnvelopeCurve::Exponential
        );
    }

    #[test]
    fn envelope_curve_default_matches_exponential_encoding() {
        assert_eq!(EnvelopeCurve::default(), EnvelopeCurve::Exponential);
        assert_eq!(EnvelopeCurve::default().to_param_value(), 1.0);
    }

    #[test]
    fn zero_length_stages_do_not_produce_nan() {
        let mut env = Envelope::new(EnvelopeParams {
            attack: 0.0,
            decay: 0.0,
            sustain: 0.8,
            release: 0.0,
            curve: EnvelopeCurve::Linear,
        });
        env.prepare(48_000.0, 128);
        env.note_on();
        for _ in 0..10 {
            assert!(env.next_sample().is_finite());
        }
        env.note_off();
        for _ in 0..10 {
            assert!(env.next_sample().is_finite());
        }
    }
}
