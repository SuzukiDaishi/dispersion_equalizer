以下、ここまでの議論を **そのまま設計書 / 実装計画書の叩き台**として使える粒度でまとめます。
仮称はひとまず **Group Delay EQ / Phase EQ / Delay EQ** とします。

---

# Group Delay EQ VST/CLAP 仕様書・実装計画

## 1. プロダクト概要

### コンセプト

通常のEQは、

```text
横軸: 周波数
縦軸: 音量 dB
```

を編集する。

本プラグインはそれに対して、

```text
横軸: 周波数
縦軸: 遅延時間 ms / group delay
```

を編集する。

つまり、

```text
普通のEQ:
  周波数ごとの音量を変える

本プラグイン:
  周波数ごとの到着時間を変える
```

エフェクト。

基本的には **all-pass filter** を用いて、周波数特性、つまり振幅は極力変えず、位相 / group delay だけを制御する。

---

## 2. 目標

### 主要目標

```text
- Disperser 的な分散エフェクトを含められる
- Pro-Q3 的な直感的ノードUI
- 横軸周波数、縦軸delay ms のグラフ
- Bell / Shelf / Disperser / Scale / Global Delay ノード
- 再生中にノードを動かしても音が途切れない
- wet 100% 時はできる限り振幅フラット
- DAW automation 対応
- CLAP / VST3 対応を視野に入れる
```

### 非目標 / 後回し

```text
- 完全任意の group delay curve を低次数で完璧に再現すること
- Linear phase EQ の代替
- 音階認識やピッチトラッキングによる動的処理
- マルチバンド分割して個別delayする方式
- Phase vocoder 的な周波数領域処理
```

このプラグインはあくまで、

```text
all-pass / pure delay による時間分散エフェクト
```

として設計する。

---

# 3. 技術スタック

## 3.1 プラグインフレームワーク

第一候補:

```text
Rust
nih-plug
nih_plug_egui
```

NIH-plug は Rust 製のオーディオプラグインフレームワークで、VST3 と CLAP の export に対応し、`nih_export_<format>!()` macro によりフォーマットを選べる設計になっている。([GitHub][1])

NIH-plug は `FloatParam`, `IntParam`, `BoolParam`, `EnumParam<T>` などの宣言的パラメータシステムを持ち、パラメータの value distribution、smoother、callback、Serde による非パラメータ状態保存にも対応している。([GitHub][1])

GUIについては、NIH-plug 側に egui / iced / VIZIA の adapter が用意されており、さらに OpenGL / wgpu / softbuffer の custom GUI examples も存在する。([GitHub][1])

## 3.2 推奨構成

```toml
nih_plug
nih_plug_egui
egui
serde
serde_json
atomic_float
rtrb
realfft
rustfft
smallvec
arrayvec
parking_lot  # audio threadでは使わない
```

### GUI

初期実装:

```text
nih_plug_egui + egui::Painter
```

理由:

```text
- ノード操作が作りやすい
- カーブ描画が作りやすい
- プロトタイプ速度が速い
- Rust DSP側との接続が楽
- Pro-Q風のグラフ中心UIを作りやすい
```

将来:

```text
wgpu custom editor
```

移行条件:

```text
- スペアナ描画が重い
- 発光・blur・AA曲線を高品質化したい
- 120fps級のGUIを狙う
- egui Painterの限界が見える
```

## 3.3 出力フォーマット

優先順位:

```text
1. CLAP
2. VST3
3. Standalone debug app
```

NIH-plug には `cargo xtask bundle <package> --release` による bundler があり、プラグインbundleの作成に使える。([GitHub][1])

VST3については、NIH-plug 本体や examples は ISC license だが、`nih_export_vst3!()` が使う VST3 bindings は GPLv3 と説明されているため、配布方針・ライセンス方針を早めに確認する。([GitHub][1])

---

# 4. 用語定義

## Group Delay

```text
周波数ごとに、音の包絡がどれだけ遅れるか
```

表示上は、

```text
delay_ms(f)
```

として扱う。

## Target Curve

ユーザーがUIで描いた理想の遅延カーブ。

```text
target_delay_ms(f)
```

## Actual Curve

実際の all-pass chain / delay によって得られる遅延カーブ。

```text
actual_delay_ms(f)
```

## Pure Delay

全周波数を同じだけ遅らせる処理。

```text
H(z) = z^-N
```

広い意味では all-pass。
全体遅延は biquad all-pass を大量に使わず、delay line で処理する。

## All-pass Section

振幅を変えずに位相だけを変える小さな構成単位。

```text
First-order all-pass
Second-order all-pass / SOS all-pass
```

## Node

GUI上の編集単位。

```text
Bell Delay Node
Shelf Delay Node
Disperser Node
Scale Delay Node
Global Delay
Free Draw Node
```

---

# 5. 機能仕様

## 5.1 ノード一覧

### 5.1.1 Global Delay

全周波数を同じだけ遅らせる。

```text
parameter:
  delay_ms: 0 - 1000 ms
```

内部実装:

```text
DelayLine
```

役割:

```text
- 全体的に大きく遅延
- target curve の共通成分を吸収
- all-pass section の節約
```

---

### 5.1.2 Bell Delay Node

特定周波数周辺だけ delay を持ち上げる。

```text
parameter:
  enabled
  frequency_hz
  amount_ms
  width_oct / q
  mode: normal / sharp / soft
```

UI上の形:

```text
      delay
        ▲
        │      ／＼
        │     ／  ＼
        │____/    \____
        └──────────────▶ frequency
```

内部実装:

```text
2nd-order all-pass SOS
```

最初は、

```text
1 node = 1〜N個の second-order all-pass sections
```

として実装する。

将来的には、`amount_ms` と `width_oct` から pole radius / Q を直接求める。

---

### 5.1.3 Disperser Node

Disperser 的な専用ノード。

```text
parameter:
  enabled
  frequency_hz
  amount
  pinch
  spread
  order / quality
```

内部実装:

```text
center frequency 周辺に複数の 2nd-order all-pass を配置
```

例:

```text
frequency = 1000 Hz
pinch low:
  700, 850, 1000, 1200, 1450 Hz

pinch high:
  930, 970, 1000, 1030, 1080 Hz
```

`amount` は、

```text
- 使用section数
- pole radius
- Q
- section分布
```

に反映する。

Disperser Node は target fitting ではなく、**直接 all-pass stack を生成**する。

---

### 5.1.4 Low Shelf Delay Node

低域側を遅らせる。

```text
parameter:
  cutoff_hz
  amount_ms
  slope / width
```

形:

```text
delay
  ▲
  │██████
  │     ███
  │       ██
  │         █
  └────────────▶ frequency
```

内部実装候補:

```text
- first-order all-pass
- low-frequency-biased all-pass cascade
- shelf-like target + direct primitive
```

---

### 5.1.5 High Shelf Delay Node

高域側を遅らせる。

```text
parameter:
  cutoff_hz
  amount_ms
  slope / width
```

形:

```text
delay
  ▲
  │          █████
  │        ███
  │      ██
  │    █
  └────────────▶ frequency
```

内部実装候補:

```text
- first-order all-pass
- high-frequency-biased all-pass cascade
- shelf-like target + direct primitive
```

---

### 5.1.6 Scale Delay Node

特定スケール上の周波数だけ遅らせる。

例:

```text
A minor pentatonic:
  A, C, D, E, G
```

parameter:

```text
root
scale_type
amount_ms
peak_width_cent
octave_min
octave_max
```

内部実装:

```text
各音階周波数に high-Q 2nd-order all-pass を配置
```

例:

```text
A:
  55, 110, 220, 440, 880, 1760 Hz

C:
  65.4, 130.8, 261.6, 523.2, 1046.5 Hz
```

注意:

```text
曲の音程を解析しているわけではない。
あくまで指定スケールの周波数付近を遅延させる。
```

---

### 5.1.7 Free Draw Node

任意カーブを描くノード。
初期リリースでは後回し。

内部実装:

```text
target group delay
↓
pure delay成分を抽出
↓
残差を all-pass fitting
```

---

# 6. DSP仕様

## 6.1 基本信号フロー

```text
input
 ├─ dry path
 │
 └─ wet path
      ↓
    Global Delay
      ↓
    All-pass Chain
      ↓
    Wet Gain
      ↓
mix
↓
output
```

wet 100% を基本想定。

```text
wet = 100%:
  振幅フラットを保ちやすい

wet < 100%:
  dry/wet の位相差で comb filtering が出る可能性あり
```

これは仕様として明示する。

---

## 6.2 All-pass SOS

2次 all-pass section は、概念的には以下の形。

```text
H(z) = (r² - 2r cos(θ) z⁻¹ + z⁻²)
       / (1 - 2r cos(θ) z⁻¹ + r² z⁻²)
```

または biquad 係数形式で持つ。

```rust
struct SosAllpass {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,

    z1_l: f32,
    z2_l: f32,
    z1_r: f32,
    z2_r: f32,
}
```

process:

```rust
fn process_sample(&mut self, x: f32, ch: usize) -> f32 {
    // transposed direct form II など
}
```

注意点:

```text
- 常に安定条件を満たす
- r < 1
- Q / radius を clamp
- denormal対策
- NaNチェック
```

---

## 6.3 First-order All-pass

Shelf delay 系に使う候補。

```text
H(z) = (a + z⁻¹) / (1 + a z⁻¹)
```

`a` の符号・値により、group delay の偏りが DC / Nyquist 側へ寄る。

```text
low delay shelf:
  DC側にgroup delayを寄せる

high delay shelf:
  Nyquist側にgroup delayを寄せる
```

---

## 6.4 Direct Primitive と Fitting の使い分け

重要方針:

```text
全部を target fitting にしない
```

ノード別に直接構造を生成する。

```text
Global Delay:
  delay line

Bell Delay:
  direct 2nd-order all-pass primitive

Disperser:
  dedicated all-pass stack

Shelf:
  first-order / biased all-pass primitive

Scale:
  note-frequency all-pass bank

Free Draw:
  residual fitting
```

これにより、

```text
- 段数が少ない
- CPUが軽い
- 操作感が安定
- ノードごとの音が予測しやすい
```

---

# 7. パラメータ遷移仕様

このプラグインでは、パラメータ変更時の滑らかさが非常に重要。

## 7.1 変更の分類

```rust
enum ChangeKind {
    Continuous,
    RebuildSoft,
    RebuildHard,
}
```

### Continuous

小さい変更。

```text
- node frequency drag
- amount drag
- width wheel
- wet
- output gain
```

対応:

```text
SmoothedParam
pole parameter smoothing
```

### RebuildSoft

構造が少し変わる変更。

```text
- Disperser amount が大きく変化
- section数が変わる
- quality変更
```

対応:

```text
new chain を作り old/new crossfade
30〜80 ms
```

### RebuildHard

大きな構造変更。

```text
- node type変更
- node追加
- node削除
- scale root変更
- scale mode変更
```

対応:

```text
new chain を作り old/new crossfade
100〜250 ms
```

---

## 7.2 SmoothedParam

```rust
struct SmoothedParam {
    current: f32,
    target: f32,
    coeff: f32,
}

impl SmoothedParam {
    fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    fn next(&mut self) -> f32 {
        self.current += (self.target - self.current) * self.coeff;
        self.current
    }
}
```

係数:

```rust
fn smoothing_coeff(sample_rate: f32, time_ms: f32) -> f32 {
    let time_sec = time_ms / 1000.0;
    1.0 - (-1.0 / (time_sec * sample_rate)).exp()
}
```

初期値:

```text
frequency:     20 ms
amount:        15 ms
width:         30 ms
wet:           10 ms
output gain:   10 ms
global delay:  50 ms
```

---

## 7.3 係数そのものは補間しない

避ける:

```text
b0, b1, b2, a1, a2 を直接線形補間
```

理由:

```text
- all-pass性が崩れる可能性
- 一瞬振幅が変わる可能性
- 不安定化する可能性
```

推奨:

```text
frequency
Q
pole radius
amount
pinch
```

など、意味のあるパラメータを smoothing し、その都度係数を再計算する。

---

## 7.4 Chain Crossfade

構造変更時は old chain と new chain を並列に走らせる。

```rust
struct Engine {
    active_chain: AllpassChain,
    fading_chain: Option<AllpassChain>,
    xfade: Crossfade,
}
```

処理:

```rust
fn process_sample(&mut self, x: f32) -> f32 {
    if let Some(old_chain) = &mut self.fading_chain {
        let old_y = old_chain.process(x);
        let new_y = self.active_chain.process(x);

        let t = self.xfade.next();

        let old_g = (t * std::f32::consts::FRAC_PI_2).cos();
        let new_g = (t * std::f32::consts::FRAC_PI_2).sin();

        let y = old_y * old_g + new_y * new_g;

        if self.xfade.finished() {
            self.fading_chain = None;
        }

        y
    } else {
        self.active_chain.process(x)
    }
}
```

初期値:

```text
soft rebuild:
  64 ms

hard rebuild:
  160 ms

emergency / preset change:
  250 ms
```

---

## 7.5 Global Delay の遷移

Global Delay は all-pass section ではなく delay line。

問題:

```text
delay time をいきなり変えると読み出し位置が飛ぶ
```

対応候補:

### Mode A: Tape

delay time を滑らかに動かす。

```text
長所:
  連続的
  テープ的で気持ちいい

短所:
  ピッチが揺れる
```

### Mode B: Two Read Heads

old delay time と new delay time を同時に読み出して crossfade。

```text
長所:
  ピッチが揺れにくい

短所:
  crossfade中に二重感が出る
```

推奨:

```text
デフォルト:
  Two Read Heads

オプション:
  Tape Mode
```

---

# 8. Fixed Topology 方針

リアルタイム処理では、chain 長が頻繁に変わると不安定になりやすい。

そのため、主要ノードは最大構造を固定する。

```text
Bell Node:
  max 4 SOS

Disperser Node:
  max 32 SOS

Low/High Shelf Node:
  max 8 sections

Scale Node:
  max 128 SOS

Free Draw:
  quality依存
```

有効量を変える場合も、できるだけ

```text
構造は固定
係数を動かす
```

に寄せる。

ただし、完全に不要な section は bypass する。

---

# 9. パラメータ設計

## 9.1 DAW automation のための固定スロット方式

可変 `Vec<Node>` をそのまま使うとDAW automationとの相性が悪い。

そのため、

```text
Max node slots: 16 or 24
```

を固定する。

例:

```rust
struct PluginParams {
    global_delay_ms: FloatParam,
    wet: FloatParam,
    output_gain_db: FloatParam,
    quality: EnumParam<QualityMode>,

    node_01: NodeParams,
    node_02: NodeParams,
    ...
    node_16: NodeParams,
}
```

NodeParams:

```rust
struct NodeParams {
    enabled: BoolParam,
    node_type: EnumParam<NodeType>,

    freq_hz: FloatParam,
    amount_ms: FloatParam,
    width_oct: FloatParam,

    pinch: FloatParam,
    spread: FloatParam,
    order: IntParam,

    scale_root: EnumParam<RootNote>,
    scale_mode: EnumParam<ScaleMode>,
}
```

GUI上では inactive node を表示しない。

---

## 9.2 パラメータ範囲

```text
global_delay_ms:
  0 - 1000 ms

node amount_ms:
  0 - 1000 ms

frequency_hz:
  20 - 20000 Hz
  log scale

width_oct:
  0.03 - 6.0 oct

pinch:
  0 - 1

spread:
  0 - 4 oct

wet:
  0 - 100 %

output_gain:
  -24 dB - +24 dB

quality:
  Eco / Normal / High / Insane
```

---

# 10. GUI仕様

## 10.1 方向性

Pro-Q3 的な操作性を参考にしつつ、見た目はオリジナルにする。

```text
- ダークテーマ
- グラフ中心
- ノード操作
- 浮遊インスペクタ
- スペアナ背景
- 高級感のある発光カーブ
```

---

## 10.2 メイン画面

```text
中央:
  group delay graph

横軸:
  frequency, log scale

縦軸:
  delay ms

表示:
  target curve
  actual curve
  node influence fill
  spectrum analyzer
  global delay line
  selected node handles
```

---

## 10.3 操作

```text
double click:
  node追加

drag node:
  frequency / amount 変更

wheel:
  width変更

shift drag:
  fine adjust

alt drag:
  amount固定 or frequency固定

right click:
  node menu

delete:
  selected node削除

ctrl/cmd drag:
  duplicate
```

---

## 10.4 Floating Inspector

選択ノードの近くに表示。

```text
Bell:
  Freq
  Amount
  Width

Disperser:
  Freq
  Amount
  Pinch
  Spread
  Order

Scale:
  Root
  Scale
  Amount
  Width

Shelf:
  Cutoff
  Amount
  Slope
```

---

## 10.5 下部バー

```text
Global Delay
Wet
Output
Quality
Analyzer On/Off
A/B
Undo
Redo
Preset
```

---

# 11. Analyzer仕様

## 11.1 基本

Pro-Q風UIではスペクトラム表示が重要。

ただし audio thread でFFTしない。

```text
audio thread:
  入力/出力サンプルを ring buffer に push

GUI/background:
  ring bufferから読む
  FFT
  smoothing
  描画
```

候補:

```text
realfft
rustfft
rtrb
```

---

## 11.2 表示

```text
Input spectrum
Output spectrum
差分表示 optional
Peak hold optional
```

Analyzer は target/actual curve より奥に薄く表示する。

---

# 12. スレッド設計

## 12.1 構成

```text
GUI thread:
  ノード編集
  パラメータUI
  描画

Compiler / background:
  NodeModel → ChainDescriptor
  fitting / section生成

Audio thread:
  DSP
  smoothing
  chain swap
  crossfade

Analyzer thread / GUI:
  FFT
  spectrum smoothing
```

NIH-plug には realtime-safe な background tasks の仕組みがあると説明されているため、重い compile / fitting は audio thread 外へ逃がす方針にする。([GitHub][1])

---

## 12.2 Audio Threadで禁止

```text
- Vec allocation
- mutex lock
- file IO
- println / heavy logging
- FFT
- fitting
- JSON parse
- dynamic memory resize
```

---

# 13. データ構造案

## 13.1 Node Model

```rust
#[derive(Clone, Serialize, Deserialize)]
struct NodeModel {
    id: u32,
    enabled: bool,
    node_type: NodeType,

    freq_hz: f32,
    amount_ms: f32,
    width_oct: f32,

    pinch: f32,
    spread_oct: f32,
    order: u32,

    scale_root: RootNote,
    scale_mode: ScaleMode,
}
```

## 13.2 Node Type

```rust
#[derive(Clone, Copy, Serialize, Deserialize)]
enum NodeType {
    Bell,
    Disperser,
    LowShelf,
    HighShelf,
    Scale,
    FreeDraw,
}
```

## 13.3 Chain Descriptor

```rust
#[derive(Clone)]
struct ChainDescriptor {
    global_delay_ms: f32,
    sections: Vec<SectionDescriptor>,
    xfade_ms: f32,
    change_kind: ChangeKind,
}
```

## 13.4 Section Descriptor

```rust
#[derive(Clone, Copy)]
enum SectionDescriptor {
    FirstOrder {
        a: f32,
    },
    SecondOrder {
        freq_hz: f32,
        q: f32,
        radius: f32,
    },
}
```

## 13.5 Runtime Chain

```rust
struct AllpassChain {
    sections: Vec<RuntimeSection>,
}

enum RuntimeSection {
    FirstOrder(FirstOrderAllpass),
    SecondOrder(SosAllpass),
}
```

---

# 14. Node Compiler

## 14.1 入力

```text
NodeModel[]
SampleRate
QualityMode
```

## 14.2 出力

```text
ChainDescriptor
ActualGroupDelayPreview
```

---

## 14.3 Compile Flow

```text
1. enabled node を集める
2. Global Delay を抽出
3. 各 node を section list に変換
4. section 数を quality に応じて制限
5. actual group delay curve を計算
6. GUIへ preview を返す
7. audio thread へ ChainDescriptor を送る
```

---

# 15. Disperser Node 詳細

## 15.1 パラメータ

```text
frequency_hz:
  center

amount:
  0 - 1

pinch:
  0 - 1

spread:
  0 - 4 oct

order:
  1 - 64
```

## 15.2 Section生成

```rust
fn compile_disperser(node: &NodeModel, sr: f32, quality: QualityMode) -> Vec<SectionDescriptor> {
    let n = compute_section_count(node.amount_ms, node.order, quality);
    let spread = node.spread_oct;
    let pinch = node.pinch;

    for i in 0..n {
        let u = normalized_index(i, n); // -1..1

        let shaped = sign(u) * abs(u).powf(lerp(1.0, 4.0, pinch));
        let freq = node.freq_hz * 2.0_f32.powf(shaped * spread * 0.5);

        let q = compute_q(node, i, n);
        let radius = compute_radius(node, i, n);

        sections.push(SecondOrder { freq_hz: freq, q, radius });
    }
}
```

考え方:

```text
pinch低:
  周波数を広く分布

pinch高:
  center frequency に密集

amount高:
  section数 / radius / Q を増やす
```

---

# 16. Scale Node 詳細

## 16.1 音階

```rust
enum ScaleMode {
    MajorPentatonic,
    MinorPentatonic,
    Major,
    Minor,
    Chromatic,
}
```

## 16.2 周波数生成

```rust
fn scale_frequencies(root: RootNote, mode: ScaleMode, min_hz: f32, max_hz: f32) -> Vec<f32> {
    // MIDI noteを列挙
    // scale intervalに一致するものだけ
    // frequencyへ変換
}
```

## 16.3 Section生成

```text
各scale frequencyに high-Q all-pass bell
```

注意:

```text
低域はピークが密集しやすい
高域は数が増えやすい
```

制御:

```text
octave range
max peaks
width cents
quality mode
```

---

# 17. Quality Mode

```text
Eco:
  Bell max 1-2 sections
  Disperser max 8 sections
  Scale max 32 sections

Normal:
  Bell max 4 sections
  Disperser max 24 sections
  Scale max 96 sections

High:
  Bell max 8 sections
  Disperser max 48 sections
  Scale max 160 sections

Insane:
  制限高め
  sound design用
```

---

# 18. テスト計画

## 18.1 DSP Unit Tests

```text
- all-pass magnitude がほぼ1である
- SOSがNaNを出さない
- radius < 1 で安定
- impulse response が発散しない
- denormalが出ない
- sample rate変更で破綻しない
```

## 18.2 Group Delay Tests

```text
- Bell Node の peak が指定周波数近くに出る
- Disperser Node の pinch が効く
- Scale Node が指定scale frequencyにpeakを作る
- Global Delay が全帯域一定delayになる
```

## 18.3 Transition Tests

```text
- frequency drag stress
- amount automation stress
- node add/remove
- scale root change
- preset change
- wet automation
- global delay automation
```

評価:

```text
- click / pop がない
- NaNがない
- peakが異常に跳ねない
- CPU spikeが許容範囲
```

## 18.4 DAW Tests

```text
REAPER:
  CLAP
  VST3

Bitwig:
  CLAP

Studio One:
  CLAP/VST3

Ableton Live:
  VST3

FL Studio:
  VST3/CLAP
```

---

# 19. 実装フェーズ

## Phase 0: Repository Setup

```text
- nih-plug template
- CLAP export
- basic stereo effect
- bypass
- wet
- output gain
- simple egui window
```

成果物:

```text
音が通るだけのプラグイン
```

---

## Phase 1: Core DSP

```text
- DelayLine
- FirstOrderAllpass
- SosAllpass
- AllpassChain
- Chain crossfade
- SmoothedParam
```

成果物:

```text
Global Delay
単一Bell Delay
単一Disperserもどき
```

---

## Phase 2: Node Compiler

```text
- NodeModel
- NodeType
- Bell compiler
- Disperser compiler
- Shelf compiler
- Scale compiler
- ChainDescriptor
```

成果物:

```text
GUIなしでもノード設定からDSP chainを生成できる
```

---

## Phase 3: GUI v1

```text
- Pro-Q風 graph area
- frequency log grid
- delay ms grid
- node bubble
- drag操作
- floating inspector
```

成果物:

```text
マウスでノードを動かせる
```

---

## Phase 4: Smooth Transition

```text
- Continuous/RebuildSoft/RebuildHard分類
- parameter smoothing
- pole parameter smoothing
- chain crossfade
- global delay two read heads
```

成果物:

```text
再生中に動かしても途切れにくい
```

---

## Phase 5: Analyzer

```text
- ring buffer
- FFT
- smoothing
- spectrum draw
- input/output optional
```

成果物:

```text
Pro-Q系っぽい見た目の完成度が上がる
```

---

## Phase 6: Preset / Undo / A-B

```text
- preset serialize
- A/B state
- undo stack
- node copy/paste
```

---

## Phase 7: Optimization

```text
- section数最適化
- SIMD検討
- denormal対策
- allocation削減
- GUI描画最適化
```

---

## Phase 8: Packaging

```text
- cargo xtask bundle
- CLAP bundle
- VST3方針確認
- installer検討
- DAW別検証
```

---

# 20. 推奨ファイル構成

```text
src/
  lib.rs
  params.rs
  editor.rs

  dsp/
    mod.rs
    engine.rs
    delay_line.rs
    allpass.rs
    chain.rs
    smooth.rs
    crossfade.rs
    analyzer.rs

  model/
    mod.rs
    node.rs
    scale.rs
    preset.rs

  compiler/
    mod.rs
    bell.rs
    disperser.rs
    shelf.rs
    scale.rs
    free_draw.rs
    descriptor.rs

  gui/
    mod.rs
    graph.rs
    node_view.rs
    inspector.rs
    theme.rs
    analyzer_view.rs
```

---

# 21. MVP仕様

最初の完成形はここまでで十分です。

```text
Format:
  CLAP

DSP:
  Global Delay
  Bell Delay
  Disperser Node

GUI:
  Pro-Q風 graph
  node drag
  floating inspector
  target/actual表示

Transition:
  SmoothedParam
  Chain crossfade
  Global Delay two read heads

State:
  fixed 16 node slots
  preset保存
```

後回し:

```text
Scale Node
Shelf Node
Analyzer
Free Draw
VST3配布
```

---

# 22. 最重要設計判断

このプラグインの核はここです。

```text
1. 縦軸は phase ではなく group delay ms

2. Global Delay は pure delay で処理

3. Disperser は専用 all-pass stack

4. Bell/Shelf/Scale はノード別 primitive

5. Free Draw だけ fitting

6. パラメータ変更は smoothing + crossfade

7. audio threadで重いcompileをしない

8. DAW automationのために node slot は固定

9. GUIは graph中心、Pro-Q的操作感、見た目は独自

10. wet 100% を基本設計にする
```

この設計なら、
**Disperser的な音作り** と **汎用的なGroup Delay EQ** の両方を狙えます。

[1]: https://github.com/robbert-vdh/nih-plug "GitHub - robbert-vdh/nih-plug: Rust VST3 and CLAP plugin framework and plugins - because everything is better when you do it yourself · GitHub"
