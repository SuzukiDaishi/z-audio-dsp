# z-audio-dsp DSP 実装計画

作成日: 2026-06-14

このZIPは、`z-audio-dsp` の **DSPコア / Rust cargo library** としての実装計画です。  
VST3 / CLAP / WebCLAP などのプラグイン形式は別ZIPの計画に分離します。

## 命名方針

公開APIでは **Oscillator よりも Generator を採用**します。

理由:

- Phase Plant 準拠の概念に近い
- Generator は oscillator だけでなく、noise / sample / audio input / math / future granular / wavetable / additive まで含められる
- 「音を生成するもの」と「音を加工するもの」と「パラメータを動かすもの」を整理しやすい

ただし実装内部では、`generators::analog`, `generators::wavetable`, `generators::noise` のように分けます。  
`Sine`, `Triangle`, `Saw`, `Pulse` は **Analog Generator の waveform variant** として扱います。

## 第一弾 MVP

第一弾では GUI なしのシンプルなシンセを作れる DSP コアを実装します。

### Generators

- Sine
- Triangle
- Saw
- Pulse
- Noise

### Modulators

- Envelope
- LFO

### Effects

- Gain
- 3-band Butterworth EQ
  - Low slot: Low-pass
  - Mid slot: Band-pass
  - High slot: High-pass
  - それぞれ on/off
  - 周波数を移動可能

ユーザー指定では `lowpass, bandpass, lowpass` となっていましたが、EQとしては `lowpass, bandpass, highpass` が自然です。  
そのため仕様上は **各bandが type を持つ ButterworthBand** として実装し、初期プリセットを LP / BP / HP にします。

## このZIP内のファイル

- `00_project_overview.md`
- `01_workspace_layout.md`
- `02_core_architecture.md`
- `03_public_api.md`
- `04_generators_plan.md`
- `05_modulators_plan.md`
- `06_effects_eq_plan.md`
- `07_simple_synth_mvp.md`
- `08_realtime_safety.md`
- `09_testing_and_validation.md`
- `10_milestones.md`
- `11_final_goal_roadmap.md`
- `REFERENCES.md`
