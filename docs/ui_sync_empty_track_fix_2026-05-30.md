# UI同期問題（空トラック時）対応メモ

更新日: 2026-05-30

## 症状

- Logic Pro で AUv2 を空トラックに挿した直後、`+ Bell` や一部UI操作が即時反映されない
- 同トラックに音声を配置して再生すると UI が反映される

## 原因

- `ParamSetter` による値変更はホスト往復の非同期反映であり、直後フレームで `Param::value()` 系が更新済みとは限らない
- 空トラックではホスト側の反映契機（処理サイクル）が遅れ、UIが古いスナップショットを描画し続けることがある

## 実装した対策

### 1) 反映待ち再描画（短時間ポーリング）
- ユーザー操作後に数フレーム `request_repaint()` を継続
- 単発クリックでもホスト反映後フレームを拾えるようにする

### 2) 楽観的UIキャッシュ（ノード状態）
- `PresetState.nodes` を UI キャッシュとして利用し、ノード追加/削除/移動/幅変更を即時に反映
- 描画時は `target_snapshot()` をベースに `PresetState.nodes` をオーバーレイして UI 用スナップショットを作成
- ホスト反映を待たずに空トラックでも視覚更新が先行する

### 3) `value()` 依存の縮小
- UIロジックの判定は `unmodulated_plain_value()` / `target_snapshot()` 優先に変更

### 4) 版表示の可視化
- 上部バーに `v{CARGO_PKG_VERSION}` を表示
- 読み込みバイナリの切り分けを容易化

## 影響範囲（AUv2 / VST3 / CLAP）

- 上記対策は `src/gui/*` の共通エディタ層実装であり、形式依存ではない
- したがって AUv2 だけでなく VST3 / CLAP でも同種現象に対して有効
- ただし最終挙動はホスト実装差があるため、主要ホストでの実機確認を推奨

## 現在の実装参照

- `src/gui/mod.rs`
  - `UI_VERSION_TEXT = env!("CARGO_PKG_VERSION")`
  - `ui_snapshot()`（UI描画用スナップショット）
  - `upsert_state_node()` / `set_state_node_enabled()` / `sync_state_nodes_from_target()`
  - 操作後の再描画継続ロジック
- `src/gui/inspector.rs`
  - Remove時の `PresetState.nodes` 同期

## 検証チェックリスト

1. 空トラック挿入直後に `+ Bell` が即表示される
2. 再生前でもノード移動・幅変更の視覚反映がある
3. 再生開始後に表示が不整合にならない
4. 上部のバージョン表示が期待版（Cargo.toml の version）と一致
