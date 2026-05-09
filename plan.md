# Dispersion Equalizer — アーキテクチャ概要 (v2)

> このドキュメントは実装完了後の現状アーキテクチャを記録する。  
> 旧ロードマップ (Bell/Disperser ヒューリスティック方式) は廃止。

---

## 1. 目的

Dispersion Equalizer は周波数ごとの group delay を編集するオーディオプラグイン (CLAP / VST3)。
通常の EQ と異なり、振幅特性をフラットに保ちながら位相のみを操作する。

```
通常の EQ:  x=frequency  y=gain dB
本プラグイン: x=frequency  y=group delay ms
```

`docs/preview.html` (v6) がリファレンス実装。DSP・ノード種別・パラメータ体系はすべてこれに準拠する。

---

## 2. ノード種別

| 種別 | 形状関数 | 用途 |
|------|---------|------|
| **Bell** | log-freq 上の Gaussian | 特定帯域の遅延を持ち上げる |
| **LowShelf** | logistic sigmoid (低域側) | 低域全体の遅延を持ち上げる |
| **HighShelf** | logistic sigmoid (高域側) | 高域全体の遅延を持ち上げる |
| **Scale** | ペンタトニック音程上の Gaussian 最大値 | スケール音程に遅延ピークを置く |

---

## 3. DSP アルゴリズム — Greedy Fitting

### 3.1 概要

`compile_runtime_descriptor()` が呼ばれるたびに `run_greedy()` を実行する。

1. **128 点ログ周波数グリッド** (20 Hz〜min(20 kHz, sr×0.45)) を生成する。
2. 各グリッド点でノード形状の総和 + `global_delay_ms` を **target** として計算する。
3. target の最小値を **pure delay** (遅延ライン) として分離し、残差を allpass で補う。
4. 候補 (freq, Q) セットを事前計算し、それぞれのグループ遅延カーブを持つ。
5. **Greedy ループ** — `max_sections` 回まで繰り返す:
   - 各候補について `improvement = 2 × dot(residual, curve) − energy` を計算する。
   - improvement > 0 の候補のうち最大のものを 1 section として採用する。
   - 採用できる候補がなければ終了。
6. 採用回数 × SectionDescriptor を unroll してチェーンへ渡す。

### 3.2 候補生成

- **36 点ロググリッド** (20 Hz〜min(20 kHz, sr×0.45))
- 各ノードの中心周波数 / ±widthOct×0.5
- Scale ノードの全スケール周波数 (preferredQ 付き)
- 各中心に対して `BASE_QS = [0.35, 0.7, 1.4, 3.0, 7.0, 16.0, 42.0]` の Q を試す

### 3.3 RBJ Allpass 係数

```
w0    = 2π × freq / sample_rate
α     = sin(w0) / (2Q)
a0_inv = 1 / (1 + α)
b0    = (1 − α) × a0_inv
b1    = (−2 cos w0) × a0_inv
b2    = (1 + α) × a0_inv
a1    = b1,  a2 = b0   ← allpass 恒等式
```

`docs/preview.html` の `biquadAllpassCoeffs()` と同一。

### 3.4 グループ遅延 (有限差分)

```
df    = clamp(freq × 0.0015, 0.25, 40) Hz
φ_a   = rbj_allpass_phase(freq − df)
φ_b   = rbj_allpass_phase(freq + df)
τ(f)  = −Δφ / Δω  [サンプル]  × 1000 / sr  [ms]
```

---

## 4. パラメータ一覧

### グローバル

| ID | 名前 | 範囲 | デフォルト |
|----|------|------|-----------|
| `gdel` | Global Delay | 0–1000 ms | 0 |
| `wet` | Wet | 0–100 % | 100 |
| `out` | Output Gain | −24–+24 dB | 0 |
| `msos` | Max SOS | 8–4096 | 1024 |

### ノード (スロット 1–16)

| ID | 名前 | 範囲 | デフォルト |
|----|------|------|-----------|
| `en` | Enabled | bool | false |
| `type` | Node Type | Bell/LowShelf/HighShelf/Scale | Bell |
| `freq` | Frequency | 20–20000 Hz (log) | 1000 |
| `amt` | Amount | 0–1000 ms | 250 |
| `width` | Width | 0.01–6.0 oct | 1.0 |
| `root` | Scale Root | C–B | A |
| `scale` | Scale Mode | MajorPentatonic/MinorPentatonic | MinorPentatonic |

> 旧パラメータ `quality`, `pinch`, `spread_oct`, `order` は廃止 (破壊的変更)。

---

## 5. ソースツリー

```
src/
  lib.rs                  — NIH-plug エントリポイント、Engine 呼び出し
  params.rs               — PluginParams / NodeParams 定義
  editor.rs               — egui エディタ登録

  dsp/
    mod.rs
    engine.rs             — active/fading chain、wet/gain smoother
    delay_line.rs         — pure delay (最大 1000 ms、線形補間)
    allpass.rs            — SosAllpass (RBJ 式)
    chain.rs              — RuntimeChain (MAX_RUNTIME_SECTIONS=1024)
    crossfade.rs          — equal-power クロスフェード

  model/
    mod.rs
    node.rs               — NodeType, NodeRuntimeParams, RuntimeSnapshot
    scale.rs              — RootNote, ScaleMode
    preset.rs             — PresetState, NodeModel

  compiler/
    mod.rs                — compile_runtime_descriptor / compile_preview
    descriptor.rs         — SectionDescriptor { SecondOrder{freq_hz,q}, Bypass }
    greedy.rs             — run_greedy(), 形状関数, スケール周波数

  gui/
    mod.rs                — メインレイアウト、プリセット、ボトムバー
    graph.rs              — グラフ描画、ノードインタラクション
    node_view.rs          — ノードラベル・色定義
    inspector.rs          — 選択ノード Inspector パネル
    theme.rs              — カラー定数
```

---

## 6. 組み込みプリセット

| 名前 | 内容 |
|------|------|
| Flat | 全スロット無効 |
| Big Global | Global Delay 500 ms |
| Disperser | Bell 2 ノード (400 Hz / 4 kHz、各 400 ms) |
| Vocal Air | HighShelf 8 kHz 300 ms |
| A Minor Penta | Scale ノード A マイナーペンタトニック |
| Bass Push | LowShelf 200 Hz 200 ms |

---

## 7. ビルド & テスト

```powershell
# コンパイル確認
cargo check

# 単体テスト (11 tests)
cargo test

# リリースビルド
cargo xtask bundle dispersion_equalizer --release
# → target/bundled/Dispersion Equalizer.clap
# → target/bundled/Dispersion Equalizer.vst3

# Python 統合テスト (pedalboard 必要)
python -m pytest tests/ -v
```

### 通過済みテスト

```
dsp::allpass::tests::sos_magnitude_is_flat
dsp::allpass::tests::sos_impulse_does_not_explode
dsp::delay_line::tests::zero_delay_returns_current_sample
dsp::delay_line::tests::fixed_delay_returns_later
compiler::greedy::tests::bell_shape_peaks_at_center
compiler::greedy::tests::low_shelf_is_monotone_decreasing
compiler::greedy::tests::high_shelf_is_monotone_increasing
compiler::greedy::tests::scale_frequencies_a_minor_pentatonic_includes_440
compiler::greedy::tests::greedy_bell_peak_near_center
compiler::greedy::tests::greedy_respects_max_sections
gui::graph::tests::freq_mapping_round_trips
```

---

## 8. 設計メモ

- **MAX_RUNTIME_SECTIONS = 1024** — スタックオーバーフロー防止のため `Box<[...]>` でヒープ確保。
- **DAW オートメーション後方互換性は破壊** — `pinch`/`spread_oct`/`order`/`quality` の ID が消えるため旧プロジェクトのオートメーションは無効化される。これは意図的な破壊的変更。
- **audio thread** では allocation、mutex lock、JSON parse、compile を行わない。chain 更新は triple-buffer swap (Arc<Mutex<...>>) で渡す。
- **wet < 100%** では dry/wet 位相差によるコムフィルタリングが生じる。これは仕様。
- **Greedy fitting** は compile 時のみ実行 (パラメータ変化時)。1024 sections でも ~30 ms 程度のため GUI スレッドで許容範囲。

---

## 9. 手動 DAW テストチェックリスト

```
□ Bell ノードをドラッグ → Target/Actual 両カーブが動く
□ LowShelf ノード追加 → 低域にのみ遅延ピーク
□ HighShelf ノード追加 → 高域にのみ遅延ピーク
□ Scale ノード追加 → ペンタトニック音程にピーク & ガイドライン表示
□ Max SOS を 8 → Actual カーブが粗くなる
□ Max SOS を 1024 → 精密な Actual カーブ
□ wet 100% で大きな音量変化なし
□ 再生中ノード操作でドロップアウトなし
□ preset 保存・再読み込みで状態復元
□ REAPER CLAP ロード・オートメーション動作
```
