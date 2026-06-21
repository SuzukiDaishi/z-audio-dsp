//! A single modal piano voice.

use z_audio_dsp::{HammerExciter, ModalBank, ModalMode, midi_note_to_hz};

use super::PianoParams;

pub const MAX_PIANO_PARTIALS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PianoVoiceState {
    #[default]
    Idle,
    Active,
    Releasing,
}

pub struct PianoVoice {
    state: PianoVoiceState,
    note: u8,
    velocity: f32,
    activation_id: u64,
    sample_rate: f32,
    hammer: HammerExciter,
    strings: ModalBank<MAX_PIANO_PARTIALS>,
    release_decays: [f32; MAX_PIANO_PARTIALS],
    pan: f32,
    release_gain: f32,
    release_coeff: f32,
    strike_env: f32,
    strike_env_coeff: f32,
    strike_env_floor: f32,
}

impl PianoVoice {
    pub fn new(seed: u32) -> Self {
        Self {
            state: PianoVoiceState::Idle,
            note: 0,
            velocity: 0.0,
            activation_id: 0,
            sample_rate: 48_000.0,
            hammer: HammerExciter::new(seed),
            strings: ModalBank::new(),
            release_decays: [0.25; MAX_PIANO_PARTIALS],
            pan: 0.0,
            release_gain: 1.0,
            release_coeff: 0.999,
            strike_env: 0.0,
            strike_env_coeff: 0.999,
            strike_env_floor: 0.30,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.hammer.prepare(self.sample_rate);
        self.strings.prepare(self.sample_rate);
        self.configure_release(0.8, 0.0);
    }

    pub fn is_active(&self) -> bool {
        self.state != PianoVoiceState::Idle
    }

    pub fn is_releasing(&self) -> bool {
        self.state == PianoVoiceState::Releasing
    }

    pub fn activation_id(&self) -> u64 {
        self.activation_id
    }

    pub fn note(&self) -> u8 {
        self.note
    }

    pub fn note_on(&mut self, note: u8, velocity: f32, params: PianoParams, activation_id: u64) {
        let params = params.sanitized();
        self.note = note;
        self.velocity = velocity.clamp(0.0, 1.0);
        self.activation_id = activation_id;
        self.release_gain = 1.0;
        self.configure_release(params.release, params.pedal_resonance);
        self.configure_strike_envelope(note);
        self.pan = ((note as f32 - 60.0) / 42.0).clamp(-1.0, 1.0) * params.stereo_width;

        let frequency = midi_note_to_hz(note as f32);
        let modes = build_modes(note, frequency, self.velocity, params, self.sample_rate);
        self.release_decays = modes.release_decays;
        self.strings.set_modes(&modes.modes);
        self.hammer.trigger(
            self.velocity,
            frequency,
            params.hammer_hardness,
            params.hammer_noise,
        );
        self.state = PianoVoiceState::Active;
    }

    pub fn note_off(&mut self, note: u8) {
        if self.state == PianoVoiceState::Active && self.note == note {
            self.state = PianoVoiceState::Releasing;
            self.strings.limit_decays(&self.release_decays);
        }
    }

    pub fn apply_params(&mut self, params: PianoParams) {
        self.configure_release(params.release, params.pedal_resonance);
    }

    pub fn next_sample(&mut self) -> (f32, f32) {
        if self.state == PianoVoiceState::Idle {
            return (0.0, 0.0);
        }
        if self.state == PianoVoiceState::Releasing {
            self.release_gain *= self.release_coeff;
        }
        let excitation = self.hammer.next_sample();
        let strike_gain = self.strike_env_floor + (1.0 - self.strike_env_floor) * self.strike_env;
        self.strike_env *= self.strike_env_coeff;
        let sample = self.strings.process(excitation) * self.release_gain * strike_gain;
        if self.hammer.is_done()
            && (self.strings.energy() * self.release_gain < 1.0e-4
                || (self.state == PianoVoiceState::Releasing && self.release_gain < 2.0e-5))
        {
            self.state = PianoVoiceState::Idle;
        }
        let angle = (self.pan + 1.0) * core::f32::consts::FRAC_PI_4;
        (sample * angle.cos(), sample * angle.sin())
    }

    fn configure_release(&mut self, release_sec: f32, pedal_resonance: f32) {
        let release = release_sec.max(0.05) * (1.0 + pedal_resonance.clamp(0.0, 1.0) * 3.5);
        self.release_coeff = (-1.0 / (release * self.sample_rate)).exp();
    }

    fn configure_strike_envelope(&mut self, note: u8) {
        let note_norm =
            ((note as f32 - PIANO_KEY_MIN) / (PIANO_KEY_MAX - PIANO_KEY_MIN)).clamp(0.0, 1.0);
        let decay_sec = lerp(0.24, 0.11, note_norm);
        self.strike_env = 1.0;
        self.strike_env_coeff = (-1.0 / (decay_sec * self.sample_rate)).exp();
        self.strike_env_floor = lerp(0.33, 0.22, note_norm);
    }
}

const PIANO_KEY_MIN: f32 = 21.0;
const PIANO_KEY_MAX: f32 = 108.0;
const B_ANCHORS: [(f32, f32); 9] = [
    (21.0, 0.00045),
    (33.0, 0.00018),
    (45.0, 0.00007),
    (57.0, 0.000035),
    (69.0, 0.000025),
    (81.0, 0.000045),
    (93.0, 0.00011),
    (105.0, 0.00028),
    (108.0, 0.00035),
];

struct PianoModeSet {
    modes: [ModalMode; MAX_PIANO_PARTIALS],
    release_decays: [f32; MAX_PIANO_PARTIALS],
}

fn build_modes(
    note: u8,
    fundamental: f32,
    velocity: f32,
    params: PianoParams,
    sample_rate: f32,
) -> PianoModeSet {
    let mut modes = [ModalMode::default(); MAX_PIANO_PARTIALS];
    let mut release_decays = [0.25; MAX_PIANO_PARTIALS];
    let mut cursor = 0usize;
    let key = note as f32;
    let note_norm = ((key - PIANO_KEY_MIN) / (PIANO_KEY_MAX - PIANO_KEY_MIN)).clamp(0.0, 1.0);
    let key_center = ((note as f32 - 60.0) / 36.0).clamp(-1.0, 1.0);
    let base_b = interp_log_key(&B_ANCHORS, key) * lerp(0.35, 1.8, params.inharmonicity);
    let strike = (0.12 - 0.025 * note_norm + 0.02 * (1.0 - params.tone)).clamp(0.075, 0.16);
    let max_freq = sample_rate * 0.46;
    let string_count = if note < 45 {
        1
    } else if note < 58 {
        2
    } else {
        3
    };
    let max_harmonic = if note < 40 {
        24
    } else if note < 72 {
        18
    } else {
        12
    };
    let hammer_tau = hammer_contact_time_sec(note_norm, velocity, params.hammer_hardness);
    let register_gain = register_level(note_norm);
    let velocity_gain = velocity.powf(0.8);
    let brightness = params.brightness.clamp(0.0, 1.0);
    let strike_power = lerp(0.48, 0.90, note_norm);

    for harmonic in 1..=max_harmonic {
        let n = harmonic as f32;
        let frequency = fundamental * n * (1.0 + base_b * n * n).sqrt();
        if frequency >= max_freq {
            break;
        }
        let strike_gain = (core::f32::consts::PI * n * strike).sin().abs();
        let hammer_spectrum =
            (-((core::f32::consts::PI * frequency * hammer_tau).powf(1.35))).exp();
        let harmonic_tilt = 1.0 / n.powf(0.55 + 0.35 * (1.0 - brightness));
        let fundamental_support = 1.0 + 1.25 * (-(n - 1.0).powf(2.0) * 1.85).exp();
        let low_partial_trim =
            1.0 / (1.0 + (1.0 - note_norm).powf(1.4) * 0.16 * (n - 1.0).powf(1.15));
        let upper_partial_trim = 1.0 / (1.0 + note_norm.powf(1.55) * 0.55 * (n - 1.0).powf(0.85));
        let mid_focus = (-(note_norm - 0.45).powf(2.0) / 0.055).exp();
        let reference_partial_lift = 1.0
            + mid_focus
                * (0.20 * (-(n - 2.0).powf(2.0) * 2.0).exp()
                    + 0.55 * (-(n - 4.0).powf(2.0) * 2.0).exp());
        let gain = velocity_gain
            * register_gain
            * strike_gain.powf(strike_power)
            * hammer_spectrum
            * harmonic_tilt
            * fundamental_support
            * low_partial_trim
            * upper_partial_trim
            * reference_partial_lift
            * bridge_coupling(frequency, note_norm)
            * (0.84 + brightness * 0.32);
        let slow_decay = modal_decay_sec(
            frequency,
            n,
            note_norm,
            params.decay,
            params.pedal_resonance,
            params.sympathetic_amount,
        );
        let release_decay = damper_decay_sec(note_norm, n, params.release, params.pedal_resonance);
        let string_spread = detune_cents(harmonic, string_count, key_center, params);

        for (string_index, cents) in string_spread.iter().copied().take(string_count).enumerate() {
            if cursor >= MAX_PIANO_PARTIALS {
                break;
            }
            let balance = string_balance(string_index, string_count);
            modes[cursor] = ModalMode {
                frequency_hz: frequency * cents_to_ratio(cents),
                gain: gain * balance * 0.12,
                decay_sec: slow_decay,
            };
            release_decays[cursor] = release_decay;
            cursor += 1;
        }

        if cursor >= MAX_PIANO_PARTIALS {
            break;
        }
    }

    PianoModeSet {
        modes,
        release_decays,
    }
}

fn hammer_contact_time_sec(note_norm: f32, velocity: f32, hardness: f32) -> f32 {
    let register_ms = 0.08 + 1.22 * (1.0 - note_norm).powf(2.2);
    let tau_ms = register_ms * (1.18 - 0.48 * hardness) * (1.08 - 0.28 * velocity);
    tau_ms.max(0.055) * 0.001
}

fn modal_decay_sec(
    frequency_hz: f32,
    harmonic: f32,
    note_norm: f32,
    decay_param: f32,
    pedal_resonance: f32,
    sympathetic_amount: f32,
) -> f32 {
    let f_khz = frequency_hz / 1000.0;
    let mid_tail = 0.45 * (-(note_norm - 0.45).powf(2.0) / 0.06).exp();
    let base = decay_param * ((2.45 - 1.15 * note_norm).max(0.45) + mid_tail);
    let partial_loss = 1.0 + 0.18 * harmonic.powf(0.85) + 0.10 * f_khz.powf(1.4);
    let sustain = (1.0 + pedal_resonance * 1.2) * (1.0 + sympathetic_amount * 0.35);
    (base * sustain / partial_loss).clamp(0.045, 12.0)
}

fn damper_decay_sec(
    note_norm: f32,
    harmonic: f32,
    release_param: f32,
    pedal_resonance: f32,
) -> f32 {
    let h_loss = 1.0 + 0.35 * harmonic.powf(0.8);
    let low_bonus = 1.0 + 1.5 * (1.0 - note_norm);
    let pedal_hold = 1.0 + pedal_resonance * 6.0;
    (release_param * low_bonus * pedal_hold / h_loss).clamp(0.03, 4.0)
}

fn bridge_coupling(frequency_hz: f32, note_norm: f32) -> f32 {
    let low_transfer = (frequency_hz / (frequency_hz + 70.0)).sqrt();
    let high_loss = 1.0 / (1.0 + 0.45 * (frequency_hz / 8500.0).powf(1.35));
    low_transfer * high_loss * (0.84 + note_norm * 0.18)
}

fn register_level(note_norm: f32) -> f32 {
    0.24 + 1.02 * note_norm.powf(0.85)
}

fn interp_log_key(anchors: &[(f32, f32)], key: f32) -> f32 {
    if key <= anchors[0].0 {
        return anchors[0].1;
    }
    for pair in anchors.windows(2) {
        let (key_a, value_a) = pair[0];
        let (key_b, value_b) = pair[1];
        if key <= key_b {
            let t = ((key - key_a) / (key_b - key_a)).clamp(0.0, 1.0);
            return (value_a.ln() + (value_b.ln() - value_a.ln()) * t).exp();
        }
    }
    anchors[anchors.len() - 1].1
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn detune_cents(
    harmonic: usize,
    string_count: usize,
    key_center: f32,
    params: PianoParams,
) -> [f32; 3] {
    let spread = (0.7 + harmonic as f32 * 0.055)
        * (1.0 + key_center.max(0.0) * 0.9)
        * (0.75 + params.sympathetic_amount * 0.65);
    match string_count {
        1 => [0.0, 0.0, 0.0],
        2 => [-spread, spread * 0.86, 0.0],
        _ => [-spread * 1.08, 0.0, spread * 0.92],
    }
}

fn string_balance(index: usize, string_count: usize) -> f32 {
    match string_count {
        1 => 1.0,
        2 => [0.52, 0.48, 0.0][index],
        _ => [0.34, 0.32, 0.34][index],
    }
}

fn cents_to_ratio(cents: f32) -> f32 {
    2.0_f32.powf(cents / 1200.0)
}
