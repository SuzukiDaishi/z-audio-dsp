# 01. Rust Workspace Layout

## 第一弾推奨構成

```text
z-audio-dsp/
├── Cargo.toml
├── crates/
│   ├── z-audio-dsp/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── buffer.rs
│   │       ├── context.rs
│   │       ├── params.rs
│   │       ├── math/
│   │       │   ├── mod.rs
│   │       │   ├── smoothing.rs
│   │       │   ├── interpolation.rs
│   │       │   └── biquad.rs
│   │       ├── generators/
│   │       │   ├── mod.rs
│   │       │   ├── analog.rs
│   │       │   └── noise.rs
│   │       ├── modulators/
│   │       │   ├── mod.rs
│   │       │   ├── envelope.rs
│   │       │   └── lfo.rs
│   │       └── effects/
│   │           ├── mod.rs
│   │           ├── gain.rs
│   │           └── butterworth_eq.rs
│   │
│   ├── z-audio-synth/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── voice.rs
│   │       ├── voice_manager.rs
│   │       ├── simple_synth.rs
│   │       └── midi.rs
│   │
│   └── z-audio-examples/
│       ├── Cargo.toml
│       └── examples/
│           ├── render_sine.rs
│           ├── render_noise.rs
│           ├── render_simple_synth.rs
│           └── render_lfo_eq.rs
│
├── tests/
│   ├── golden/
│   └── integration/
│
└── docs/
```

## z-audio-dsp の責務

- sample/block processing
- generators
- modulators
- effects
- math utilities
- no MIDI dependency
- no plugin dependency
- no GUI dependency
- no file IO in processing path

## z-audio-synth の責務

- voice allocation
- note on/off
- MIDI-like event handling
- monophonic/polyphonic simple synth
- fixed MVP signal chain
- `z-audio-dsp` を cargo library として参照

## z-audio-examples の責務

- wav書き出しサンプル
- 各Generator/Effectの確認
- CIでの簡単なsnapshot/golden test補助

## Cargo features

```toml
[features]
default = ["std"]
std = []
simd = []
serde = ["dep:serde"]
alloc-check = []
```

第一弾では `std` 前提でよいです。  
ただしDSPコアは将来的に `no_std + alloc` に寄せられるよう、IOやOS依存は入れない設計にします。
