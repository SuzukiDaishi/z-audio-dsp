//! Polyphonic formula synthesizer runtime.

use z_audio_dsp::{
    Effect, EnvelopeCurve, EnvelopeParams, EventKind, FormulaParams, FormulaProgramId, Gain,
    ParamId, ProcessContext, TimedEvent,
};

use crate::formula_voice::{FormulaVoice, default_formula_envelope};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormulaSynthConfig {
    pub sample_rate: f32,
    pub max_block_size: usize,
    pub max_polyphony: usize,
}

impl Default for FormulaSynthConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            max_block_size: 512,
            max_polyphony: 16,
        }
    }
}

pub struct FormulaSynth {
    sample_rate: f32,
    max_block_size: usize,
    voices: Vec<FormulaVoice>,
    next_activation_id: u64,
    formula_params: FormulaParams,
    env_params: EnvelopeParams,
    master_gain: Gain,
}

impl FormulaSynth {
    pub fn new(config: FormulaSynthConfig) -> Self {
        let voices = (0..config.max_polyphony.max(1))
            .map(|_| FormulaVoice::new())
            .collect();
        let mut synth = Self {
            sample_rate: config.sample_rate,
            max_block_size: config.max_block_size,
            voices,
            next_activation_id: 0,
            formula_params: FormulaParams::default(),
            env_params: default_formula_envelope(),
            master_gain: Gain::default(),
        };
        synth.prepare();
        synth
    }

    fn prepare(&mut self) {
        for voice in &mut self.voices {
            voice.prepare(self.sample_rate, self.max_block_size);
        }
        self.master_gain
            .prepare(self.sample_rate, self.max_block_size);
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.is_active()).count()
    }

    pub fn set_param(&mut self, id: ParamId, value: f32) {
        let m = id.metadata();
        let clamped = value.clamp(m.min, m.max);
        match id {
            ParamId::MasterGain => self.master_gain.set_gain(clamped),
            ParamId::FormulaProgram => {
                self.formula_params.program_id = FormulaProgramId::from_param_value(value);
                self.apply_voice_params();
            }
            ParamId::FormulaMacro1
            | ParamId::FormulaMacro2
            | ParamId::FormulaMacro3
            | ParamId::FormulaMacro4
            | ParamId::FormulaMacro5
            | ParamId::FormulaMacro6
            | ParamId::FormulaMacro7
            | ParamId::FormulaMacro8 => {
                let index = (id as u32 - ParamId::FormulaMacro1 as u32) as usize;
                self.formula_params.macros[index] = clamped;
                self.apply_voice_params();
            }
            ParamId::FormulaOutputGain => {
                self.formula_params.output_gain_db = clamped;
                self.apply_voice_params();
            }
            ParamId::EnvAttack => self.env_params.attack = clamped,
            ParamId::EnvDecay => self.env_params.decay = clamped,
            ParamId::EnvSustain => self.env_params.sustain = clamped,
            ParamId::EnvRelease => self.env_params.release = clamped,
            ParamId::EnvCurve => self.env_params.curve = EnvelopeCurve::from_param_value(value),
            _ => {}
        }
    }

    pub fn param_value(&self, id: ParamId) -> f32 {
        match id {
            ParamId::MasterGain => self.master_gain.target_gain(),
            ParamId::FormulaProgram => self.formula_params.program_id.to_param_value(),
            ParamId::FormulaMacro1
            | ParamId::FormulaMacro2
            | ParamId::FormulaMacro3
            | ParamId::FormulaMacro4
            | ParamId::FormulaMacro5
            | ParamId::FormulaMacro6
            | ParamId::FormulaMacro7
            | ParamId::FormulaMacro8 => {
                let index = (id as u32 - ParamId::FormulaMacro1 as u32) as usize;
                self.formula_params.macros[index]
            }
            ParamId::FormulaOutputGain => self.formula_params.output_gain_db,
            ParamId::EnvAttack => self.env_params.attack,
            ParamId::EnvDecay => self.env_params.decay,
            ParamId::EnvSustain => self.env_params.sustain,
            ParamId::EnvRelease => self.env_params.release,
            ParamId::EnvCurve => self.env_params.curve.to_param_value(),
            _ => id.metadata().default,
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: f32) {
        let index = self.find_voice_for_note_on();
        self.next_activation_id = self.next_activation_id.wrapping_add(1);
        self.voices[index].note_on(
            note,
            velocity,
            self.formula_params,
            self.env_params,
            self.next_activation_id,
        );
    }

    pub fn note_off(&mut self, note: u8) {
        for voice in &mut self.voices {
            voice.note_off(note);
        }
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let ctx = ProcessContext::new(self.sample_rate, left.len(), 120.0, &[]);
        self.process_with_context(&ctx, left, right);
    }

    pub fn process_with_context(
        &mut self,
        ctx: &ProcessContext,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        debug_assert_eq!(left.len(), right.len());
        let mut event_index = 0;
        for i in 0..left.len() {
            while event_index < ctx.events.len() && ctx.events[event_index].sample_offset == i {
                self.handle_event(ctx.events[event_index]);
                event_index += 1;
            }

            let mut sum = 0.0;
            for voice in &mut self.voices {
                sum += voice.next_sample();
            }
            left[i] = sum * core::f32::consts::FRAC_1_SQRT_2;
            right[i] = sum * core::f32::consts::FRAC_1_SQRT_2;
        }
        self.master_gain.process_stereo(ctx, left, right);
    }

    fn handle_event(&mut self, event: TimedEvent) {
        match event.kind {
            EventKind::NoteOn { note, velocity } => self.note_on(note, velocity),
            EventKind::NoteOff { note, .. } => self.note_off(note),
            EventKind::Param { id, value } => self.set_param(id, value),
        }
    }

    fn apply_voice_params(&mut self) {
        for voice in &mut self.voices {
            voice.apply_params(self.formula_params, self.env_params);
        }
    }

    fn find_voice_for_note_on(&self) -> usize {
        if let Some(index) = self.voices.iter().position(|v| !v.is_active()) {
            return index;
        }
        self.voices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_releasing())
            .min_by_key(|(_, v)| v.activation_id())
            .or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.activation_id())
            })
            .map(|(index, _)| index)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_on_produces_signal() {
        let mut synth = FormulaSynth::new(FormulaSynthConfig {
            max_block_size: 128,
            max_polyphony: 4,
            ..Default::default()
        });
        synth.note_on(60, 1.0);
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        synth.process(&mut left, &mut right);
        assert!(left.iter().any(|s| s.abs() > 0.0));
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    }

    #[test]
    fn formula_param_round_trips() {
        let mut synth = FormulaSynth::new(FormulaSynthConfig::default());
        synth.set_param(
            ParamId::FormulaProgram,
            FormulaProgramId::BrightFold.to_param_value(),
        );
        synth.set_param(ParamId::FormulaMacro1, 0.75);
        assert_eq!(
            synth.param_value(ParamId::FormulaProgram),
            FormulaProgramId::BrightFold.to_param_value()
        );
        assert_eq!(synth.param_value(ParamId::FormulaMacro1), 0.75);
    }
}
