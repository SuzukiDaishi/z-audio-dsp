//! Polyphonic voice for [`crate::FormulaSynth`].

use z_audio_dsp::{
    Envelope, EnvelopeParams, EnvelopeState, FormulaGenerator, FormulaParams, Generator, Modulator,
    ProcessContext, midi_note_to_hz,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormulaVoiceState {
    #[default]
    Idle,
    Active,
    Releasing,
}

pub struct FormulaVoice {
    state: FormulaVoiceState,
    note: u8,
    generator: FormulaGenerator,
    amp_env: Envelope,
    activation_id: u64,
    sample_rate: f32,
    max_block_size: usize,
}

impl FormulaVoice {
    pub fn new() -> Self {
        Self {
            state: FormulaVoiceState::Idle,
            note: 0,
            generator: FormulaGenerator::default(),
            amp_env: Envelope::new(default_formula_envelope()),
            activation_id: 0,
            sample_rate: 48_000.0,
            max_block_size: 0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32, max_block_size: usize) {
        self.sample_rate = sample_rate;
        self.max_block_size = max_block_size;
        self.generator.prepare(sample_rate, max_block_size);
        self.amp_env.prepare(sample_rate, max_block_size);
    }

    pub fn is_active(&self) -> bool {
        self.state != FormulaVoiceState::Idle
    }

    pub fn is_releasing(&self) -> bool {
        self.state == FormulaVoiceState::Releasing
    }

    pub fn note(&self) -> u8 {
        self.note
    }

    pub fn activation_id(&self) -> u64 {
        self.activation_id
    }

    pub fn note_on(
        &mut self,
        note: u8,
        velocity: f32,
        params: FormulaParams,
        env: EnvelopeParams,
        activation_id: u64,
    ) {
        self.note = note;
        self.activation_id = activation_id;
        self.generator.set_params(FormulaParams {
            frequency_hz: midi_note_to_hz(note as f32),
            velocity: velocity.clamp(0.0, 1.0),
            ..params
        });
        self.generator
            .prepare(self.sample_rate, self.max_block_size);
        self.generator.reset();
        self.amp_env.set_params(env);
        self.amp_env.note_on();
        self.state = FormulaVoiceState::Active;
    }

    pub fn note_off(&mut self, note: u8) {
        if self.state == FormulaVoiceState::Active && self.note == note {
            self.amp_env.note_off();
            self.state = FormulaVoiceState::Releasing;
        }
    }

    pub fn apply_params(&mut self, params: FormulaParams, env: EnvelopeParams) {
        if self.state == FormulaVoiceState::Idle {
            return;
        }
        self.generator.set_params(FormulaParams {
            frequency_hz: midi_note_to_hz(self.note as f32),
            ..params
        });
        self.amp_env.set_params(env);
    }

    pub fn next_sample(&mut self) -> f32 {
        if self.state == FormulaVoiceState::Idle {
            return 0.0;
        }
        let env = self.amp_env.next_sample();
        if self.amp_env.state() == EnvelopeState::Idle {
            self.state = FormulaVoiceState::Idle;
        }
        self.generator.next_sample() * env
    }
}

impl Default for FormulaVoice {
    fn default() -> Self {
        Self::new()
    }
}

pub fn default_formula_envelope() -> EnvelopeParams {
    EnvelopeParams {
        attack: 0.005,
        decay: 0.18,
        sustain: 0.75,
        release: 0.35,
        ..EnvelopeParams::default()
    }
}

pub fn empty_context(sample_rate: f32) -> ProcessContext<'static> {
    ProcessContext::new(sample_rate, 1, 120.0, &[])
}
