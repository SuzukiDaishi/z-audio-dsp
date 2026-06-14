# 11. Final Goal Roadmap

## 解釈

ユーザー文の「さくぃ集目標計画」は、文脈上 **最終目標計画** と解釈します。

## Final Goal

最終的には、Phase Plant をベンチマークにした **コードファースト・モジュラーシンセDSPライブラリ** を目指します。

```text
Generator / Effect / Modulator を自由に組み合わせ、
ゲームランタイム、DAWプラグイン、Web上で同じDSPコアを使う。
```

## Roadmap

### v0.1: Simple Synth Core

- simple generators
- envelope
- LFO
- 3-band Butterworth EQ
- fixed chain
- no GUI

### v0.2: Plugin-ready Synth Core

- parameter metadata
- stable process API
- voice stealing
- automation smoothing
- plugin wrapper integration

### v0.3: Generator expansion

- wavetable generator
- sample player
- PolyBLEP
- unison
- pitch bend
- velocity mapping

### v0.4: Modulation Matrix

- multiple modulators
- multiple targets
- add/multiply/replace modes
- curves/remap
- macros
- limited modulator-to-modulator routing

### v0.5: Graph IR

- nodes
- edges
- mod routes
- graph compiler
- fixed acyclic graph
- preset serialization

### v0.6: Phase Plant-like generator area

- multiple generators
- generator groups
- voice-local effects
- mix/aux/output nodes
- limited audio-rate modulation

### v0.7: Advanced effects

- distortion
- delay
- chorus
- phaser
- compressor
- simple reverb
- oversampling for nonlinear nodes

### v0.8: Wavetable / sample assets

- wavetable asset builder
- mipmaps
- sample player loop/crossfade
- asset serialization

### v0.9: Code-first patch DSL

Example target:

```rust
let patch = Patch::new()
    .voice(|v| {
        let osc = v.generator(Generator::wavetable("wt"))
            .position(v.lfo("motion"));
        let env = v.env("amp");
        osc.gain(env).then(v.filter().lowpass())
    })
    .global(|g, input| {
        input.then(g.eq()).then(g.limiter())
    });
```

### v1.0: Stable DSP library

- stable public API
- plugin wrappers
- WebCLAP build
- examples
- docs
- CI host tests
- benchmark suite

## 最終的な差別化

- Phase Plant風だが、ソースコードで書ける
- ゲームランタイムに組み込みやすい
- GUIなしでも使える
- DSP正確性より音楽的使いやすさ優先
- ただしリアルタイム安全性とCPU管理は最初から仕様化
