//! Realtime-safe formula synthesis using a small stack VM and built-in
//! programs. User text parsing is intentionally outside the audio thread.

pub mod builtins;
pub mod generator;
pub mod opcode;
pub mod program;
pub mod runtime;

pub use builtins::{builtin_program, builtin_programs};
pub use generator::{FormulaGenerator, FormulaParams};
pub use opcode::FormulaOpcode;
pub use program::{
    FORMULA_MACRO_COUNT, FORMULA_STACK_SIZE, FormulaMacroMetadata, FormulaProgram, FormulaProgramId,
};
pub use runtime::{FormulaRuntime, validate_stack_depth};
