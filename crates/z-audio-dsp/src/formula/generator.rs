//! Formula-based mono signal generator.

use crate::context::ProcessContext;
use crate::math::{db_to_linear, midi_note_to_hz};
use crate::{Generator, formula::runtime::FormulaEvalContext};

use super::{
    FORMULA_MACRO_COUNT, FormulaProgram, FormulaProgramId, FormulaRuntime, builtin_program,
};

#[derive(Debug, Clone, Copy)]
pub struct FormulaParams {
    pub frequency_hz: f32,
    pub velocity: f32,
    pub program_id: FormulaProgramId,
    pub macros: [f32; FORMULA_MACRO_COUNT],
    pub output_gain_db: f32,
}

impl Default for FormulaParams {
    fn default() -> Self {
        let program = builtin_program(FormulaProgramId::default());
        let mut macros = [0.0; FORMULA_MACRO_COUNT];
        for (dst, meta) in macros.iter_mut().zip(program.macro_metadata) {
            *dst = meta.default;
        }
        Self {
            frequency_hz: 440.0,
            velocity: 1.0,
            program_id: FormulaProgramId::default(),
            macros,
            output_gain_db: -6.0,
        }
    }
}

pub struct FormulaGenerator {
    sample_rate: f32,
    phase: f32,
    time_samples: u64,
    frequency_hz: f32,
    velocity: f32,
    midi_note: f32,
    program: &'static FormulaProgram,
    runtime: FormulaRuntime,
    macros: [f32; FORMULA_MACRO_COUNT],
    output_gain: f32,
}

impl FormulaGenerator {
    pub fn new(params: FormulaParams) -> Self {
        let mut generator = Self {
            sample_rate: 48_000.0,
            phase: 0.0,
            time_samples: 0,
            frequency_hz: params.frequency_hz,
            velocity: params.velocity.clamp(0.0, 1.0),
            midi_note: 69.0,
            program: builtin_program(params.program_id),
            runtime: FormulaRuntime::new(),
            macros: params.macros,
            output_gain: db_to_linear(params.output_gain_db),
        };
        generator.set_frequency_hz(params.frequency_hz);
        generator
    }

    pub fn set_params(&mut self, params: FormulaParams) {
        self.set_program_id(params.program_id);
        self.set_frequency_hz(params.frequency_hz);
        self.velocity = params.velocity.clamp(0.0, 1.0);
        self.macros = params.macros;
        self.output_gain = db_to_linear(params.output_gain_db);
    }

    pub fn set_program_id(&mut self, id: FormulaProgramId) {
        self.program = builtin_program(id);
    }

    pub fn program(&self) -> &'static FormulaProgram {
        self.program
    }

    pub fn set_frequency_hz(&mut self, frequency_hz: f32) {
        self.frequency_hz = frequency_hz.clamp(0.0, 24_000.0);
        self.midi_note = if self.frequency_hz > 0.0 {
            69.0 + 12.0 * (self.frequency_hz / 440.0).log2()
        } else {
            0.0
        };
    }

    pub fn set_velocity(&mut self, velocity: f32) {
        self.velocity = velocity.clamp(0.0, 1.0);
    }

    pub fn set_macro(&mut self, index: usize, value: f32) {
        if let Some(macro_value) = self.macros.get_mut(index) {
            *macro_value = value.clamp(0.0, 1.0);
        }
    }

    pub fn next_sample(&mut self) -> f32 {
        let ctx = FormulaEvalContext {
            phase: self.phase,
            time_sec: self.time_samples as f32 / self.sample_rate,
            frequency_hz: self.frequency_hz,
            midi_note: self.midi_note,
            velocity: self.velocity,
            macros: self.macros,
        };
        let sample = self.runtime.eval(self.program, ctx) * self.velocity * self.output_gain;

        if self.sample_rate > 0.0 {
            self.phase += self.frequency_hz / self.sample_rate;
            self.phase -= self.phase.floor();
        }
        self.time_samples = self.time_samples.wrapping_add(1);
        sample
    }
}

impl Default for FormulaGenerator {
    fn default() -> Self {
        Self::new(FormulaParams::default())
    }
}

impl Generator for FormulaGenerator {
    fn prepare(&mut self, sample_rate: f32, _max_block_size: usize) {
        self.sample_rate = sample_rate.max(1.0);
        self.frequency_hz = self.frequency_hz.min(self.sample_rate * 0.49);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.time_samples = 0;
    }

    fn process_mono(&mut self, _ctx: &ProcessContext, out: &mut [f32]) {
        for sample in out {
            *sample = self.next_sample();
        }
    }
}

impl From<u8> for FormulaGenerator {
    fn from(note: u8) -> Self {
        let mut generator = Self::default();
        generator.set_frequency_hz(midi_note_to_hz(note as f32));
        generator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::builtin_programs;

    #[test]
    fn every_builtin_renders_finite_samples() {
        for program in builtin_programs() {
            let mut generator = FormulaGenerator::default();
            generator.prepare(48_000.0, 128);
            generator.set_program_id(program.id);
            generator.set_velocity(1.0);
            for _ in 0..2048 {
                assert!(generator.next_sample().is_finite(), "{}", program.name);
            }
        }
    }

    #[test]
    fn reset_restores_phase_and_time() {
        let mut generator = FormulaGenerator::default();
        generator.prepare(48_000.0, 128);
        for _ in 0..32 {
            generator.next_sample();
        }
        generator.reset();
        assert_eq!(generator.phase, 0.0);
        assert_eq!(generator.time_samples, 0);
    }
}
