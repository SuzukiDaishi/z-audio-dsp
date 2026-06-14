# 10. DSP Milestones

## Phase 0: Repository setup

- Rust workspace作成
- crate分割
- CI設定
- formatter/clippy
- examples skeleton

Deliverable:

```text
cargo test
cargo run --example render_sine
```

## Phase 1: Core DSP primitives

- AudioBuffer
- ProcessContext
- parameter smoothing
- midi note to Hz
- biquad utility
- random utility

Deliverable:

```text
z-audio-dsp core primitives
```

## Phase 2: Generators

- Sine
- Triangle
- Saw
- Pulse
- Noise

Deliverable:

```text
render_generators example
```

## Phase 3: Modulators

- ADSR Envelope
- LFO

Deliverable:

```text
render_env_lfo example
```

## Phase 4: Effects

- Gain
- 3-band Butterworth EQ
- smoothing
- band on/off

Deliverable:

```text
render_eq_sweep example
```

## Phase 5: SimpleSynth

- Voice
- VoiceManager
- note on/off
- fixed signal chain
- stereo output

Deliverable:

```text
render_simple_synth example
```

## Phase 6: API stabilization for plugin wrappers

- no allocation during process
- parameter IDs fixed
- metadata export
- sample-accurate event input shape

Deliverable:

```text
z-audio-synth usable from z-audio-plugin
```

## Phase 7: Documentation

- README
- API examples
- parameter table
- architecture diagram
- future roadmap

Deliverable:

```text
DSP crate v0.1 plan complete
```
