# 00. Project Overview

## プロジェクト名

`z-audio-dsp`

読み方は仮に **ズィー・オーディオ・ディーエスピー**。  
将来的には以下のようなcrate群に分割します。

```text
z-audio-dsp          # DSP core / math / node contracts
z-audio-graph        # node graph / modulation graph / compiler
z-audio-synth        # voice manager / note handling / synth runtime
z-audio-assets       # wavetable/sample preparation
z-audio-plugin       # VST3/CLAP wrapper
z-audio-webclap      # WebCLAP/WASM wrapper
```

第一弾では分割しすぎず、以下の2〜3 crate程度で開始します。

```text
z-audio-dsp          # pure DSP library
z-audio-synth        # simple synth runtime, depends on z-audio-dsp
z-audio-examples     # CLI render examples
```

## ゴール

Phase Plant のように、最終的には次の要素をコードで組めるようにします。

```text
Generators -> Voice-local Effects -> Voice Mix -> Global Effects -> Output
              ^                         ^
              |                         |
          Modulators --------------- Parameters
```

ただし第一弾では、自由グラフではなく **固定構成のシンプルシンセ** を実装します。

```text
MIDI Note
  -> Single Generator
  -> Amp Envelope
  -> 3-band Butterworth EQ
  -> Stereo Output
```

LFOは第一弾では簡単化して、以下のターゲットだけに割り当てます。

- pitch
- gain
- filter/EQ frequency

## 非ゴール

第一弾では以下はやりません。

- GUI
- wavetable editor
- sample player
- arbitrary modular graph
- Env -> Env / LFO -> Env のような再帰的modulation
- audio-rate modulation
- polyphonic effects
- oversampling
- reverb / delay / chorus
- プラグインラッパー実装

プラグインラッパーは別計画に分離します。
