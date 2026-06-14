# 09. Testing and Validation

## Unit tests

### Generators

- sine 440Hz のzero crossing / RMS
- saw の範囲が -1..1
- pulse_width の境界
- noise が finite
- phase wrap

### Envelope

- Attack/Decay/Sustain/Releaseのstate遷移
- release完了でIdle
- 再NoteOn時の段差

### LFO

- 各waveformの出力範囲
- retrigger挙動
- target scaling

### EQ

- LP/BP/HP の係数が finite
- impulse responseが finite
- frequency clamp
- on/off bypass

## Golden audio tests

`tests/golden` に短いwavまたは数値snapshotを置きます。

例:

```text
golden/
├── sine_440_1sec.json
├── saw_c4_env.json
├── noise_bp_sweep.json
└── eq_impulse_lp_1khz.json
```

音声の完全一致は環境差やSIMD差で壊れやすいので、以下のような評価にします。

- RMS誤差
- peak誤差
- spectral centroid誤差
- NaN/Infなし
- sample count一致

## Performance tests

criterionを使います。

```text
bench_generator_sine
bench_generator_saw
bench_simple_synth_16voices
bench_eq_3band
```

目標:

```text
48kHz / block 128 / 16 voices / 3-band EQ
Realtime比 10%以上余裕
```

最初は厳密な数値ではなく、継続的な比較を重視します。

## Manual listening tests

examplesでwavを書き出して確認します。

- sine + envelope
- saw + envelope
- pulse + LFO pitch
- noise + bandpass
- saw + EQ sweep

## Plugin integration tests

Plugin ZIP側の計画に詳細を分離します。  
DSP側では `SimpleSynth` の process API がplugin threadから呼べることを保証します。
