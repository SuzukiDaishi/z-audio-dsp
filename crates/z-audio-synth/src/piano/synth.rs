//! Polyphonic modal piano.

use z_audio_dsp::{BodyResonator, EventKind, ParamId, ProcessContext, TimedEvent, db_to_linear};

use super::{PianoParams, PianoVoice};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormulaPianoConfig {
    pub sample_rate: f32,
    pub max_block_size: usize,
    pub max_polyphony: usize,
}

impl Default for FormulaPianoConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            max_block_size: 512,
            max_polyphony: 32,
        }
    }
}

pub struct FormulaPiano {
    sample_rate: f32,
    voices: Vec<PianoVoice>,
    next_activation_id: u64,
    params: PianoParams,
    body: BodyResonator,
}

impl FormulaPiano {
    pub fn new(config: FormulaPianoConfig) -> Self {
        let voices = (0..config.max_polyphony.max(1))
            .map(|i| PianoVoice::new(0x5049_414e_u32.wrapping_add(i as u32)))
            .collect();
        let mut piano = Self {
            sample_rate: config.sample_rate,
            voices,
            next_activation_id: 0,
            params: PianoParams::default(),
            body: BodyResonator::default(),
        };
        piano.prepare();
        piano
    }

    fn prepare(&mut self) {
        for voice in &mut self.voices {
            voice.prepare(self.sample_rate);
        }
        self.body.prepare(self.sample_rate);
        self.refresh_body_params();
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.is_active()).count()
    }

    pub fn set_param(&mut self, id: ParamId, value: f32) {
        let m = id.metadata();
        let clamped = value.clamp(m.min, m.max);
        match id {
            ParamId::PianoTone => self.params.tone = clamped,
            ParamId::PianoBrightness => self.params.brightness = clamped,
            ParamId::PianoHammerHardness => self.params.hammer_hardness = clamped,
            ParamId::PianoHammerNoise => self.params.hammer_noise = clamped,
            ParamId::PianoInharmonicity => self.params.inharmonicity = clamped,
            ParamId::PianoDecay => self.params.decay = clamped,
            ParamId::PianoRelease => self.params.release = clamped,
            ParamId::PianoBodyAmount => self.params.body_amount = clamped,
            ParamId::PianoStereoWidth => self.params.stereo_width = clamped,
            ParamId::PianoSympatheticAmount => self.params.sympathetic_amount = clamped,
            ParamId::PianoPedalResonance => self.params.pedal_resonance = clamped,
            ParamId::PianoMasterGain => self.params.master_gain_db = clamped,
            _ => {}
        }
        self.params = self.params.sanitized();
        self.refresh_body_params();
        for voice in &mut self.voices {
            voice.apply_params(self.params);
        }
    }

    pub fn param_value(&self, id: ParamId) -> f32 {
        match id {
            ParamId::PianoTone => self.params.tone,
            ParamId::PianoBrightness => self.params.brightness,
            ParamId::PianoHammerHardness => self.params.hammer_hardness,
            ParamId::PianoHammerNoise => self.params.hammer_noise,
            ParamId::PianoInharmonicity => self.params.inharmonicity,
            ParamId::PianoDecay => self.params.decay,
            ParamId::PianoRelease => self.params.release,
            ParamId::PianoBodyAmount => self.params.body_amount,
            ParamId::PianoStereoWidth => self.params.stereo_width,
            ParamId::PianoSympatheticAmount => self.params.sympathetic_amount,
            ParamId::PianoPedalResonance => self.params.pedal_resonance,
            ParamId::PianoMasterGain => self.params.master_gain_db,
            _ => id.metadata().default,
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: f32) {
        let index = self.find_voice_for_note_on();
        self.next_activation_id = self.next_activation_id.wrapping_add(1);
        self.voices[index].note_on(note, velocity, self.params, self.next_activation_id);
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
        let master = db_to_linear(self.params.master_gain_db);
        for i in 0..left.len() {
            while event_index < ctx.events.len() && ctx.events[event_index].sample_offset == i {
                self.handle_event(ctx.events[event_index]);
                event_index += 1;
            }
            let mut l = 0.0;
            let mut r = 0.0;
            for voice in &mut self.voices {
                let (vl, vr) = voice.next_sample();
                l += vl;
                r += vr;
            }
            let body_send = self.body_send_amount();
            let body_input = (l + r) * 0.5 * (1.0 + self.params.pedal_resonance * 0.12);
            let (body_l, body_r) = self.body.process(body_input);
            let body_gain = body_send * 0.10;
            left[i] = (l + body_l * body_gain) * master;
            right[i] = (r + body_r * body_gain) * master;
        }
    }

    fn refresh_body_params(&mut self) {
        self.body.set_params(1.0, self.params.stereo_width);
    }

    fn body_send_amount(&self) -> f32 {
        (self.params.body_amount * 0.70
            + self.params.sympathetic_amount * 0.08
            + self.params.pedal_resonance * 0.05)
            .clamp(0.0, 1.0)
    }

    fn handle_event(&mut self, event: TimedEvent) {
        match event.kind {
            EventKind::NoteOn { note, velocity } => self.note_on(note, velocity),
            EventKind::NoteOff { note, .. } => self.note_off(note),
            EventKind::Param { id, value } => self.set_param(id, value),
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

    const TEST_SAMPLE_RATE: f32 = 48_000.0;

    fn render_rms(piano: &mut FormulaPiano, blocks: usize) -> f32 {
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        let mut sum = 0.0;
        let mut count = 0;
        for _ in 0..blocks {
            piano.process(&mut left, &mut right);
            sum += left.iter().map(|s| s * s).sum::<f32>();
            count += left.len();
        }
        (sum / count as f32).sqrt()
    }

    fn collect_mono(piano: &mut FormulaPiano, samples: usize) -> Vec<f32> {
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        let mut out = Vec::with_capacity(samples);
        while out.len() < samples {
            piano.process(&mut left, &mut right);
            for (l, r) in left.iter().zip(right.iter()) {
                if out.len() == samples {
                    break;
                }
                out.push((l + r) * 0.5);
            }
        }
        out
    }

    fn partial_magnitude(samples: &[f32], frequency_hz: f32) -> f32 {
        let step = core::f32::consts::TAU * frequency_hz / TEST_SAMPLE_RATE;
        let mut re = 0.0;
        let mut im = 0.0;
        for (i, sample) in samples.iter().enumerate() {
            let phase = step * i as f32;
            re += sample * phase.cos();
            im -= sample * phase.sin();
        }
        (re.mul_add(re, im * im).sqrt() * 2.0) / samples.len() as f32
    }

    fn windowed_partial_magnitude(samples: &[f32], frequency_hz: f32) -> f32 {
        let step = core::f32::consts::TAU * frequency_hz / TEST_SAMPLE_RATE;
        let mut re = 0.0;
        let mut im = 0.0;
        for (i, sample) in samples.iter().enumerate() {
            let window = if samples.len() <= 1 {
                1.0
            } else {
                0.5 - 0.5 * (core::f32::consts::TAU * i as f32 / (samples.len() - 1) as f32).cos()
            };
            let phase = step * i as f32;
            re += sample * window * phase.cos();
            im -= sample * window * phase.sin();
        }
        (re.mul_add(re, im * im).sqrt() * 4.0) / samples.len() as f32
    }

    fn partial_band_magnitude(samples: &[f32], fundamental_hz: f32, harmonic: usize) -> f32 {
        let nominal = fundamental_hz * harmonic as f32;
        let width = if harmonic == 1 { 0.006 } else { 0.022 };
        let mut max = 0.0_f32;
        for i in 0..25 {
            let t = i as f32 / 24.0;
            let frequency = nominal * (1.0 - width + 2.0 * width * t);
            max = max.max(windowed_partial_magnitude(samples, frequency));
        }
        max
    }

    fn decay_time(samples: &[f32], percent: f32) -> Option<f32> {
        let win = (TEST_SAMPLE_RATE * 0.010) as usize;
        let frames = samples.len() / win;
        if frames == 0 {
            return None;
        }
        let mut env = Vec::with_capacity(frames);
        for frame in 0..frames {
            let start = frame * win;
            let sum = samples[start..start + win]
                .iter()
                .map(|sample| sample * sample)
                .sum::<f32>();
            env.push((sum / win as f32).sqrt());
        }
        let (peak_index, peak) =
            env.iter()
                .copied()
                .enumerate()
                .fold(
                    (0usize, 0.0_f32),
                    |best, item| {
                        if item.1 > best.1 { item } else { best }
                    },
                );
        env.iter()
            .enumerate()
            .skip(peak_index)
            .find(|(_, value)| **value <= peak * percent)
            .map(|(index, _)| (index - peak_index) as f32 * win as f32 / TEST_SAMPLE_RATE)
    }

    #[test]
    fn note_on_produces_modal_piano_signal() {
        let mut piano = FormulaPiano::new(FormulaPianoConfig {
            max_block_size: 128,
            max_polyphony: 8,
            ..Default::default()
        });
        piano.note_on(60, 1.0);
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        piano.process(&mut left, &mut right);
        assert!(left.iter().any(|s| s.abs() > 0.0));
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    }

    #[test]
    fn velocity_changes_loudness() {
        let mut soft = FormulaPiano::new(FormulaPianoConfig::default());
        let mut loud = FormulaPiano::new(FormulaPianoConfig::default());
        soft.note_on(60, 0.2);
        loud.note_on(60, 1.0);
        let soft_rms = render_rms(&mut soft, 20);
        let loud_rms = render_rms(&mut loud, 20);
        assert!(
            loud_rms > soft_rms * 2.0,
            "soft={soft_rms}, loud={loud_rms}"
        );
    }

    #[test]
    fn note_off_releases_voice() {
        let mut piano = FormulaPiano::new(FormulaPianoConfig::default());
        piano.set_param(ParamId::PianoRelease, 0.05);
        piano.note_on(60, 1.0);
        piano.note_off(60);
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        for _ in 0..400 {
            piano.process(&mut left, &mut right);
        }
        assert_eq!(piano.active_voice_count(), 0);
    }

    #[test]
    fn low_mid_high_notes_remain_finite() {
        for note in [33, 60, 84] {
            let mut piano = FormulaPiano::new(FormulaPianoConfig::default());
            piano.note_on(note, 0.85);
            let samples = collect_mono(&mut piano, 4096);
            let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
            assert!(
                samples.iter().all(|s| s.is_finite()),
                "note {note} produced non-finite samples"
            );
            assert!(rms > 1.0e-5, "note {note} rms={rms}");
        }
    }

    #[test]
    fn pedal_resonance_extends_release_tail() {
        let mut dry = FormulaPiano::new(FormulaPianoConfig::default());
        let mut pedal = FormulaPiano::new(FormulaPianoConfig::default());
        dry.set_param(ParamId::PianoPedalResonance, 0.0);
        pedal.set_param(ParamId::PianoPedalResonance, 1.0);
        for piano in [&mut dry, &mut pedal] {
            piano.set_param(ParamId::PianoRelease, 0.08);
            piano.set_param(ParamId::PianoSympatheticAmount, 0.0);
            piano.note_on(57, 0.9);
            let _ = render_rms(piano, 12);
            piano.note_off(57);
        }
        let _ = render_rms(&mut dry, 90);
        let _ = render_rms(&mut pedal, 90);
        let dry_tail = render_rms(&mut dry, 40);
        let pedal_tail = render_rms(&mut pedal, 40);
        assert!(
            pedal_tail > dry_tail * 1.6,
            "dry_tail={dry_tail}, pedal_tail={pedal_tail}"
        );
    }

    #[test]
    fn sympathetic_amount_changes_rendered_output() {
        let mut dry = FormulaPiano::new(FormulaPianoConfig::default());
        let mut resonant = FormulaPiano::new(FormulaPianoConfig::default());
        dry.set_param(ParamId::PianoSympatheticAmount, 0.0);
        resonant.set_param(ParamId::PianoSympatheticAmount, 1.0);
        dry.note_on(64, 0.75);
        resonant.note_on(64, 0.75);
        let dry_samples = collect_mono(&mut dry, 4096);
        let resonant_samples = collect_mono(&mut resonant, 4096);
        let diff = dry_samples
            .iter()
            .zip(resonant_samples.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            / dry_samples.len() as f32;
        assert!(diff > 1.0e-5, "sympathetic diff={diff}");
    }

    #[test]
    fn spectral_regression_has_fundamental_and_controlled_partials() {
        for (note, fundamental) in [(45, 110.0), (69, 440.0), (81, 880.0)] {
            let mut piano = FormulaPiano::new(FormulaPianoConfig::default());
            piano.set_param(ParamId::PianoBrightness, 0.55);
            piano.note_on(note, 0.95);
            let _ = collect_mono(&mut piano, 1024);
            let samples = collect_mono(&mut piano, 8192);
            let f0 = partial_magnitude(&samples, fundamental);
            let p2 = partial_magnitude(&samples, fundamental * 2.0);
            let p3 = partial_magnitude(&samples, fundamental * 3.0);
            let high = partial_magnitude(&samples, fundamental * 10.0);
            assert!(f0 > 1.0e-5, "note {note} f0={f0}");
            assert!(
                p2 > f0 * 0.02 || p3 > f0 * 0.02,
                "note {note} f0={f0}, p2={p2}, p3={p3}"
            );
            assert!(
                high < f0 * 1.3,
                "note {note} high partial too strong: f0={f0}, high={high}"
            );
        }
    }

    #[test]
    fn low_register_keeps_fundamental_forward() {
        let mut piano = FormulaPiano::new(FormulaPianoConfig::default());
        piano.note_on(33, 0.95);
        let _ = collect_mono(&mut piano, 2048);
        let samples = collect_mono(&mut piano, 8192);
        let f0 = partial_magnitude(&samples, 55.0);
        let p2 = partial_magnitude(&samples, 110.0);
        assert!(
            f0 > p2 * 1.15,
            "low note fundamental is buried: f0={f0}, p2={p2}"
        );
    }

    #[test]
    fn c_notes_keep_fundamental_as_pitch_anchor() {
        for (note, fundamental) in [(36, 65.406), (60, 261.626), (84, 1046.502)] {
            let mut piano = FormulaPiano::new(FormulaPianoConfig::default());
            piano.note_on(note, 0.95);
            let _ = collect_mono(&mut piano, 2048);
            let samples = collect_mono(&mut piano, 8192);
            let f0 = partial_magnitude(&samples, fundamental);
            let p2 = partial_magnitude(&samples, fundamental * 2.0);
            assert!(
                f0 > p2 * 1.05,
                "C note pitch anchor is weak: note={note}, f0={f0}, p2={p2}"
            );
        }
    }

    #[test]
    fn c4_partials_match_reference_target_band() {
        let mut piano = FormulaPiano::new(FormulaPianoConfig::default());
        piano.note_on(60, 0.95);
        let _ = collect_mono(&mut piano, (TEST_SAMPLE_RATE * 0.08) as usize);
        let samples = collect_mono(&mut piano, (TEST_SAMPLE_RATE * 1.10) as usize);
        let f0 = partial_band_magnitude(&samples, 261.626, 1);
        let p2 = partial_band_magnitude(&samples, 261.626, 2) / f0;
        let p3 = partial_band_magnitude(&samples, 261.626, 3) / f0;
        let p4 = partial_band_magnitude(&samples, 261.626, 4) / f0;
        assert!(
            (0.28..=0.38).contains(&p2),
            "C4 p2/f0 outside reference-fit target: {p2}"
        );
        assert!(
            (0.06..=0.13).contains(&p3),
            "C4 p3/f0 outside reference-fit target: {p3}"
        );
        assert!(
            (0.015..=0.05).contains(&p4),
            "C4 p4/f0 outside reference-fit target: {p4}"
        );
    }

    #[test]
    fn c4_decay_follows_reference_shape() {
        let mut piano = FormulaPiano::new(FormulaPianoConfig::default());
        piano.note_on(60, 0.95);
        let samples = collect_mono(&mut piano, (TEST_SAMPLE_RATE * 3.0) as usize);
        let d50 = decay_time(&samples, 0.50).expect("C4 should decay below 50%");
        let d25 = decay_time(&samples, 0.25).expect("C4 should decay below 25%");
        let d10 = decay_time(&samples, 0.10).expect("C4 should decay below 10%");
        assert!(
            (0.12..=0.25).contains(&d50),
            "C4 decay50 outside reference-fit target: {d50}"
        );
        assert!(
            (0.45..=0.85).contains(&d25),
            "C4 decay25 outside reference-fit target: {d25}"
        );
        assert!(
            (1.80..=2.80).contains(&d10),
            "C4 decay10 outside reference-fit target: {d10}"
        );
    }

    #[test]
    fn low_register_does_not_overpower_middle_register() {
        let mut low = FormulaPiano::new(FormulaPianoConfig::default());
        let mut middle = FormulaPiano::new(FormulaPianoConfig::default());
        low.note_on(33, 0.95);
        middle.note_on(69, 0.95);
        let low_rms = render_rms(&mut low, 60);
        let middle_rms = render_rms(&mut middle, 60);
        assert!(
            low_rms < middle_rms * 2.5,
            "low register too loud: low={low_rms}, middle={middle_rms}"
        );
    }
}
