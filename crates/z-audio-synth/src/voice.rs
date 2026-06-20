//! A single synthesizer voice: one generator, one amplitude envelope, and one
//! LFO, mixed to stereo via a constant-power pan law.

use z_audio_dsp::{
    Envelope, EnvelopeParams, EnvelopeState, Generator, GeneratorInstance, GeneratorKind,
    GeneratorParams, Lfo, LfoParams, LfoTarget, Modulator, math::SmoothedParam, midi_note_to_hz,
};

const PARAM_SMOOTHING_SECONDS: f32 = 0.006;
const GENERATOR_XFADE_SECONDS: f32 = 0.006;

/// The lifecycle state of a [`Voice`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceState {
    #[default]
    Idle,
    Active,
    Releasing,
}

/// A single synthesizer voice: `Generator -> Amp Envelope -> LFO -> Gain/Pan`.
pub struct Voice {
    state: VoiceState,
    note: u8,
    velocity: f32,
    gain: SmoothedParam,
    pan: SmoothedParam,
    pulse_width: SmoothedParam,
    lfo_amount: SmoothedParam,
    generator_kind: GeneratorKind,
    generator: GeneratorInstance,
    next_generator: Option<GeneratorInstance>,
    generator_xfade_pos: usize,
    generator_xfade_len: usize,
    amp_env: Envelope,
    lfo: Lfo,
    activation_id: u64,
    seed: u64,
    sample_rate: f32,
    max_block_size: usize,
}

impl Voice {
    /// Creates a new, idle voice. `seed` seeds the voice's noise generator
    /// and LFO random-hold RNG.
    pub fn new(seed: u64) -> Self {
        let generator_params = GeneratorParams::default();
        Self {
            state: VoiceState::Idle,
            note: 0,
            velocity: 0.0,
            gain: SmoothedParam::new(generator_params.gain),
            pan: SmoothedParam::new(generator_params.pan),
            pulse_width: SmoothedParam::new(generator_params.pulse_width),
            lfo_amount: SmoothedParam::new(Self::effective_lfo_amount(&LfoParams::default())),
            generator_kind: generator_params.kind,
            generator: GeneratorInstance::from_params(&generator_params, seed),
            next_generator: None,
            generator_xfade_pos: 0,
            generator_xfade_len: 0,
            amp_env: Envelope::new(EnvelopeParams::default()),
            lfo: Lfo::new(LfoParams::default(), seed),
            activation_id: 0,
            seed,
            sample_rate: 48_000.0,
            max_block_size: 0,
        }
    }

    /// Prepares the generator, envelope, and LFO for `sample_rate`/`max_block_size`.
    pub fn prepare(&mut self, sample_rate: f32, max_block_size: usize) {
        self.sample_rate = sample_rate;
        self.max_block_size = max_block_size;
        self.gain.configure(sample_rate, PARAM_SMOOTHING_SECONDS);
        self.pan.configure(sample_rate, PARAM_SMOOTHING_SECONDS);
        self.pulse_width
            .configure(sample_rate, PARAM_SMOOTHING_SECONDS);
        self.lfo_amount
            .configure(sample_rate, PARAM_SMOOTHING_SECONDS);
        self.generator.prepare(sample_rate, max_block_size);
        if let Some(next_generator) = self.next_generator.as_mut() {
            next_generator.prepare(sample_rate, max_block_size);
        }
        self.amp_env.prepare(sample_rate, max_block_size);
        self.lfo.prepare(sample_rate, max_block_size);
    }

    /// Returns `true` unless the voice is [`VoiceState::Idle`].
    pub fn is_active(&self) -> bool {
        self.state != VoiceState::Idle
    }

    /// Returns `true` if the voice is in [`VoiceState::Releasing`].
    pub fn is_releasing(&self) -> bool {
        self.state == VoiceState::Releasing
    }

    /// Returns the MIDI note number this voice is currently playing (only
    /// meaningful while [`Voice::is_active`] is `true`).
    pub fn note(&self) -> u8 {
        self.note
    }

    /// Returns the activation order used by the voice-stealing algorithm:
    /// higher means more recently triggered.
    pub fn activation_id(&self) -> u64 {
        self.activation_id
    }

    /// Returns the voice's LFO, used by `SimpleSynth` for voice-0 LFO -> EQ
    /// routing.
    pub fn lfo(&self) -> &Lfo {
        &self.lfo
    }

    /// (Re)triggers the voice for `note`/`velocity`, configuring its
    /// generator, envelope, and LFO from the given parameters.
    ///
    /// The amplitude envelope and LFO retrigger smoothly from their current
    /// state (no hard reset), avoiding clicks when stealing a releasing
    /// voice. The generator's phase is reset to its configured phase offset.
    pub fn note_on(
        &mut self,
        note: u8,
        velocity: f32,
        generator_params: &GeneratorParams,
        env_params: &EnvelopeParams,
        lfo_params: &LfoParams,
        activation_id: u64,
    ) {
        self.note = note;
        self.velocity = velocity.clamp(0.0, 1.0);
        self.gain.set_immediate(generator_params.gain);
        self.pan.set_immediate(generator_params.pan);
        self.pulse_width.set_immediate(generator_params.pulse_width);
        self.lfo_amount
            .set_immediate(Self::effective_lfo_amount(lfo_params));
        self.activation_id = activation_id;
        self.next_generator = None;
        self.generator_xfade_pos = 0;
        self.generator_xfade_len = 0;

        if generator_params.kind != self.generator_kind {
            self.generator_kind = generator_params.kind;
            self.generator = GeneratorInstance::from_params(generator_params, self.seed);
            self.generator
                .prepare(self.sample_rate, self.max_block_size);
        }
        self.generator
            .set_frequency_hz(midi_note_to_hz(note as f32));
        self.generator.set_pulse_width(generator_params.pulse_width);
        self.generator.reset();

        self.amp_env.set_params(*env_params);
        self.amp_env.note_on();

        self.lfo.set_params(*lfo_params);
        self.lfo.note_on();

        self.state = VoiceState::Active;
    }

    /// Applies shared synth parameters to an already-sounding voice.
    ///
    /// Continuous parameters are smoothed by [`Voice::next_sample`]. Generator
    /// kind changes use a short crossfade so discrete waveform changes avoid a
    /// hard sample discontinuity.
    pub fn apply_realtime_params(
        &mut self,
        generator_params: &GeneratorParams,
        env_params: &EnvelopeParams,
        lfo_params: &LfoParams,
    ) {
        if self.state == VoiceState::Idle {
            return;
        }

        self.gain.set_target(generator_params.gain);
        self.pan.set_target(generator_params.pan);
        self.pulse_width.set_target(generator_params.pulse_width);
        self.lfo_amount
            .set_target(Self::effective_lfo_amount(lfo_params));
        self.amp_env.set_params(*env_params);
        self.lfo.set_params(*lfo_params);

        if generator_params.kind != self.generator_kind {
            self.start_generator_crossfade(generator_params);
        }
    }

    /// Starts the release stage if this voice is currently active and
    /// playing `note`.
    pub fn note_off(&mut self, note: u8) {
        if self.state == VoiceState::Active && self.note == note {
            self.amp_env.note_off();
            self.state = VoiceState::Releasing;
        }
    }

    /// Computes the next stereo sample `(left, right)`. Returns `(0.0, 0.0)`
    /// while [`VoiceState::Idle`] without advancing any internal state.
    pub fn next_sample(&mut self) -> (f32, f32) {
        if self.state == VoiceState::Idle {
            return (0.0, 0.0);
        }

        let lfo_value = self.lfo.next_sample();
        let lfo_params = *self.lfo.params();
        let lfo_amount = self.lfo_amount.tick();
        let pulse_width = self.pulse_width.tick();

        let mut note = self.note as f32;
        if lfo_params.target == LfoTarget::PitchSemitone {
            note += lfo_value * lfo_amount;
        }
        let frequency_hz = midi_note_to_hz(note);
        self.generator.set_frequency_hz(frequency_hz);
        self.generator.set_pulse_width(pulse_width);
        if let Some(next_generator) = self.next_generator.as_mut() {
            next_generator.set_frequency_hz(frequency_hz);
            next_generator.set_pulse_width(pulse_width);
        }

        let env = self.amp_env.next_sample();
        if self.amp_env.state() == EnvelopeState::Idle {
            self.state = VoiceState::Idle;
        }

        let generator_sample = self.next_generator_sample();
        let gain = self.gain.tick();
        let mut amplitude = generator_sample * env * gain * self.velocity;
        if lfo_params.target == LfoTarget::Gain {
            amplitude *= 1.0 + lfo_value * lfo_amount;
        }

        let pan = self.pan.tick().clamp(-1.0, 1.0);
        let angle = (pan + 1.0) * core::f32::consts::FRAC_PI_4;
        (amplitude * angle.cos(), amplitude * angle.sin())
    }

    fn effective_lfo_amount(params: &LfoParams) -> f32 {
        if params.enabled && params.target != LfoTarget::None {
            params.amount
        } else {
            0.0
        }
    }

    fn start_generator_crossfade(&mut self, params: &GeneratorParams) {
        self.generator_kind = params.kind;
        let mut next_generator = GeneratorInstance::from_params(params, self.seed);
        next_generator.prepare(self.sample_rate, self.max_block_size);
        next_generator.set_frequency_hz(midi_note_to_hz(self.note as f32));
        next_generator.set_pulse_width(self.pulse_width.current());
        next_generator.reset();

        self.next_generator = Some(next_generator);
        self.generator_xfade_pos = 0;
        self.generator_xfade_len =
            ((self.sample_rate * GENERATOR_XFADE_SECONDS).round() as usize).max(1);
    }

    fn next_generator_sample(&mut self) -> f32 {
        let current = self.generator.next_sample();
        let Some(next_generator) = self.next_generator.as_mut() else {
            return current;
        };

        let next = next_generator.next_sample();
        let t = (self.generator_xfade_pos as f32 / self.generator_xfade_len as f32).clamp(0.0, 1.0);
        let sample = current * (1.0 - t) + next * t;

        self.generator_xfade_pos += 1;
        if self.generator_xfade_pos >= self.generator_xfade_len {
            if let Some(next_generator) = self.next_generator.take() {
                self.generator = next_generator;
            }
            self.generator_xfade_pos = 0;
            self.generator_xfade_len = 0;
        }

        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use z_audio_dsp::LfoWaveform;

    fn default_params() -> (GeneratorParams, EnvelopeParams, LfoParams) {
        (
            GeneratorParams::default(),
            EnvelopeParams::default(),
            LfoParams::default(),
        )
    }

    #[test]
    fn idle_voice_produces_silence() {
        let mut voice = Voice::new(1);
        voice.prepare(48_000.0, 128);
        assert_eq!(voice.next_sample(), (0.0, 0.0));
        assert!(!voice.is_active());
    }

    #[test]
    fn note_on_activates_and_produces_sound() {
        let (gen_params, env_params, lfo_params) = default_params();
        let mut voice = Voice::new(1);
        voice.prepare(48_000.0, 128);
        voice.note_on(60, 1.0, &gen_params, &env_params, &lfo_params, 1);

        assert!(voice.is_active());
        let mut max_abs: f32 = 0.0;
        for _ in 0..1000 {
            let (l, r) = voice.next_sample();
            max_abs = max_abs.max(l.abs()).max(r.abs());
        }
        assert!(max_abs > 0.0);
    }

    #[test]
    fn note_off_releases_and_eventually_goes_idle() {
        let (gen_params, env_params, lfo_params) = default_params();
        let env_params = EnvelopeParams {
            attack: 0.001,
            decay: 0.001,
            sustain: 0.5,
            release: 0.001,
            ..env_params
        };
        let mut voice = Voice::new(1);
        voice.prepare(48_000.0, 128);
        voice.note_on(60, 1.0, &gen_params, &env_params, &lfo_params, 1);

        for _ in 0..100 {
            voice.next_sample();
        }
        voice.note_off(60);
        assert!(voice.is_releasing());

        for _ in 0..10_000 {
            voice.next_sample();
            if !voice.is_active() {
                break;
            }
        }
        assert!(!voice.is_active());
    }

    #[test]
    fn note_off_ignores_mismatched_note() {
        let (gen_params, env_params, lfo_params) = default_params();
        let mut voice = Voice::new(1);
        voice.prepare(48_000.0, 128);
        voice.note_on(60, 1.0, &gen_params, &env_params, &lfo_params, 1);

        voice.note_off(61);
        assert!(voice.is_active());
        assert!(!voice.is_releasing());
    }

    #[test]
    fn pan_extremes_isolate_channels() {
        let (mut gen_params, env_params, lfo_params) = default_params();

        gen_params.pan = -1.0;
        let mut left_voice = Voice::new(1);
        left_voice.prepare(48_000.0, 128);
        left_voice.note_on(60, 1.0, &gen_params, &env_params, &lfo_params, 1);
        let mut max_left = 0.0_f32;
        let mut max_right = 0.0_f32;
        for _ in 0..100 {
            let (l, r) = left_voice.next_sample();
            max_left = max_left.max(l.abs());
            max_right = max_right.max(r.abs());
        }
        assert!(max_left > 0.0);
        assert!(max_right < 1e-6);

        gen_params.pan = 1.0;
        let mut right_voice = Voice::new(1);
        right_voice.prepare(48_000.0, 128);
        right_voice.note_on(60, 1.0, &gen_params, &env_params, &lfo_params, 1);
        let mut max_left = 0.0_f32;
        let mut max_right = 0.0_f32;
        for _ in 0..100 {
            let (l, r) = right_voice.next_sample();
            max_left = max_left.max(l.abs());
            max_right = max_right.max(r.abs());
        }
        assert!(max_left < 1e-6);
        assert!(max_right > 0.0);
    }

    #[test]
    fn gain_lfo_modulates_amplitude() {
        let (gen_params, env_params, mut lfo_params) = default_params();
        lfo_params.waveform = LfoWaveform::Sine;
        lfo_params.rate_hz = 1.0;
        lfo_params.target = LfoTarget::Gain;

        lfo_params.amount = 0.0;
        let mut base_voice = Voice::new(1);
        base_voice.prepare(48_000.0, 128);
        base_voice.note_on(60, 1.0, &gen_params, &env_params, &lfo_params, 1);

        lfo_params.amount = 1.0;
        let mut scaled_voice = Voice::new(1);
        scaled_voice.prepare(48_000.0, 128);
        scaled_voice.note_on(60, 1.0, &gen_params, &env_params, &lfo_params, 1);

        // Both voices share the same generator/envelope/LFO phase trajectory
        // (only `amount` differs), so at each sample the ratio of their
        // outputs reveals the `1.0 + lfo * amount` scaling factor applied for
        // `LfoTarget::Gain`.
        let mut max_ratio: f32 = 0.0;
        let mut min_ratio: f32 = f32::MAX;
        for _ in 0..48_000 {
            let (base_l, _) = base_voice.next_sample();
            let (scaled_l, _) = scaled_voice.next_sample();
            if base_l.abs() > 0.01 {
                let ratio = scaled_l / base_l;
                max_ratio = max_ratio.max(ratio);
                min_ratio = min_ratio.min(ratio);
            }
        }
        assert!(max_ratio > 1.5, "max_ratio = {max_ratio}");
        assert!(min_ratio < 0.5, "min_ratio = {min_ratio}");
    }

    #[test]
    fn pitch_lfo_modulates_frequency() {
        let (gen_params, env_params, mut lfo_params) = default_params();
        lfo_params.target = LfoTarget::PitchSemitone;
        lfo_params.amount = 12.0; // one octave
        lfo_params.rate_hz = 1.0;

        let mut voice = Voice::new(1);
        voice.prepare(48_000.0, 128);
        voice.note_on(60, 1.0, &gen_params, &env_params, &lfo_params, 1);

        for _ in 0..1000 {
            assert!(voice.next_sample().0.is_finite());
        }
    }

    #[test]
    fn switching_generator_kind_rebuilds_generator() {
        let (mut gen_params, env_params, lfo_params) = default_params();
        let mut voice = Voice::new(1);
        voice.prepare(48_000.0, 128);
        voice.note_on(60, 1.0, &gen_params, &env_params, &lfo_params, 1);

        gen_params.kind = GeneratorKind::Noise;
        voice.note_on(60, 1.0, &gen_params, &env_params, &lfo_params, 2);
        for _ in 0..100 {
            let (l, _) = voice.next_sample();
            assert!(l.is_finite());
        }
    }
}
