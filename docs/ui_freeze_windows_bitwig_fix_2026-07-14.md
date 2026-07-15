# UIフリーズ（Windows / Bitwig）対応メモ

更新日: 2026-07-14

## 症状

- Windows の Bitwig でプラグインGUIを開くと、UIが固まって（フリーズして）操作できなくなる
- 特定操作に限らず、GUIを表示しているだけで発生しうる

## 原因

- `src/gui/mod.rs` の `draw_editor()` が **毎フレーム** `compile_preview()` を呼んでいた
- `compile_preview()` は内部で `run_greedy()`（貪欲 all-pass フィッティング）を実行する
  - 反復回数は `max_sections`（デフォルト **1024**、`MAX_RUNTIME_SECTIONS = 1024`）
  - 各反復で全候補（周波数×Q、数百件）× 128点グリッドを走査
  - 1回あたり概算で **約1億回規模の浮動小数点演算**
- vendor 版 `egui-baseview` のエディタは、メーター描画のため**毎フレーム無条件で `request_repaint()`** を呼ぶ
- 結果として、UIがアイドル（無操作）でも重い greedy フィットが回り続け、GUIスレッドがCPU 100%に張り付いて「固まった」ように見える
- プラットフォーム非依存の問題だが、Windows / Bitwig の環境で顕在化した

## 実装した対策

### プレビュー計算のメモ化（`src/gui/mod.rs`）

- `compile_preview()` の結果を egui コンテキストの一時データ（temp data）にキャッシュする `memoized_preview()` を追加
- キャッシュキーは、曲線に実際に影響する入力のみ:
  - `RuntimeSnapshot`（`PartialEq` で比較）
  - `graph_max_ms`
  - `sample_rate`
- 入力が前フレームと同一なら再計算せずキャッシュを返す
- キャッシュは GUIスレッドからのみ触るため、追加のロックは不要（`ctx.data` / `ctx.data_mut`）

### 効果

- 無操作時は各フレームがキャッシュヒットとなり、GUIスレッドの負荷がほぼゼロになる → フリーズ解消
- ノード操作・プリセット適用・スライダー変更など、実際に入力が変わったフレームでのみ再計算する
- `run_greedy()` / DSP チェイン生成（`compile_runtime_descriptor`）のアルゴリズムや出力は**一切変更していない**ため、音・レイテンシ・フィット結果は従来と同一

## 影響範囲

- 変更は `src/gui/mod.rs` のGUI描画層のみ（形式非依存: VST3 / CLAP / AUv2 / standalone すべてに有効）
- オーディオスレッド、パラメータ、状態保存フォーマットへの変更なし

## 検証

- `cargo check --lib`：成功
- `cargo test --lib`：24 件すべてパス
- `cargo clippy --lib`：本変更による新規警告なし

## 参考

- `src/gui/mod.rs`
  - `PreviewCache` / `memoized_preview()`
  - `draw_editor()` と `refresh!` マクロ内の呼び出しを差し替え
- 計算コストの出所: `src/compiler/greedy.rs`（`run_greedy`, `build_candidates`）, `src/compiler/mod.rs`（`compile_preview`, `PREVIEW_POINTS`）
