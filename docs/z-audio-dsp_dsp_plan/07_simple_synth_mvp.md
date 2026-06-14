# 07. Simple Synth MVP

## 目的

GUIなしで、以下ができるシンプルなシンセを作ります。

- MIDI note on/off
- 単一Generator
- Amp Envelope
- LFO
- 3-band Butterworth EQ
- stereo output
- VST/CLAP/WebCLAP wrapperから呼び出しやすい API

## Signal Flow

```text
MIDI/Event Input
  -> VoiceManager
  -> Voice:
       Generator
       Amp Envelope
       LFO
       Gain/Pan
  -> Voice Sum
  -> 3-band Butterworth EQ
  -> Output
```

## SimpleSynth

```rust
pub struct SimpleSynth {
    sample_rate: f32,
    max_block_size: usize,
    voices: VoiceManager,
    generator_params: GeneratorParams,
    amp_env_params: EnvelopeParams,
    lfo_params: LfoParams,
    eq: ThreeBandButterworthEq,
    master_gain: f32,
}
```

## Voice

```rust
pub struct Voice {
    state: VoiceState,
    note: u8,
    velocity: f32,
    generator: GeneratorInstance,
    amp_env: Envelope,
    lfo: Lfo,
    gain: f32,
    pan: f32,
}
```

## GeneratorInstance

```rust
pub enum GeneratorInstance {
    Analog(AnalogGenerator),
    Noise(NoiseGenerator),
}
```

## Voice allocation

```rust
pub enum VoiceStealPolicy {
    Oldest,
    ReleasedFirst,
}
```

第一弾では `ReleasedFirst`、なければ `Oldest`。

## Parameter list

### Synth

- generator_type
- master_gain
- max_polyphony

### Generator

- gain
- pulse_width
- phase_reset
- pan

### Envelope

- attack
- decay
- sustain
- release
- curve

### LFO

- enabled
- waveform
- rate_hz
- amount
- target
- retrigger

### EQ

- low_enabled
- low_freq
- low_type
- mid_enabled
- mid_freq
- mid_type
- high_enabled
- high_freq
- high_type

## Rendering examples

### `render_saw_env_eq.rs`

- saw
- short attack
- medium release
- mid band moved over time

### `render_lfo_pitch.rs`

- sine generator
- LFO pitch vibrato

### `render_noise_eq.rs`

- noise
- bandpass sweep

## MVP Completion Definition

- `cargo test` が通る
- `cargo run --example render_simple_synth` で wav が出る
- 1 note / chord / rapid note sequence が鳴る
- EQ周波数移動ができる
- プラグインラッパーから `process()` を呼べる
- audio thread内でallocationしない設計になっている
