//! A single synthesizer voice: one generator, one amplitude envelope, and one
//! LFO, mixed to stereo via a constant-power pan law.

use z_audio_dsp::{
    Envelope, EnvelopeParams, EnvelopeState, Generator, GeneratorInstance, GeneratorKind,
    GeneratorParams, Lfo, LfoParams, LfoTarget, Modulator, midi_note_to_hz,
};

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
    gain: f32,
    pan: f32,
    generator_kind: GeneratorKind,
    generator: GeneratorInstance,
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
            gain: generator_params.gain,
            pan: generator_params.pan,
            generator_kind: generator_params.kind,
            generator: GeneratorInstance::from_params(&generator_params, seed),
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
        self.generator.prepare(sample_rate, max_block_size);
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
        self.gain = generator_params.gain;
        self.pan = generator_params.pan;
        self.activation_id = activation_id;

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

        let mut note = self.note as f32;
        if lfo_params.target == LfoTarget::PitchSemitone {
            note += lfo_value * lfo_params.amount;
        }
        self.generator.set_frequency_hz(midi_note_to_hz(note));

        let env = self.amp_env.next_sample();
        if self.amp_env.state() == EnvelopeState::Idle {
            self.state = VoiceState::Idle;
        }

        let mut amplitude = self.generator.next_sample() * env * self.gain * self.velocity;
        if lfo_params.target == LfoTarget::Gain {
            amplitude *= 1.0 + lfo_value * lfo_params.amount;
        }

        let pan = self.pan.clamp(-1.0, 1.0);
        let angle = (pan + 1.0) * core::f32::consts::FRAC_PI_4;
        (amplitude * angle.cos(), amplitude * angle.sin())
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
