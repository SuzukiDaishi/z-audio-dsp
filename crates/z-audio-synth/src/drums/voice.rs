//! One drum hit voice.

use z_audio_dsp::midi_note_to_hz;

use super::DrumKitParams;

const MAX_MODES: usize = 10;
const TAU: f32 = core::f32::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrumVoiceState {
    #[default]
    Idle,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrumInstrument {
    Kick,
    Snare,
    Rim,
    Tom,
    ClosedHat,
    PedalHat,
    OpenHat,
    Crash,
    Ride,
    RideBell,
}

impl DrumInstrument {
    pub fn from_midi_note(note: u8) -> Self {
        match note {
            35 | 36 => Self::Kick,
            37 | 39 => Self::Rim,
            38 | 40 => Self::Snare,
            41 | 43 | 45 | 47 | 48 | 50 => Self::Tom,
            42 => Self::ClosedHat,
            44 => Self::PedalHat,
            46 => Self::OpenHat,
            49 | 52 | 55 | 57 => Self::Crash,
            53 => Self::RideBell,
            51 | 59 => Self::Ride,
            _ if note < 38 => Self::Kick,
            _ if note < 46 => Self::Snare,
            _ if note < 51 => Self::Tom,
            _ => Self::Ride,
        }
    }

    pub fn is_hat(self) -> bool {
        matches!(self, Self::ClosedHat | Self::PedalHat | Self::OpenHat)
    }

    pub fn is_open_hat(self) -> bool {
        self == Self::OpenHat
    }
}

pub struct DrumVoice {
    state: DrumVoiceState,
    instrument: DrumInstrument,
    note: u8,
    activation_id: u64,
    sample_rate: f32,
    modes: [DampedMode; MAX_MODES],
    mode_count: usize,
    pan: f32,
    noise_amp: f32,
    noise_decay: f32,
    noise_low_state: f32,
    noise_low_coeff: f32,
    noise_low_reject: f32,
    click_amp: f32,
    click_decay: f32,
    kick_phase: f32,
    kick_base_hz: f32,
    kick_amp: f32,
    kick_decay: f32,
    kick_pitch_env: f32,
    kick_pitch_decay: f32,
    rng: u32,
}

impl DrumVoice {
    pub fn new(seed: u32) -> Self {
        Self {
            state: DrumVoiceState::Idle,
            instrument: DrumInstrument::Kick,
            note: 0,
            activation_id: 0,
            sample_rate: 48_000.0,
            modes: [DampedMode::default(); MAX_MODES],
            mode_count: 0,
            pan: 0.0,
            noise_amp: 0.0,
            noise_decay: 0.0,
            noise_low_state: 0.0,
            noise_low_coeff: 0.08,
            noise_low_reject: 0.6,
            click_amp: 0.0,
            click_decay: 0.0,
            kick_phase: 0.0,
            kick_base_hz: 52.0,
            kick_amp: 0.0,
            kick_decay: 0.0,
            kick_pitch_env: 0.0,
            kick_pitch_decay: 0.0,
            rng: seed.max(1),
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.state = DrumVoiceState::Idle;
    }

    pub fn is_active(&self) -> bool {
        self.state == DrumVoiceState::Active
    }

    pub fn is_open_hat(&self) -> bool {
        self.is_active() && self.instrument.is_open_hat()
    }

    pub fn activation_id(&self) -> u64 {
        self.activation_id
    }

    pub fn note(&self) -> u8 {
        self.note
    }

    pub fn choke(&mut self, amount: f32) {
        let scale = amount.clamp(0.0, 1.0);
        self.noise_amp *= scale;
        self.click_amp *= scale;
        self.kick_amp *= scale;
        for mode in self.modes.iter_mut().take(self.mode_count) {
            mode.amp *= scale;
            mode.decay = mode.decay.min(coeff(0.035, self.sample_rate));
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: f32, params: DrumKitParams, activation_id: u64) {
        let instrument = DrumInstrument::from_midi_note(note);
        let params = params.sanitized();
        self.state = DrumVoiceState::Active;
        self.instrument = instrument;
        self.note = note;
        self.activation_id = activation_id;
        self.mode_count = 0;
        self.noise_amp = 0.0;
        self.click_amp = 0.0;
        self.kick_amp = 0.0;
        self.kick_phase = 0.0;
        self.kick_pitch_env = 0.0;
        self.noise_low_state = 0.0;
        let velocity = velocity.clamp(0.0, 1.0).powf(0.78);
        match instrument {
            DrumInstrument::Kick => self.trigger_kick(note, velocity, params),
            DrumInstrument::Snare => self.trigger_snare(velocity, params),
            DrumInstrument::Rim => self.trigger_rim(velocity, params),
            DrumInstrument::Tom => self.trigger_tom(note, velocity, params),
            DrumInstrument::ClosedHat => self.trigger_hat(velocity, params, 0.055, 0.20),
            DrumInstrument::PedalHat => self.trigger_hat(velocity, params, 0.12, 0.14),
            DrumInstrument::OpenHat => self.trigger_hat(velocity, params, 0.62, 0.10),
            DrumInstrument::Crash => self.trigger_cymbal(velocity, params, 1.65, 0.44),
            DrumInstrument::Ride => self.trigger_cymbal(velocity, params, 2.25, 0.30),
            DrumInstrument::RideBell => self.trigger_bell(velocity, params),
        }
    }

    pub fn next_sample(&mut self) -> (f32, f32) {
        if self.state == DrumVoiceState::Idle {
            return (0.0, 0.0);
        }
        let mut sample = 0.0;
        if self.kick_amp > 1.0e-6 {
            let pitch = self.kick_base_hz * (1.0 + self.kick_pitch_env * 2.3);
            self.kick_phase = wrap_phase(self.kick_phase + TAU * pitch / self.sample_rate);
            sample += self.kick_phase.sin() * self.kick_amp;
            self.kick_amp *= self.kick_decay;
            self.kick_pitch_env *= self.kick_pitch_decay;
        }
        for mode in self.modes.iter_mut().take(self.mode_count) {
            sample += mode.process(self.sample_rate);
        }
        if self.noise_amp > 1.0e-6 {
            let raw = self.noise();
            self.noise_low_state += (raw - self.noise_low_state) * self.noise_low_coeff;
            let filtered = raw - self.noise_low_state * self.noise_low_reject;
            sample += filtered * self.noise_amp;
            self.noise_amp *= self.noise_decay;
        }
        if self.click_amp > 1.0e-6 {
            let snap = 0.7 + 0.3 * self.noise();
            sample += snap * self.click_amp;
            self.click_amp *= self.click_decay;
        }
        if self.energy() < 1.0e-5 {
            self.state = DrumVoiceState::Idle;
        }
        let angle = (self.pan + 1.0) * core::f32::consts::FRAC_PI_4;
        (sample * angle.cos(), sample * angle.sin())
    }

    fn trigger_kick(&mut self, note: u8, velocity: f32, params: DrumKitParams) {
        let gain = velocity * params.kick_level;
        let tuning = semitone_ratio(params.tuning_semitones);
        let note_offset = if note == 35 { 0.88 } else { 1.0 };
        self.kick_base_hz = 51.0 * tuning * note_offset;
        self.kick_amp = 1.55 * gain;
        self.kick_decay = coeff(0.34 * params.decay_scale, self.sample_rate);
        self.kick_pitch_env = 1.0;
        self.kick_pitch_decay = coeff(0.032 + 0.018 * (1.0 - params.tone), self.sample_rate);
        self.click_amp = gain * (0.13 + params.tone * 0.18);
        self.click_decay = coeff(0.0045 + 0.002 * (1.0 - params.tone), self.sample_rate);
        self.noise_amp = gain * 0.035 * (0.5 + params.tone);
        self.noise_decay = coeff(0.026, self.sample_rate);
        self.noise_low_coeff = 0.20;
        self.noise_low_reject = 0.55;
        self.pan = -0.04 * params.stereo_width;
        self.add_mode(92.0 * tuning, 0.13 * gain, 0.16 * params.decay_scale, 0.15);
        self.add_mode(138.0 * tuning, 0.055 * gain, 0.055, 0.41);
    }

    fn trigger_snare(&mut self, velocity: f32, params: DrumKitParams) {
        let gain = velocity * params.snare_level;
        let tuning = semitone_ratio(params.tuning_semitones * 0.55);
        let decay = params.decay_scale;
        self.add_mode(178.0 * tuning, 0.38 * gain, 0.20 * decay, 0.08);
        self.add_mode(332.0 * tuning, 0.23 * gain, 0.13 * decay, 0.21);
        self.add_mode(472.0 * tuning, 0.12 * gain, 0.075 * decay, 0.37);
        self.add_mode(690.0 * tuning, 0.055 * gain, 0.045, 0.53);
        self.noise_amp = gain * (0.32 + params.snare_wire * 0.72) * (0.65 + params.tone * 0.45);
        self.noise_decay = coeff(
            (0.105 + params.snare_wire * 0.115) * decay,
            self.sample_rate,
        );
        self.noise_low_coeff = 0.12 + params.tone * 0.12;
        self.noise_low_reject = 0.45 + params.tone * 0.35;
        self.click_amp = gain * 0.075 * (0.7 + params.tone);
        self.click_decay = coeff(0.004, self.sample_rate);
        self.pan = 0.08 * params.stereo_width;
    }

    fn trigger_rim(&mut self, velocity: f32, params: DrumKitParams) {
        let gain = velocity * params.snare_level;
        let tuning = semitone_ratio(params.tuning_semitones * 0.35);
        self.add_mode(820.0 * tuning, 0.34 * gain, 0.12, 0.00);
        self.add_mode(1460.0 * tuning, 0.25 * gain, 0.075, 0.31);
        self.add_mode(2380.0 * tuning, 0.11 * gain, 0.045, 0.62);
        self.click_amp = gain * 0.16;
        self.click_decay = coeff(0.003, self.sample_rate);
        self.pan = 0.18 * params.stereo_width;
    }

    fn trigger_tom(&mut self, note: u8, velocity: f32, params: DrumKitParams) {
        let gain = velocity * params.tom_level;
        let base = match note {
            41 => 82.0,
            43 => 92.0,
            45 => 110.0,
            47 => 130.0,
            48 => 146.0,
            50 => 165.0,
            _ => midi_note_to_hz(note as f32 - 24.0).clamp(80.0, 190.0),
        } * semitone_ratio(params.tuning_semitones);
        let decay = params.decay_scale;
        self.add_mode(base, 0.78 * gain, 0.50 * decay, 0.02);
        self.add_mode(base * 1.59, 0.26 * gain, 0.24 * decay, 0.24);
        self.add_mode(base * 2.14, 0.13 * gain, 0.15 * decay, 0.43);
        self.add_mode(base * 2.30, 0.08 * gain, 0.10 * decay, 0.58);
        self.noise_amp = gain * 0.045 * (0.7 + params.tone);
        self.noise_decay = coeff(0.030, self.sample_rate);
        self.noise_low_coeff = 0.16;
        self.noise_low_reject = 0.40 + params.tone * 0.25;
        self.click_amp = gain * 0.045;
        self.click_decay = coeff(0.004, self.sample_rate);
        self.pan = ((note as f32 - 46.0) / 8.0).clamp(-1.0, 1.0) * 0.42 * params.stereo_width;
    }

    fn trigger_hat(&mut self, velocity: f32, params: DrumKitParams, decay_sec: f32, body: f32) {
        let gain = velocity * params.hat_level;
        let tuning = semitone_ratio(params.tuning_semitones * 0.20);
        let frequencies = [6100.0, 7600.0, 9300.0, 10_900.0, 12_700.0, 14_400.0];
        for (i, freq) in frequencies.iter().copied().enumerate() {
            let amp = gain * body * (0.16 / (1.0 + i as f32 * 0.22));
            self.add_mode(
                freq * tuning,
                amp,
                decay_sec * (0.55 + i as f32 * 0.08),
                i as f32 * 0.17,
            );
        }
        self.noise_amp = gain * (0.45 + params.tone * 0.55);
        self.noise_decay = coeff(decay_sec * params.decay_scale, self.sample_rate);
        self.noise_low_coeff = 0.035 + params.tone * 0.035;
        self.noise_low_reject = 0.92;
        self.click_amp = gain * 0.055;
        self.click_decay = coeff(0.0025, self.sample_rate);
        self.pan = 0.34 * params.stereo_width;
    }

    fn trigger_cymbal(
        &mut self,
        velocity: f32,
        params: DrumKitParams,
        decay_sec: f32,
        ping_amount: f32,
    ) {
        let gain = velocity * params.cymbal_level;
        let tuning = semitone_ratio(params.tuning_semitones * 0.18);
        let frequencies = [
            760.0, 1180.0, 1830.0, 2710.0, 4020.0, 5680.0, 7950.0, 10_900.0,
        ];
        for (i, freq) in frequencies.iter().copied().enumerate() {
            let high = i as f32 / (frequencies.len() - 1) as f32;
            let amp = gain * (0.12 + ping_amount * 0.14) * (0.45 + high) / (i as f32 + 1.0);
            self.add_mode(freq * tuning, amp, decay_sec * (0.60 + high * 0.70), high);
        }
        self.noise_amp = gain * (0.22 + params.tone * 0.38);
        self.noise_decay = coeff(decay_sec * params.decay_scale, self.sample_rate);
        self.noise_low_coeff = 0.030 + params.tone * 0.025;
        self.noise_low_reject = 0.86;
        self.click_amp = gain * 0.035;
        self.click_decay = coeff(0.003, self.sample_rate);
        self.pan = 0.55 * params.stereo_width;
    }

    fn trigger_bell(&mut self, velocity: f32, params: DrumKitParams) {
        let gain = velocity * params.cymbal_level;
        let tuning = semitone_ratio(params.tuning_semitones * 0.20);
        self.add_mode(
            2360.0 * tuning,
            0.34 * gain,
            0.95 * params.decay_scale,
            0.00,
        );
        self.add_mode(
            3540.0 * tuning,
            0.18 * gain,
            0.62 * params.decay_scale,
            0.31,
        );
        self.add_mode(
            5310.0 * tuning,
            0.10 * gain,
            0.45 * params.decay_scale,
            0.62,
        );
        self.noise_amp = gain * 0.10;
        self.noise_decay = coeff(0.22 * params.decay_scale, self.sample_rate);
        self.noise_low_coeff = 0.06;
        self.noise_low_reject = 0.82;
        self.click_amp = gain * 0.05;
        self.click_decay = coeff(0.0025, self.sample_rate);
        self.pan = 0.46 * params.stereo_width;
    }

    fn add_mode(&mut self, frequency_hz: f32, amp: f32, decay_sec: f32, phase: f32) {
        if self.mode_count >= MAX_MODES || frequency_hz >= self.sample_rate * 0.46 {
            return;
        }
        self.modes[self.mode_count] = DampedMode::new(
            frequency_hz.max(20.0),
            amp,
            decay_sec,
            phase,
            self.sample_rate,
        );
        self.mode_count += 1;
    }

    fn noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn energy(&self) -> f32 {
        let modal = self
            .modes
            .iter()
            .take(self.mode_count)
            .map(|m| m.amp.abs())
            .sum::<f32>();
        modal + self.noise_amp.abs() + self.click_amp.abs() + self.kick_amp.abs()
    }
}

#[derive(Debug, Clone, Copy)]
struct DampedMode {
    frequency_hz: f32,
    phase: f32,
    amp: f32,
    decay: f32,
}

impl Default for DampedMode {
    fn default() -> Self {
        Self {
            frequency_hz: 440.0,
            phase: 0.0,
            amp: 0.0,
            decay: 0.0,
        }
    }
}

impl DampedMode {
    fn new(
        frequency_hz: f32,
        amp: f32,
        decay_sec: f32,
        phase_turns: f32,
        sample_rate: f32,
    ) -> Self {
        Self {
            frequency_hz,
            phase: phase_turns.fract() * TAU,
            amp,
            decay: coeff(decay_sec, sample_rate),
        }
    }

    fn process(&mut self, sample_rate: f32) -> f32 {
        if self.amp.abs() <= 1.0e-8 {
            return 0.0;
        }
        let out = self.phase.sin() * self.amp;
        self.phase = wrap_phase(self.phase + TAU * self.frequency_hz / sample_rate);
        self.amp *= self.decay;
        out
    }
}

fn coeff(decay_sec: f32, sample_rate: f32) -> f32 {
    (-1.0 / (decay_sec.max(0.001) * sample_rate.max(1.0))).exp()
}

fn semitone_ratio(semitones: f32) -> f32 {
    2.0_f32.powf(semitones / 12.0)
}

fn wrap_phase(phase: f32) -> f32 {
    if phase >= TAU { phase - TAU } else { phase }
}
