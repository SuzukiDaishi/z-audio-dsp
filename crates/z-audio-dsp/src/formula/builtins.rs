//! Curated built-in formula programs.

use super::{
    FORMULA_MACRO_COUNT, FormulaMacroMetadata, FormulaOpcode as Op, FormulaProgram,
    FormulaProgramId,
};

const EMPTY: FormulaMacroMetadata = FormulaMacroMetadata::empty();

const fn macros(
    a: FormulaMacroMetadata,
    b: FormulaMacroMetadata,
) -> [FormulaMacroMetadata; FORMULA_MACRO_COUNT] {
    [a, b, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY]
}

const PURE_PHASE_OPS: &[Op] = &[Op::Phase, Op::Sin2Pi];

const BRIGHT_FOLD_OPS: &[Op] = &[
    Op::Phase,
    Op::Phase,
    Op::Const(2.0),
    Op::Mul,
    Op::Sin2Pi,
    Op::Macro(0),
    Op::Mul,
    Op::Add,
    Op::Sin2Pi,
    Op::Const(1.0),
    Op::Macro(1),
    Op::Const(8.0),
    Op::Mul,
    Op::Add,
    Op::Mul,
    Op::WaveFold,
];

const FM_BELL_OPS: &[Op] = &[
    Op::Phase,
    Op::Phase,
    Op::Const(2.71),
    Op::Mul,
    Op::Sin2Pi,
    Op::Macro(0),
    Op::Const(8.0),
    Op::Mul,
    Op::TimeSec,
    Op::Const(-4.5),
    Op::Mul,
    Op::Exp,
    Op::Mul,
    Op::Mul,
    Op::Add,
    Op::Sin2Pi,
];

const ADDITIVE_ODD_OPS: &[Op] = &[
    Op::Phase,
    Op::Sin2Pi,
    Op::Phase,
    Op::Const(3.0),
    Op::Mul,
    Op::Sin2Pi,
    Op::Macro(0),
    Op::Const(0.55),
    Op::Mul,
    Op::Mul,
    Op::Add,
    Op::Phase,
    Op::Const(5.0),
    Op::Mul,
    Op::Sin2Pi,
    Op::Macro(0),
    Op::Const(0.32),
    Op::Mul,
    Op::Mul,
    Op::Add,
    Op::Phase,
    Op::Const(7.0),
    Op::Mul,
    Op::Sin2Pi,
    Op::Macro(1),
    Op::Const(0.18),
    Op::Mul,
    Op::Mul,
    Op::Add,
];

const PD_SYNCISH_OPS: &[Op] = &[
    Op::Phase,
    Op::Phase,
    Op::Const(0.5),
    Op::Sub,
    Op::Macro(0),
    Op::Mul,
    Op::Sin2Pi,
    Op::Macro(0),
    Op::Const(0.35),
    Op::Mul,
    Op::Mul,
    Op::Add,
    Op::Const(1.0),
    Op::Macro(1),
    Op::Const(8.0),
    Op::Mul,
    Op::Add,
    Op::Mul,
    Op::Sin2Pi,
];

const CHAOTIC_SOFT_OPS: &[Op] = &[
    Op::Phase,
    Op::Sin2Pi,
    Op::TimeSec,
    Op::Const(127.0),
    Op::Mul,
    Op::Floor,
    Op::NoiseHash,
    Op::Macro(0),
    Op::Mul,
    Op::Add,
    Op::TanH,
];

pub static PURE_PHASE: FormulaProgram = FormulaProgram {
    id: FormulaProgramId::PurePhase,
    name: "Pure Phase",
    opcodes: PURE_PHASE_OPS,
    output_scale: 0.9,
    recommended_gain_db: -6.0,
    macro_metadata: macros(EMPTY, EMPTY),
};

pub static BRIGHT_FOLD: FormulaProgram = FormulaProgram {
    id: FormulaProgramId::BrightFold,
    name: "Bright Fold",
    opcodes: BRIGHT_FOLD_OPS,
    output_scale: 0.85,
    recommended_gain_db: -10.0,
    macro_metadata: macros(
        FormulaMacroMetadata {
            name: "Phase Mod",
            default: 0.15,
        },
        FormulaMacroMetadata {
            name: "Fold",
            default: 0.35,
        },
    ),
};

pub static FM_BELL: FormulaProgram = FormulaProgram {
    id: FormulaProgramId::FmBell,
    name: "FM Bell",
    opcodes: FM_BELL_OPS,
    output_scale: 0.8,
    recommended_gain_db: -9.0,
    macro_metadata: macros(
        FormulaMacroMetadata {
            name: "Index",
            default: 0.55,
        },
        EMPTY,
    ),
};

pub static ADDITIVE_ODD: FormulaProgram = FormulaProgram {
    id: FormulaProgramId::AdditiveOdd,
    name: "Additive Odd",
    opcodes: ADDITIVE_ODD_OPS,
    output_scale: 0.75,
    recommended_gain_db: -8.0,
    macro_metadata: macros(
        FormulaMacroMetadata {
            name: "Odd Mix",
            default: 0.55,
        },
        FormulaMacroMetadata {
            name: "Air",
            default: 0.4,
        },
    ),
};

pub static PD_SYNCISH: FormulaProgram = FormulaProgram {
    id: FormulaProgramId::PdSyncish,
    name: "PD Syncish",
    opcodes: PD_SYNCISH_OPS,
    output_scale: 0.82,
    recommended_gain_db: -9.0,
    macro_metadata: macros(
        FormulaMacroMetadata {
            name: "Bend",
            default: 0.35,
        },
        FormulaMacroMetadata {
            name: "Harmonics",
            default: 0.35,
        },
    ),
};

pub static CHAOTIC_SOFT: FormulaProgram = FormulaProgram {
    id: FormulaProgramId::ChaoticSoft,
    name: "Chaotic Soft",
    opcodes: CHAOTIC_SOFT_OPS,
    output_scale: 0.7,
    recommended_gain_db: -12.0,
    macro_metadata: macros(
        FormulaMacroMetadata {
            name: "Noise",
            default: 0.12,
        },
        EMPTY,
    ),
};

static PROGRAMS: [&FormulaProgram; FormulaProgramId::VARIANT_COUNT as usize] = [
    &PURE_PHASE,
    &BRIGHT_FOLD,
    &FM_BELL,
    &ADDITIVE_ODD,
    &PD_SYNCISH,
    &CHAOTIC_SOFT,
];

pub fn builtin_programs() -> &'static [&'static FormulaProgram] {
    &PROGRAMS
}

pub fn builtin_program(id: FormulaProgramId) -> &'static FormulaProgram {
    PROGRAMS[id.to_param_value() as usize]
}
