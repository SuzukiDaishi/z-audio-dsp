//! Stack VM evaluator for formula programs.

use crate::math::{TAU, flush_denormal};

use super::{FORMULA_MACRO_COUNT, FORMULA_STACK_SIZE, FormulaOpcode, FormulaProgram};

#[derive(Debug, Clone, Copy)]
pub struct FormulaRuntime {
    stack: [f32; FORMULA_STACK_SIZE],
}

impl Default for FormulaRuntime {
    fn default() -> Self {
        Self {
            stack: [0.0; FORMULA_STACK_SIZE],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FormulaEvalContext {
    pub phase: f32,
    pub time_sec: f32,
    pub frequency_hz: f32,
    pub midi_note: f32,
    pub velocity: f32,
    pub macros: [f32; FORMULA_MACRO_COUNT],
}

impl FormulaRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn eval(&mut self, program: &FormulaProgram, ctx: FormulaEvalContext) -> f32 {
        let mut sp = 0usize;

        for op in program.opcodes {
            match *op {
                FormulaOpcode::Const(v) => push(&mut self.stack, &mut sp, v),
                FormulaOpcode::Phase => push(&mut self.stack, &mut sp, ctx.phase),
                FormulaOpcode::TimeSec => push(&mut self.stack, &mut sp, ctx.time_sec),
                FormulaOpcode::FrequencyHz => push(&mut self.stack, &mut sp, ctx.frequency_hz),
                FormulaOpcode::MidiNote => push(&mut self.stack, &mut sp, ctx.midi_note),
                FormulaOpcode::Velocity => push(&mut self.stack, &mut sp, ctx.velocity),
                FormulaOpcode::Macro(index) => {
                    let value = ctx.macros.get(index as usize).copied().unwrap_or(0.0);
                    push(&mut self.stack, &mut sp, value);
                }
                FormulaOpcode::Add => binary(&mut self.stack, &mut sp, |a, b| a + b),
                FormulaOpcode::Sub => binary(&mut self.stack, &mut sp, |a, b| a - b),
                FormulaOpcode::Mul => binary(&mut self.stack, &mut sp, |a, b| a * b),
                FormulaOpcode::DivSafe => binary(&mut self.stack, &mut sp, |a, b| {
                    if b.abs() < 1.0e-9 { 0.0 } else { a / b }
                }),
                FormulaOpcode::Neg => unary(&mut self.stack, sp, |x| -x),
                FormulaOpcode::Sin2Pi => unary(&mut self.stack, sp, |x| (TAU * x).sin()),
                FormulaOpcode::Cos2Pi => unary(&mut self.stack, sp, |x| (TAU * x).cos()),
                FormulaOpcode::TanH => unary(&mut self.stack, sp, |x| x.tanh()),
                FormulaOpcode::Abs => unary(&mut self.stack, sp, f32::abs),
                FormulaOpcode::Sign => unary(&mut self.stack, sp, f32::signum),
                FormulaOpcode::Floor => unary(&mut self.stack, sp, f32::floor),
                FormulaOpcode::Fract => unary(&mut self.stack, sp, |x| x - x.floor()),
                FormulaOpcode::PowSafe => binary(&mut self.stack, &mut sp, |a, b| {
                    a.abs().max(1.0e-9).powf(b.clamp(-16.0, 16.0))
                }),
                FormulaOpcode::Exp => unary(&mut self.stack, sp, |x| x.clamp(-40.0, 40.0).exp()),
                FormulaOpcode::LogSafe => unary(&mut self.stack, sp, |x| x.abs().max(1.0e-9).ln()),
                FormulaOpcode::SqrtSafe => unary(&mut self.stack, sp, |x| x.max(0.0).sqrt()),
                FormulaOpcode::Min => binary(&mut self.stack, &mut sp, f32::min),
                FormulaOpcode::Max => binary(&mut self.stack, &mut sp, f32::max),
                FormulaOpcode::Clamp01 => unary(&mut self.stack, sp, |x| x.clamp(0.0, 1.0)),
                FormulaOpcode::Mix => ternary(&mut self.stack, &mut sp, |a, b, t| {
                    a + (b - a) * t.clamp(0.0, 1.0)
                }),
                FormulaOpcode::Saw => unary(&mut self.stack, sp, |x| 2.0 * (x - x.floor()) - 1.0),
                FormulaOpcode::Square => unary(&mut self.stack, sp, |x| {
                    if x - x.floor() < 0.5 { 1.0 } else { -1.0 }
                }),
                FormulaOpcode::Triangle => unary(&mut self.stack, sp, |x| {
                    4.0 * ((x - x.floor()) - 0.5).abs() - 1.0
                }),
                FormulaOpcode::NoiseHash => unary(&mut self.stack, sp, noise_hash),
                FormulaOpcode::SmoothStep => unary(&mut self.stack, sp, |x| {
                    let t = x.clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                }),
                FormulaOpcode::SoftClip => unary(&mut self.stack, sp, |x| x / (1.0 + x.abs())),
                FormulaOpcode::WaveFold => unary(&mut self.stack, sp, wavefold),
            }
        }

        let out = if sp > 0 { self.stack[sp - 1] } else { 0.0 };
        flush_denormal((out * program.output_scale).clamp(-8.0, 8.0).tanh())
    }
}

pub fn validate_stack_depth(opcodes: &[FormulaOpcode]) -> Result<usize, &'static str> {
    let mut depth = 0isize;
    let mut max_depth = 0isize;
    for op in opcodes {
        let (pop, push) = match op {
            FormulaOpcode::Const(_)
            | FormulaOpcode::Phase
            | FormulaOpcode::TimeSec
            | FormulaOpcode::FrequencyHz
            | FormulaOpcode::MidiNote
            | FormulaOpcode::Velocity
            | FormulaOpcode::Macro(_) => (0, 1),
            FormulaOpcode::Neg
            | FormulaOpcode::Sin2Pi
            | FormulaOpcode::Cos2Pi
            | FormulaOpcode::TanH
            | FormulaOpcode::Abs
            | FormulaOpcode::Sign
            | FormulaOpcode::Floor
            | FormulaOpcode::Fract
            | FormulaOpcode::Exp
            | FormulaOpcode::LogSafe
            | FormulaOpcode::SqrtSafe
            | FormulaOpcode::Clamp01
            | FormulaOpcode::Saw
            | FormulaOpcode::Square
            | FormulaOpcode::Triangle
            | FormulaOpcode::NoiseHash
            | FormulaOpcode::SmoothStep
            | FormulaOpcode::SoftClip
            | FormulaOpcode::WaveFold => (1, 1),
            FormulaOpcode::Add
            | FormulaOpcode::Sub
            | FormulaOpcode::Mul
            | FormulaOpcode::DivSafe
            | FormulaOpcode::PowSafe
            | FormulaOpcode::Min
            | FormulaOpcode::Max => (2, 1),
            FormulaOpcode::Mix => (3, 1),
        };
        depth -= pop;
        if depth < 0 {
            return Err("formula stack underflow");
        }
        depth += push;
        max_depth = max_depth.max(depth);
        if max_depth as usize > FORMULA_STACK_SIZE {
            return Err("formula stack overflow");
        }
    }
    if depth != 1 {
        return Err("formula must leave exactly one output value");
    }
    Ok(max_depth as usize)
}

fn push(stack: &mut [f32; FORMULA_STACK_SIZE], sp: &mut usize, value: f32) {
    if *sp < FORMULA_STACK_SIZE {
        stack[*sp] = value;
        *sp += 1;
    }
}

fn pop(stack: &mut [f32; FORMULA_STACK_SIZE], sp: &mut usize) -> f32 {
    if *sp == 0 {
        0.0
    } else {
        *sp -= 1;
        stack[*sp]
    }
}

fn unary(stack: &mut [f32; FORMULA_STACK_SIZE], sp: usize, f: impl FnOnce(f32) -> f32) {
    if sp > 0 {
        stack[sp - 1] = f(stack[sp - 1]);
    }
}

fn binary(stack: &mut [f32; FORMULA_STACK_SIZE], sp: &mut usize, f: impl FnOnce(f32, f32) -> f32) {
    let b = pop(stack, sp);
    let a = pop(stack, sp);
    push(stack, sp, f(a, b));
}

fn ternary(
    stack: &mut [f32; FORMULA_STACK_SIZE],
    sp: &mut usize,
    f: impl FnOnce(f32, f32, f32) -> f32,
) {
    let c = pop(stack, sp);
    let b = pop(stack, sp);
    let a = pop(stack, sp);
    push(stack, sp, f(a, b, c));
}

fn noise_hash(x: f32) -> f32 {
    let mut n = x.to_bits().wrapping_mul(0x27d4_eb2d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x85eb_ca6b);
    n ^= n >> 13;
    let unit = (n as f32) / (u32::MAX as f32);
    unit * 2.0 - 1.0
}

fn wavefold(x: f32) -> f32 {
    let folded = (x + 1.0).rem_euclid(4.0);
    if folded < 2.0 {
        folded - 1.0
    } else {
        3.0 - folded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::builtin_programs;

    #[test]
    fn builtins_have_valid_stack_depth() {
        for program in builtin_programs() {
            validate_stack_depth(program.opcodes).expect(program.name);
        }
    }

    #[test]
    fn noise_hash_is_deterministic() {
        assert_eq!(noise_hash(123.0), noise_hash(123.0));
        assert_ne!(noise_hash(123.0), noise_hash(124.0));
    }
}
