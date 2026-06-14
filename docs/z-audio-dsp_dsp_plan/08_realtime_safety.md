# 08. Realtime Safety

## 基本ルール

audio processing pathでは以下を禁止します。

- heap allocation
- file IO
- network IO
- mutex lock
- blocking wait
- panic
- println/log
- graph rebuild
- asset decode
- Vec再確保

## 許可するもの

- preallocated bufferへの書き込み
- atomic read/write
- lock-free queue
- immutable asset参照
- small fixed-size stack処理

## prepare/process 分離

全DSP objectは以下のライフサイクルを持つ。

```text
new()
  -> prepare(sample_rate, max_block_size)
  -> process()
  -> reset()
```

`process()` 中に必要なbufferは `prepare()` で確保します。

## Parameter update

外部からのパラメータ変更は、第一弾ではシンプルにします。

```text
Plugin/UI thread
  -> atomic parameter value
Audio thread
  -> block beginningで読み取り
  -> smoothing
```

## Buffer reuse

`SimpleSynth` は内部に scratch buffer を持ってよいです。

```rust
pub struct SimpleSynth {
    scratch_mono: Vec<f32>,
    scratch_left: Vec<f32>,
    scratch_right: Vec<f32>,
}
```

ただし `prepare()` で `max_block_size` に合わせて確保します。

## Panic safety

DSP coreでは `unwrap()` を避けます。  
どうしても必要なassertはdebug build限定にします。

```rust
debug_assert!(sample_rate > 0.0);
```

## NaN/Inf対策

各ブロック終端またはテストで以下を確認します。

```rust
assert!(sample.is_finite());
```

リリースビルドでは必要なら denormal対策を入れます。

## Denormal対策

第一弾では簡易的に、極小値を0に寄せます。

```rust
fn flush_denormal(x: f32) -> f32 {
    if x.abs() < 1.0e-20 { 0.0 } else { x }
}
```

将来的にはCPUごとのFTZ/DAZ設定も検討します。
