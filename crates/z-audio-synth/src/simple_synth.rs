//! Fixed-chain simple synthesizer: voices summed, then 3-band EQ, then
//! master gain.

use z_audio_dsp::{
    ButterworthKind, Effect, EnvelopeCurve, EnvelopeParams, EventKind, Gain, GeneratorKind,
    GeneratorParams, LfoParams, LfoTarget, LfoWaveform, ParamId, ProcessContext,
    ThreeBandButterworthEq,
};

use crate::voice_manager::{VoiceManager, VoiceStealPolicy};

/// Configuration used to construct a [`SimpleSynth`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimpleSynthConfig {
    pub sample_rate: f32,
    pub max_block_size: usize,
    pub max_polyphony: usize,
}

impl Default for SimpleSynthConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            max_block_size: 512,
            max_polyphony: 16,
        }
    }
}

/// Encodes a `bool`-valued parameter as the `0.0`/`1.0` automation value
/// expected by [`SimpleSynth::set_param`]/returned by
/// [`SimpleSynth::param_value`].
fn bool_to_param_value(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

fn updates_active_voice(id: ParamId) -> bool {
    matches!(
        id,
        ParamId::GeneratorKind
            | ParamId::GeneratorGain
            | ParamId::GeneratorPulseWidth
            | ParamId::GeneratorPan
            | ParamId::EnvAttack
            | ParamId::EnvDecay
            | ParamId::EnvSustain
            | ParamId::EnvRelease
            | ParamId::EnvCurve
            | ParamId::LfoEnabled
            | ParamId::LfoWaveform
            | ParamId::LfoRateHz
            | ParamId::LfoAmount
            | ParamId::LfoTarget
            | ParamId::LfoRetrigger
    )
}

/// A fixed-chain simple synthesizer:
///
/// ```text
/// MIDI/Event Input -> VoiceManager -> Voice Sum -> 3-band Butterworth EQ -> master gain -> Output
/// ```
///
/// The LFO of voice slot 0 additionally drives EQ band-frequency modulation
/// when its target is one of the `Eq*Freq` variants; see
/// [`SimpleSynth::apply_lfo_eq_routing`].
pub struct SimpleSynth {
    sample_rate: f32,
    max_block_size: usize,
    voices: VoiceManager,
    generator_params: GeneratorParams,
    amp_env_params: EnvelopeParams,
    lfo_params: LfoParams,
    eq: ThreeBandButterworthEq,
    master_gain: Gain,
    eq_base_low_freq_hz: f32,
    eq_base_mid_freq_hz: f32,
    eq_base_high_freq_hz: f32,
}

impl SimpleSynth {
    /// Creates a new synth and prepares it for processing.
    pub fn new(config: SimpleSynthConfig) -> Self {
        let eq = ThreeBandButterworthEq::new();
        let mut synth = Self {
            sample_rate: config.sample_rate,
            max_block_size: config.max_block_size,
            voices: VoiceManager::new(config.max_polyphony, 0x5EED_5EED),
            generator_params: GeneratorParams::default(),
            amp_env_params: EnvelopeParams::default(),
            lfo_params: LfoParams::default(),
            eq_base_low_freq_hz: eq.low.frequency_hz,
            eq_base_mid_freq_hz: eq.mid.frequency_hz,
            eq_base_high_freq_hz: eq.high.frequency_hz,
            eq,
            master_gain: Gain::default(),
        };
        synth.prepare();
        synth
    }

    fn prepare(&mut self) {
        self.voices.prepare(self.sample_rate, self.max_block_size);
        self.eq.prepare(self.sample_rate, self.max_block_size);
        self.master_gain
            .prepare(self.sample_rate, self.max_block_size);
    }

    /// Returns the configured sample rate.
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Returns the maximum block size this synth was prepared for.
    pub fn max_block_size(&self) -> usize {
        self.max_block_size
    }

    /// Returns the number of currently-active (non-idle) voices.
    pub fn active_voice_count(&self) -> usize {
        self.voices.active_count()
    }

    /// Selects the oscillator type used by every new note.
    pub fn set_generator_kind(&mut self, kind: GeneratorKind) {
        self.generator_params.kind = kind;
        self.apply_realtime_voice_params();
    }

    /// Returns mutable access to the shared generator parameters (gain,
    /// phase offset, pulse width, pan) applied to every new note.
    pub fn generator_params_mut(&mut self) -> &mut GeneratorParams {
        &mut self.generator_params
    }

    /// Replaces the amplitude envelope parameters applied to every new note.
    pub fn set_amp_envelope(&mut self, params: EnvelopeParams) {
        self.amp_env_params = params;
        self.apply_realtime_voice_params();
    }

    /// Replaces the LFO parameters applied to every new note.
    pub fn set_lfo(&mut self, params: LfoParams) {
        self.lfo_params = params;
        self.apply_realtime_voice_params();
    }

    /// Returns mutable access to the 3-band EQ. Band frequency, Q, enabled
    /// state, and filter kind can be set directly, e.g.
    /// `synth.eq_mut().low.frequency_hz = 180.0;`.
    pub fn eq_mut(&mut self) -> &mut ThreeBandButterworthEq {
        &mut self.eq
    }

    /// Sets the master output gain (linear, smoothed).
    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain.set_gain(gain);
    }

    /// Returns the voice-stealing policy used when [`SimpleSynth::note_on`]
    /// is called with no idle voices remaining.
    pub fn voice_steal_policy(&self) -> VoiceStealPolicy {
        self.voices.steal_policy()
    }

    /// Sets the voice-stealing policy. See [`VoiceStealPolicy`] for the
    /// difference between `Oldest` and `ReleasedFirst`.
    pub fn set_voice_steal_policy(&mut self, policy: VoiceStealPolicy) {
        self.voices.set_steal_policy(policy);
    }

    /// Applies an automation value to `id`, per [`ParamId::metadata`]:
    /// continuous (`Linear`/`Hertz`/`Seconds`) values are clamped to
    /// `metadata().min..=metadata().max`; `Enum` values are decoded via the
    /// relevant `from_param_value` (which itself rounds and clamps);
    /// `Boolean` values are `true` when `value >= 0.5`.
    ///
    /// [`ParamId::MaxPolyphony`] is read-only and ignored.
    ///
    /// Generator gain/pulse-width/pan, envelope, and LFO changes apply to
    /// already-sounding voices with short smoothing. Generator kind changes
    /// crossfade on active voices. Generator phase offset still applies to
    /// newly-triggered notes. EQ and master-gain changes apply immediately
    /// (smoothed where applicable).
    pub fn set_param(&mut self, id: ParamId, value: f32) {
        let m = id.metadata();
        let clamped = value.clamp(m.min, m.max);
        let flag = value >= 0.5;
        let update_voices = updates_active_voice(id);

        match id {
            ParamId::MasterGain => self.master_gain.set_gain(clamped),
            ParamId::MaxPolyphony => {}
            ParamId::GeneratorKind => {
                self.generator_params.kind = GeneratorKind::from_param_value(value);
            }
            ParamId::GeneratorGain => self.generator_params.gain = clamped,
            ParamId::GeneratorPulseWidth => self.generator_params.pulse_width = clamped,
            ParamId::GeneratorPhaseOffset => self.generator_params.phase_offset = clamped,
            ParamId::GeneratorPan => self.generator_params.pan = clamped,
            ParamId::EnvAttack => self.amp_env_params.attack = clamped,
            ParamId::EnvDecay => self.amp_env_params.decay = clamped,
            ParamId::EnvSustain => self.amp_env_params.sustain = clamped,
            ParamId::EnvRelease => self.amp_env_params.release = clamped,
            ParamId::EnvCurve => self.amp_env_params.curve = EnvelopeCurve::from_param_value(value),
            ParamId::LfoEnabled => self.lfo_params.enabled = flag,
            ParamId::LfoWaveform => self.lfo_params.waveform = LfoWaveform::from_param_value(value),
            ParamId::LfoRateHz => self.lfo_params.rate_hz = clamped,
            ParamId::LfoAmount => self.lfo_params.amount = clamped,
            ParamId::LfoTarget => self.lfo_params.target = LfoTarget::from_param_value(value),
            ParamId::LfoRetrigger => self.lfo_params.retrigger = flag,
            ParamId::EqLowEnabled => self.eq.low.enabled = flag,
            ParamId::EqLowFreq => {
                self.eq.low.frequency_hz = clamped;
                self.eq_base_low_freq_hz = clamped;
            }
            ParamId::EqLowType => self.eq.low.kind = ButterworthKind::from_param_value(value),
            ParamId::EqMidEnabled => self.eq.mid.enabled = flag,
            ParamId::EqMidFreq => {
                self.eq.mid.frequency_hz = clamped;
                self.eq_base_mid_freq_hz = clamped;
            }
            ParamId::EqMidType => self.eq.mid.kind = ButterworthKind::from_param_value(value),
            ParamId::EqHighEnabled => self.eq.high.enabled = flag,
            ParamId::EqHighFreq => {
                self.eq.high.frequency_hz = clamped;
                self.eq_base_high_freq_hz = clamped;
            }
            ParamId::EqHighType => self.eq.high.kind = ButterworthKind::from_param_value(value),
            ParamId::EqLowGainDb => self.eq.low.gain_db = clamped,
            ParamId::EqLowQ => self.eq.low.q = clamped,
            ParamId::EqMidGainDb => self.eq.mid.gain_db = clamped,
            ParamId::EqMidQ => self.eq.mid.q = clamped,
            ParamId::EqHighGainDb => self.eq.high.gain_db = clamped,
            ParamId::EqHighQ => self.eq.high.q = clamped,
            _ => {}
        }

        if update_voices {
            self.apply_realtime_voice_params();
        }
    }

    /// Returns the current value of `id`, in the same encoding accepted by
    /// [`SimpleSynth::set_param`].
    pub fn param_value(&self, id: ParamId) -> f32 {
        match id {
            ParamId::MasterGain => self.master_gain.target_gain(),
            ParamId::MaxPolyphony => self.voices.max_polyphony() as f32,
            ParamId::GeneratorKind => self.generator_params.kind.to_param_value(),
            ParamId::GeneratorGain => self.generator_params.gain,
            ParamId::GeneratorPulseWidth => self.generator_params.pulse_width,
            ParamId::GeneratorPhaseOffset => self.generator_params.phase_offset,
            ParamId::GeneratorPan => self.generator_params.pan,
            ParamId::EnvAttack => self.amp_env_params.attack,
            ParamId::EnvDecay => self.amp_env_params.decay,
            ParamId::EnvSustain => self.amp_env_params.sustain,
            ParamId::EnvRelease => self.amp_env_params.release,
            ParamId::EnvCurve => self.amp_env_params.curve.to_param_value(),
            ParamId::LfoEnabled => bool_to_param_value(self.lfo_params.enabled),
            ParamId::LfoWaveform => self.lfo_params.waveform.to_param_value(),
            ParamId::LfoRateHz => self.lfo_params.rate_hz,
            ParamId::LfoAmount => self.lfo_params.amount,
            ParamId::LfoTarget => self.lfo_params.target.to_param_value(),
            ParamId::LfoRetrigger => bool_to_param_value(self.lfo_params.retrigger),
            ParamId::EqLowEnabled => bool_to_param_value(self.eq.low.enabled),
            ParamId::EqLowFreq => self.eq.low.frequency_hz,
            ParamId::EqLowType => self.eq.low.kind.to_param_value(),
            ParamId::EqMidEnabled => bool_to_param_value(self.eq.mid.enabled),
            ParamId::EqMidFreq => self.eq.mid.frequency_hz,
            ParamId::EqMidType => self.eq.mid.kind.to_param_value(),
            ParamId::EqHighEnabled => bool_to_param_value(self.eq.high.enabled),
            ParamId::EqHighFreq => self.eq.high.frequency_hz,
            ParamId::EqHighType => self.eq.high.kind.to_param_value(),
            ParamId::EqLowGainDb => self.eq.low.gain_db,
            ParamId::EqLowQ => self.eq.low.q,
            ParamId::EqMidGainDb => self.eq.mid.gain_db,
            ParamId::EqMidQ => self.eq.mid.q,
            ParamId::EqHighGainDb => self.eq.high.gain_db,
            ParamId::EqHighQ => self.eq.high.q,
            _ => id.metadata().default,
        }
    }

    /// Triggers `note` (0-127) at `velocity` (0.0-1.0), allocating or
    /// stealing a voice as needed.
    pub fn note_on(&mut self, note: u8, velocity: f32) {
        self.voices.note_on(
            note,
            velocity,
            &self.generator_params,
            &self.amp_env_params,
            &self.lfo_params,
        );
    }

    /// Releases every active voice currently playing `note`.
    pub fn note_off(&mut self, note: u8) {
        self.voices.note_off(note);
    }

    /// Renders `left.len()` samples with no events.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        let events = [];
        let ctx = ProcessContext::new(self.sample_rate, left.len(), 120.0, &events);
        self.process_with_context(&ctx, left, right);
    }

    /// Renders `left.len()` samples, applying `ctx.events` (note on/off) at
    /// their scheduled sample offsets.
    ///
    /// `ctx.events` must be sorted by `sample_offset`, and every
    /// `sample_offset` must be `< left.len()`.
    pub fn process_with_context(
        &mut self,
        ctx: &ProcessContext,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        debug_assert_eq!(left.len(), right.len());
        debug_assert_eq!(left.len(), ctx.block_size);

        let mut event_index = 0;
        for i in 0..left.len() {
            while event_index < ctx.events.len() && ctx.events[event_index].sample_offset == i {
                match ctx.events[event_index].kind {
                    EventKind::NoteOn { note, velocity } => self.note_on(note, velocity),
                    EventKind::NoteOff { note, .. } => self.note_off(note),
                    EventKind::Param { id, value } => self.set_param(id, value),
                }
                event_index += 1;
            }

            let (l, r) = self.next_sample();
            left[i] = l;
            right[i] = r;
        }
    }

    fn next_sample(&mut self) -> (f32, f32) {
        let mut sum_left = 0.0;
        let mut sum_right = 0.0;
        for voice in self.voices.voices_mut() {
            let (l, r) = voice.next_sample();
            sum_left += l;
            sum_right += r;
        }

        self.apply_lfo_eq_routing();

        let mut left = [sum_left];
        let mut right = [sum_right];
        let events = [];
        let ctx = ProcessContext::new(self.sample_rate, 1, 120.0, &events);
        self.eq.process_stereo(&ctx, &mut left, &mut right);
        self.master_gain.process_stereo(&ctx, &mut left, &mut right);

        (left[0], right[0])
    }

    fn apply_realtime_voice_params(&mut self) {
        for voice in self.voices.voices_mut() {
            if voice.is_active() {
                voice.apply_realtime_params(
                    &self.generator_params,
                    &self.amp_env_params,
                    &self.lfo_params,
                );
            }
        }
    }

    /// Routes voice slot 0's LFO to EQ band-frequency modulation when its
    /// target is one of the `Eq*Freq` variants:
    /// `frequency_hz = base_frequency_hz * 2^(lfo * amount)`.
    ///
    /// Each band's base frequency tracks the EQ's `frequency_hz` field live
    /// whenever that band is *not* the active LFO target, so direct
    /// mutation via [`SimpleSynth::eq_mut`] continues to work for
    /// un-modulated bands, and for the modulated band whenever LFO targeting
    /// is switched away from it.
    fn apply_lfo_eq_routing(&mut self) {
        let voice0 = self.voices.voice(0);
        let target = if voice0.is_active() {
            voice0.lfo().params().target
        } else {
            LfoTarget::None
        };
        let lfo_value = voice0.lfo().last_value();
        let amount = voice0.lfo().params().amount;

        if target == LfoTarget::EqLowFreq {
            self.eq.low.frequency_hz = self.eq_base_low_freq_hz * 2f32.powf(lfo_value * amount);
        } else {
            self.eq_base_low_freq_hz = self.eq.low.frequency_hz;
        }

        if target == LfoTarget::EqMidFreq {
            self.eq.mid.frequency_hz = self.eq_base_mid_freq_hz * 2f32.powf(lfo_value * amount);
        } else {
            self.eq_base_mid_freq_hz = self.eq.mid.frequency_hz;
        }

        if target == LfoTarget::EqHighFreq {
            self.eq.high.frequency_hz = self.eq_base_high_freq_hz * 2f32.powf(lfo_value * amount);
        } else {
            self.eq_base_high_freq_hz = self.eq.high.frequency_hz;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use z_audio_dsp::ParamUnit;

    fn small_synth() -> SimpleSynth {
        SimpleSynth::new(SimpleSynthConfig {
            sample_rate: 48_000.0,
            max_block_size: 128,
            max_polyphony: 4,
        })
    }

    fn render_mean_abs(synth: &mut SimpleSynth, blocks: usize) -> f32 {
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        let mut sum = 0.0_f32;
        let mut count = 0_usize;
        for _ in 0..blocks {
            synth.process(&mut left, &mut right);
            sum += left.iter().map(|sample| sample.abs()).sum::<f32>();
            count += left.len();
        }
        sum / count as f32
    }

    fn render_mean_signed(synth: &mut SimpleSynth, blocks: usize) -> f32 {
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        let mut sum = 0.0_f32;
        let mut count = 0_usize;
        for _ in 0..blocks {
            synth.process(&mut left, &mut right);
            sum += left.iter().sum::<f32>();
            count += left.len();
        }
        sum / count as f32
    }

    fn block_rms_values(synth: &mut SimpleSynth, blocks: usize) -> Vec<f32> {
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        let mut values = Vec::with_capacity(blocks);
        for _ in 0..blocks {
            synth.process(&mut left, &mut right);
            let sum_sq: f32 = left.iter().map(|sample| sample * sample).sum();
            values.push((sum_sq / left.len() as f32).sqrt());
        }
        values
    }

    fn max_adjacent_delta(samples: &[f32]) -> f32 {
        samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0_f32, f32::max)
    }

    #[test]
    fn silence_with_no_notes() {
        let mut synth = small_synth();
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        synth.process(&mut left, &mut right);
        for &s in left.iter().chain(right.iter()) {
            assert_eq!(s, 0.0);
        }
    }

    #[test]
    fn note_on_produces_finite_nonzero_signal() {
        let mut synth = small_synth();
        synth.note_on(60, 1.0);

        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        synth.process(&mut left, &mut right);

        assert_eq!(synth.active_voice_count(), 1);
        for &s in left.iter().chain(right.iter()) {
            assert!(s.is_finite());
        }
        assert!(left.iter().any(|&s| s.abs() > 0.0));
    }

    #[test]
    fn note_off_eventually_silences_output() {
        let mut synth = small_synth();
        synth.set_amp_envelope(EnvelopeParams {
            attack: 0.0,
            decay: 0.0,
            sustain: 0.5,
            release: 0.01,
            ..EnvelopeParams::default()
        });
        synth.note_on(60, 1.0);

        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        synth.process(&mut left, &mut right);
        synth.note_off(60);

        for _ in 0..20 {
            synth.process(&mut left, &mut right);
        }
        assert_eq!(synth.active_voice_count(), 0);
        for &s in left.iter().chain(right.iter()) {
            assert!((s).abs() < 1e-3);
        }
    }

    #[test]
    fn sample_accurate_note_on_event_starts_mid_block() {
        use z_audio_dsp::TimedEvent;

        let mut synth = small_synth();
        let events = [TimedEvent {
            sample_offset: 64,
            kind: EventKind::NoteOn {
                note: 60,
                velocity: 1.0,
            },
        }];
        let ctx = ProcessContext::new(48_000.0, 128, 120.0, &events);

        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        synth.process_with_context(&ctx, &mut left, &mut right);

        // Nothing should sound before the event fires.
        for &s in &left[..64] {
            assert_eq!(s, 0.0);
        }
        assert!(left[64..].iter().any(|&s| s.abs() > 0.0));
    }

    #[test]
    fn chord_sums_multiple_voices() {
        let mut synth = small_synth();
        synth.note_on(60, 1.0);
        synth.note_on(64, 1.0);
        synth.note_on(67, 1.0);
        assert_eq!(synth.active_voice_count(), 3);

        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        synth.process(&mut left, &mut right);
        for &s in left.iter().chain(right.iter()) {
            assert!(s.is_finite());
        }
    }

    #[test]
    fn eq_mut_allows_direct_field_mutation() {
        let mut synth = small_synth();
        synth.eq_mut().low.frequency_hz = 180.0;
        synth.eq_mut().low.enabled = false;
        assert_eq!(synth.eq_mut().low.frequency_hz, 180.0);
        assert!(!synth.eq_mut().low.enabled);
    }

    #[test]
    fn lfo_eq_routing_modulates_low_band_frequency() {
        let mut synth = small_synth();
        synth.set_lfo(LfoParams {
            enabled: true,
            waveform: z_audio_dsp::LfoWaveform::Sine,
            rate_hz: 10.0,
            amount: 1.0, // +/- 1 octave
            target: LfoTarget::EqLowFreq,
            retrigger: true,
        });
        let base = synth.eq_mut().low.frequency_hz;
        synth.note_on(60, 1.0);

        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        let mut max_freq: f32 = base;
        let mut min_freq: f32 = base;
        for _ in 0..50 {
            synth.process(&mut left, &mut right);
            let f = synth.eq_mut().low.frequency_hz;
            max_freq = max_freq.max(f);
            min_freq = min_freq.min(f);
        }
        assert!(max_freq > base, "max_freq={max_freq}, base={base}");
        assert!(min_freq < base, "min_freq={min_freq}, base={base}");
    }

    #[test]
    fn voice_steal_policy_defaults_to_released_first() {
        let synth = small_synth();
        assert_eq!(synth.voice_steal_policy(), VoiceStealPolicy::ReleasedFirst);
    }

    #[test]
    fn set_voice_steal_policy_round_trips() {
        let mut synth = small_synth();
        synth.set_voice_steal_policy(VoiceStealPolicy::Oldest);
        assert_eq!(synth.voice_steal_policy(), VoiceStealPolicy::Oldest);

        synth.set_voice_steal_policy(VoiceStealPolicy::ReleasedFirst);
        assert_eq!(synth.voice_steal_policy(), VoiceStealPolicy::ReleasedFirst);
    }

    #[test]
    fn set_param_then_param_value_round_trips_defaults_for_all_params() {
        let mut synth = SimpleSynth::new(SimpleSynthConfig::default());
        for id in simple_synth_param_ids() {
            let m = id.metadata();
            synth.set_param(id, m.default);
            assert_eq!(synth.param_value(id), m.default, "param: {}", m.name);
        }
    }

    #[test]
    fn set_param_clamps_continuous_value_below_minimum() {
        let mut synth = SimpleSynth::new(SimpleSynthConfig::default());
        for id in simple_synth_param_ids() {
            let m = id.metadata();
            if m.step_count.is_some() || id == ParamId::MaxPolyphony {
                continue;
            }
            synth.set_param(id, m.min - 1.0);
            assert_eq!(synth.param_value(id), m.min, "param: {}", m.name);
        }
    }

    #[test]
    fn set_param_clamps_continuous_value_above_maximum() {
        let mut synth = SimpleSynth::new(SimpleSynthConfig::default());
        for id in simple_synth_param_ids() {
            let m = id.metadata();
            if m.step_count.is_some() || id == ParamId::MaxPolyphony {
                continue;
            }
            synth.set_param(id, m.max + 1.0);
            assert_eq!(synth.param_value(id), m.max, "param: {}", m.name);
        }
    }

    #[test]
    fn set_param_enum_value_rounds_to_nearest_and_clamps() {
        let mut synth = SimpleSynth::new(SimpleSynthConfig::default());
        for id in simple_synth_param_ids() {
            let m = id.metadata();
            if m.unit != ParamUnit::Enum {
                continue;
            }

            synth.set_param(id, m.max + 5.0);
            assert_eq!(
                synth.param_value(id),
                m.max,
                "param: {} (above max)",
                m.name
            );

            synth.set_param(id, m.min - 5.0);
            assert_eq!(
                synth.param_value(id),
                m.min,
                "param: {} (below min)",
                m.name
            );
        }
    }

    #[test]
    fn set_param_boolean_value_threshold_at_half() {
        let mut synth = SimpleSynth::new(SimpleSynthConfig::default());
        for id in simple_synth_param_ids() {
            let m = id.metadata();
            if m.unit != ParamUnit::Boolean {
                continue;
            }

            synth.set_param(id, 0.49);
            assert_eq!(
                synth.param_value(id),
                0.0,
                "param: {} (0.49 -> false)",
                m.name
            );

            synth.set_param(id, 0.5);
            assert_eq!(
                synth.param_value(id),
                1.0,
                "param: {} (0.5 -> true)",
                m.name
            );
        }
    }

    #[test]
    fn set_param_max_polyphony_is_read_only() {
        let mut synth = small_synth(); // max_polyphony == 4
        assert_eq!(synth.param_value(ParamId::MaxPolyphony), 4.0);

        synth.set_param(ParamId::MaxPolyphony, 64.0);
        assert_eq!(synth.param_value(ParamId::MaxPolyphony), 4.0);
    }

    #[test]
    fn set_param_master_gain_updates_target_gain() {
        let mut synth = small_synth();
        synth.set_param(ParamId::MasterGain, 0.5);
        assert_eq!(synth.param_value(ParamId::MasterGain), 0.5);
    }

    #[test]
    fn set_param_master_gain_zero_eventually_silences_output() {
        let mut synth = small_synth();
        synth.note_on(60, 1.0);
        synth.set_param(ParamId::MasterGain, 0.0);

        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        for _ in 0..50 {
            synth.process(&mut left, &mut right);
        }
        for &s in left.iter().chain(right.iter()) {
            assert!(s.abs() < 1e-3, "sample={s}");
        }
    }

    #[test]
    fn set_param_generator_kind_updates_generator_params() {
        let mut synth = small_synth();
        synth.set_param(ParamId::GeneratorKind, GeneratorKind::Saw.to_param_value());
        assert_eq!(synth.generator_params_mut().kind, GeneratorKind::Saw);
        assert_eq!(
            synth.param_value(ParamId::GeneratorKind),
            GeneratorKind::Saw.to_param_value()
        );
    }

    #[test]
    fn set_param_generator_continuous_params_are_independent() {
        let mut synth = small_synth();
        synth.set_param(ParamId::GeneratorGain, 1.5);
        synth.set_param(ParamId::GeneratorPulseWidth, 0.25);
        synth.set_param(ParamId::GeneratorPhaseOffset, 0.75);
        synth.set_param(ParamId::GeneratorPan, -0.5);

        let g = synth.generator_params_mut();
        assert_eq!(g.gain, 1.5);
        assert_eq!(g.pulse_width, 0.25);
        assert_eq!(g.phase_offset, 0.75);
        assert_eq!(g.pan, -0.5);
    }

    #[test]
    fn set_param_envelope_continuous_params_are_independent() {
        let mut synth = small_synth();
        synth.set_param(ParamId::EnvAttack, 0.05);
        synth.set_param(ParamId::EnvDecay, 0.15);
        synth.set_param(ParamId::EnvSustain, 0.4);
        synth.set_param(ParamId::EnvRelease, 0.3);

        assert_eq!(synth.param_value(ParamId::EnvAttack), 0.05);
        assert_eq!(synth.param_value(ParamId::EnvDecay), 0.15);
        assert_eq!(synth.param_value(ParamId::EnvSustain), 0.4);
        assert_eq!(synth.param_value(ParamId::EnvRelease), 0.3);
    }

    #[test]
    fn set_param_env_curve_updates_curve() {
        let mut synth = small_synth();
        synth.set_param(ParamId::EnvCurve, EnvelopeCurve::Linear.to_param_value());
        assert_eq!(
            synth.param_value(ParamId::EnvCurve),
            EnvelopeCurve::Linear.to_param_value()
        );

        synth.set_param(
            ParamId::EnvCurve,
            EnvelopeCurve::Exponential.to_param_value(),
        );
        assert_eq!(
            synth.param_value(ParamId::EnvCurve),
            EnvelopeCurve::Exponential.to_param_value()
        );
    }

    #[test]
    fn set_param_lfo_booleans_are_independent() {
        let mut synth = small_synth();
        synth.set_param(ParamId::LfoEnabled, 0.0);
        synth.set_param(ParamId::LfoRetrigger, 0.0);
        assert_eq!(synth.param_value(ParamId::LfoEnabled), 0.0);
        assert_eq!(synth.param_value(ParamId::LfoRetrigger), 0.0);

        synth.set_param(ParamId::LfoEnabled, 1.0);
        assert_eq!(synth.param_value(ParamId::LfoEnabled), 1.0);
        assert_eq!(
            synth.param_value(ParamId::LfoRetrigger),
            0.0,
            "retrigger unaffected"
        );
    }

    #[test]
    fn set_param_lfo_enum_params_are_independent() {
        let mut synth = small_synth();
        synth.set_param(ParamId::LfoWaveform, LfoWaveform::Square.to_param_value());
        synth.set_param(
            ParamId::LfoTarget,
            LfoTarget::PitchSemitone.to_param_value(),
        );

        assert_eq!(
            synth.param_value(ParamId::LfoWaveform),
            LfoWaveform::Square.to_param_value()
        );
        assert_eq!(
            synth.param_value(ParamId::LfoTarget),
            LfoTarget::PitchSemitone.to_param_value()
        );
    }

    #[test]
    fn set_param_lfo_continuous_params_are_independent() {
        let mut synth = small_synth();
        synth.set_param(ParamId::LfoRateHz, 2.5);
        synth.set_param(ParamId::LfoAmount, 7.0);

        assert_eq!(synth.param_value(ParamId::LfoRateHz), 2.5);
        assert_eq!(synth.param_value(ParamId::LfoAmount), 7.0);
    }

    #[test]
    fn generator_gain_param_affects_active_voice_smoothly() {
        let mut synth = small_synth();
        synth.note_on(60, 1.0);
        let before = render_mean_abs(&mut synth, 20);

        synth.set_param(ParamId::GeneratorGain, 0.0);
        let after = render_mean_abs(&mut synth, 80);

        assert!(before > 0.1, "before={before}");
        assert!(after < before * 0.25, "before={before}, after={after}");
    }

    #[test]
    fn pulse_width_param_affects_active_pulse_voice() {
        let mut synth = small_synth();
        synth.set_param(
            ParamId::GeneratorKind,
            GeneratorKind::Pulse.to_param_value(),
        );
        synth.note_on(60, 1.0);
        let before = render_mean_signed(&mut synth, 80);

        synth.set_param(ParamId::GeneratorPulseWidth, 0.1);
        let after = render_mean_signed(&mut synth, 100);

        assert!(
            after < before - 0.25,
            "expected narrower pulse to lower signed mean: before={before}, after={after}"
        );
    }

    #[test]
    fn env_sustain_param_affects_active_voice() {
        let mut synth = small_synth();
        synth.set_amp_envelope(EnvelopeParams {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 0.1,
            ..EnvelopeParams::default()
        });
        synth.note_on(60, 1.0);
        let before = render_mean_abs(&mut synth, 30);

        synth.set_param(ParamId::EnvSustain, 0.2);
        let after = render_mean_abs(&mut synth, 100);

        assert!(before > 0.2, "before={before}");
        assert!(after < before * 0.45, "before={before}, after={after}");
    }

    #[test]
    fn lfo_gain_route_amount_and_rate_affect_active_voice() {
        let mut synth = small_synth();
        synth.note_on(60, 1.0);
        render_mean_abs(&mut synth, 40);

        synth.set_param(ParamId::LfoTarget, LfoTarget::Gain.to_param_value());
        synth.set_param(ParamId::LfoAmount, 1.0);
        synth.set_param(ParamId::LfoRateHz, 8.0);
        let rms = block_rms_values(&mut synth, 160);
        let min = rms.iter().copied().fold(f32::MAX, f32::min);
        let max = rms.iter().copied().fold(0.0_f32, f32::max);

        assert!(max > min * 1.8, "min={min}, max={max}");
    }

    #[test]
    fn generator_kind_param_crossfades_active_voice_without_large_step() {
        let mut synth = small_synth();
        synth.note_on(60, 1.0);
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        synth.process(&mut left, &mut right);
        let last_before = left[127];

        synth.set_param(
            ParamId::GeneratorKind,
            GeneratorKind::Triangle.to_param_value(),
        );
        synth.process(&mut left, &mut right);

        let mut samples = Vec::with_capacity(129);
        samples.push(last_before);
        samples.extend_from_slice(&left);
        let max_delta = max_adjacent_delta(&samples);
        assert!(max_delta < 0.25, "max_delta={max_delta}");
    }

    #[test]
    fn set_param_eq_band_enabled_flags_are_independent() {
        let mut synth = small_synth();
        // All three bands start disabled (see `ThreeBandButterworthEq::new`);
        // enable mid/high first so the low-only toggle below actually
        // exercises independence rather than three already-false flags.
        synth.set_param(ParamId::EqMidEnabled, 1.0);
        synth.set_param(ParamId::EqHighEnabled, 1.0);

        synth.set_param(ParamId::EqLowEnabled, 1.0);
        assert!(synth.eq_mut().low.enabled);
        assert!(synth.eq_mut().mid.enabled);
        assert!(synth.eq_mut().high.enabled);

        synth.set_param(ParamId::EqLowEnabled, 0.0);
        assert!(!synth.eq_mut().low.enabled);
        assert!(synth.eq_mut().mid.enabled);
        assert!(synth.eq_mut().high.enabled);

        synth.set_param(ParamId::EqMidEnabled, 0.0);
        synth.set_param(ParamId::EqHighEnabled, 0.0);
        assert!(!synth.eq_mut().mid.enabled);
        assert!(!synth.eq_mut().high.enabled);
    }

    #[test]
    fn set_param_eq_band_frequencies_update_eq_mut() {
        let mut synth = small_synth();
        synth.set_param(ParamId::EqLowFreq, 300.0);
        synth.set_param(ParamId::EqMidFreq, 1500.0);
        synth.set_param(ParamId::EqHighFreq, 8000.0);

        assert_eq!(synth.eq_mut().low.frequency_hz, 300.0);
        assert_eq!(synth.eq_mut().mid.frequency_hz, 1500.0);
        assert_eq!(synth.eq_mut().high.frequency_hz, 8000.0);
    }

    #[test]
    fn set_param_eq_band_types_are_independent() {
        let mut synth = small_synth();
        synth.set_param(
            ParamId::EqLowType,
            ButterworthKind::HighPass.to_param_value(),
        );
        assert_eq!(synth.eq_mut().low.kind, ButterworthKind::HighPass);
        assert_eq!(synth.eq_mut().mid.kind, ButterworthKind::BandPass);
        assert_eq!(synth.eq_mut().high.kind, ButterworthKind::HighPass);
    }

    #[test]
    fn event_kind_param_master_gain_applies_during_process() {
        use z_audio_dsp::TimedEvent;

        let mut synth = small_synth();
        let events = [TimedEvent {
            sample_offset: 0,
            kind: EventKind::Param {
                id: ParamId::MasterGain,
                value: 0.25,
            },
        }];
        let ctx = ProcessContext::new(48_000.0, 128, 120.0, &events);

        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        synth.process_with_context(&ctx, &mut left, &mut right);

        assert_eq!(synth.param_value(ParamId::MasterGain), 0.25);
    }

    #[test]
    fn event_kind_param_eq_low_freq_applies_mid_block() {
        use z_audio_dsp::TimedEvent;

        let mut synth = small_synth();
        let events = [TimedEvent {
            sample_offset: 64,
            kind: EventKind::Param {
                id: ParamId::EqLowFreq,
                value: 500.0,
            },
        }];
        let ctx = ProcessContext::new(48_000.0, 128, 120.0, &events);

        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        synth.process_with_context(&ctx, &mut left, &mut right);

        assert_eq!(synth.eq_mut().low.frequency_hz, 500.0);
    }

    fn simple_synth_param_ids() -> impl Iterator<Item = ParamId> {
        ParamId::ALL.into_iter().filter(|id| {
            matches!(
                id,
                ParamId::MasterGain
                    | ParamId::MaxPolyphony
                    | ParamId::GeneratorKind
                    | ParamId::GeneratorGain
                    | ParamId::GeneratorPulseWidth
                    | ParamId::GeneratorPhaseOffset
                    | ParamId::GeneratorPan
                    | ParamId::EnvAttack
                    | ParamId::EnvDecay
                    | ParamId::EnvSustain
                    | ParamId::EnvRelease
                    | ParamId::EnvCurve
                    | ParamId::LfoEnabled
                    | ParamId::LfoWaveform
                    | ParamId::LfoRateHz
                    | ParamId::LfoAmount
                    | ParamId::LfoTarget
                    | ParamId::LfoRetrigger
                    | ParamId::EqLowEnabled
                    | ParamId::EqLowFreq
                    | ParamId::EqLowType
                    | ParamId::EqMidEnabled
                    | ParamId::EqMidFreq
                    | ParamId::EqMidType
                    | ParamId::EqHighEnabled
                    | ParamId::EqHighFreq
                    | ParamId::EqHighType
                    | ParamId::EqLowGainDb
                    | ParamId::EqLowQ
                    | ParamId::EqMidGainDb
                    | ParamId::EqMidQ
                    | ParamId::EqHighGainDb
                    | ParamId::EqHighQ
            )
        })
    }
}
