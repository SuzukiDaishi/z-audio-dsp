//! Built-in formula program metadata.

use super::FormulaOpcode;

pub const FORMULA_STACK_SIZE: usize = 32;
pub const FORMULA_MACRO_COUNT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormulaProgramId {
    #[default]
    PurePhase,
    BrightFold,
    FmBell,
    AdditiveOdd,
    PdSyncish,
    ChaoticSoft,
}

impl FormulaProgramId {
    pub const VARIANT_COUNT: u32 = 6;

    pub fn from_param_value(value: f32) -> Self {
        match value.round().clamp(0.0, (Self::VARIANT_COUNT - 1) as f32) as u32 {
            0 => Self::PurePhase,
            1 => Self::BrightFold,
            2 => Self::FmBell,
            3 => Self::AdditiveOdd,
            4 => Self::PdSyncish,
            _ => Self::ChaoticSoft,
        }
    }

    pub fn to_param_value(self) -> f32 {
        match self {
            Self::PurePhase => 0.0,
            Self::BrightFold => 1.0,
            Self::FmBell => 2.0,
            Self::AdditiveOdd => 3.0,
            Self::PdSyncish => 4.0,
            Self::ChaoticSoft => 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormulaMacroMetadata {
    pub name: &'static str,
    pub default: f32,
}

impl FormulaMacroMetadata {
    pub const fn empty() -> Self {
        Self {
            name: "",
            default: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FormulaProgram {
    pub id: FormulaProgramId,
    pub name: &'static str,
    pub opcodes: &'static [FormulaOpcode],
    pub output_scale: f32,
    pub recommended_gain_db: f32,
    pub macro_metadata: [FormulaMacroMetadata; FORMULA_MACRO_COUNT],
}
