# 03. Public API Design

## 第一弾の使い方イメージ

```rust
use z_audio_synth::*;

let mut synth = SimpleSynth::new(SimpleSynthConfig {
    sample_rate: 48_000.0,
    max_block_size: 128,
    max_polyphony: 16,
});

synth.set_generator(GeneratorKind::Saw);
synth.set_amp_envelope(EnvelopeParams {
    attack: 0.01,
    decay: 0.12,
    sustain: 0.7,
    release: 0.25,
});

synth.set_lfo(LfoParams {
    waveform: LfoWaveform::Sine,
    rate_hz: 5.0,
    amount: 0.05,
    target: LfoTarget::PitchSemitone,
});

synth.eq_mut().low.enabled = true;
synth.eq_mut().low.frequency_hz = 180.0;
synth.eq_mut().mid.enabled = true;
synth.eq_mut().mid.frequency_hz = 1200.0;
synth.eq_mut().high.enabled = true;
synth.eq_mut().high.frequency_hz = 6500.0;

synth.note_on(60, 0.8);
synth.process(&mut left, &mut right);
```

## Generator API

```rust
pub enum GeneratorKind {
    Sine,
    Triangle,
    Saw,
    Pulse,
    Noise,
}
```

Pulseだけ追加パラメータを持ちます。

```rust
pub struct GeneratorParams {
    pub kind: GeneratorKind,
    pub gain: f32,
    pub phase_offset: f32,
    pub pulse_width: f32,
}
```

## Envelope API

```rust
pub struct EnvelopeParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub curve: EnvelopeCurve,
}

pub enum EnvelopeCurve {
    Linear,
    Exponential,
}
```

## LFO API

```rust
pub struct LfoParams {
    pub enabled: bool,
    pub waveform: LfoWaveform,
    pub rate_hz: f32,
    pub amount: f32,
    pub target: LfoTarget,
    pub retrigger: bool,
}

pub enum LfoTarget {
    None,
    Gain,
    PitchSemitone,
    EqLowFreq,
    EqMidFreq,
    EqHighFreq,
}

pub enum LfoWaveform {
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    RandomHold,
}
```

## EQ API

```rust
pub struct ThreeBandButterworthEq {
    pub low: ButterworthBand,
    pub mid: ButterworthBand,
    pub high: ButterworthBand,
}

pub struct ButterworthBand {
    pub enabled: bool,
    pub kind: ButterworthKind,
    pub frequency_hz: f32,
    pub q: f32,
}

pub enum ButterworthKind {
    LowPass,
    BandPass,
    HighPass,
}
```

## Parameter Automation API (v0.2)

`ParamId` enumerates every automatable parameter (master gain, generator/envelope/LFO/EQ settings, etc.). `ParamId::ALL` lists every variant, and `ParamId::metadata()` returns its name, unit, valid range, default, and (for `Enum`/`Boolean` parameters) step count — enough for a plugin wrapper to build its parameter list.

```rust
pub enum ParamUnit {
    Linear,
    Hertz,
    Seconds,
    Boolean,
    Enum,
}

pub struct ParamMetadata {
    pub id: ParamId,
    pub name: &'static str,
    pub unit: ParamUnit,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub step_count: Option<u32>,
}
```

`SimpleSynth::set_param(id, value)` / `SimpleSynth::param_value(id) -> f32` apply and read automation values:

- Continuous (`Linear`/`Hertz`/`Seconds`) values are clamped to `metadata().min..=max`.
- `Enum` values are decoded via the relevant `from_param_value` (rounds to nearest, clamps).
- `Boolean` values are `true` when `value >= 0.5`.
- `ParamId::MaxPolyphony` is read-only; `set_param` ignores it.

`EventKind::Param { id, value }` events dispatch to `set_param` at their scheduled sample offset inside `process_with_context`.

## Voice Stealing (v0.2)

```rust
pub enum VoiceStealPolicy {
    Oldest,
    ReleasedFirst, // default
}

synth.set_voice_steal_policy(VoiceStealPolicy::Oldest);
```

When `note_on` is called with no idle voices left, `ReleasedFirst` steals the oldest *releasing* voice (falling back to the oldest active voice if none are releasing); `Oldest` always steals the oldest active voice regardless of release state.

## 将来的な graph API

第一弾では未実装ですが、最終的には以下のような方向にします。

```rust
let patch = Patch::new()
    .generator("osc", Generator::saw())
    .modulator("env", Envelope::adsr())
    .effect("filter", Filter::lowpass())
    .connect("osc.out", "filter.in")
    .modulate("env.out", "osc.gain", 1.0);
```

第一弾は固定チェーン。  
第二弾以降で graph IR に移行します。
