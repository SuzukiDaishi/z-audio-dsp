//! Formula/modal drum set.

mod params;
mod synth;
mod voice;

pub use params::DrumKitParams;
pub use synth::{FormulaDrumKit, FormulaDrumKitConfig};
pub use voice::{DrumInstrument, DrumVoice, DrumVoiceState};
