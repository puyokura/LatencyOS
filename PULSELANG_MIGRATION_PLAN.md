# PulseLang Migration (.pul) & Test Suite Remediation Plan

## Context
ユーザーからの指示である「PulseLangの拡張子移行（`.pul`はPerlと重複するため`.pul`へ移行）」および「PulseEditorで行番号サイドバーがスクロールに追従しない問題の修正」について、現在のコードベースにおける完了状況および残存課題を整理し、テストスイートの完全通過とドキュメント整合性を完了させる。

現状、PulseEditorのスクロール修正（`test-editor-scroll` PASS）およびスタンドアロンテスト（`test-standalone` 45項目全PASS）は完了しているが、`cargo xtask test-px64` 内の「Test 7（5.0ms ハードウォッチドッグタイムアウト検証）」において、テスト用スクリプトが 5.0ms 経過前に 10,000 ステップ上限（MAX_VM_STEPS）に達して `ERR_PX64_WCET_EXCEEDED` になる不整合が存在する。このテスト生成コードを修正して全テストを PASS させ、ドキュメント類の `.pul` 表記を `.pul` へ統一する。

## Approach

### 1. `xtask/src/main.rs` の `test_px64_architecture` Test 7 修正
- **目的**: 5.0ms ハードウォッチドッグタイムアウト（`ERR_PX64_TIMEOUT_EXCEEDED`）の確実な検証。
- **変更内容**:
  `xtask/src/main.rs` の Test 7 において、生成するスクリプト `/loop_cap.pul` を、10,000 ステップ未満で確実に 5.0ms (5,000,000 ns) を超過する処理に変更する。
  具体的には、`@busy_wait(10000000)`（10.0ms スピンウェイト）を含むスクリプトを記述して実行させ、`ERR_PX64_TIMEOUT_EXCEEDED` が返ることを検証する。
  ```pulse
  @contract: @wcet(10ms) @budget(20ms);
  @busy_wait(10000000);
  ```
- **確認**: `cargo xtask test-px64` が最後まで完走し、PASS すること。

### 2. ドキュメント類の `.pul` 表記を `.pul` へ統一
- **対象ファイル**:
  - `README.md`
  - `architecture.md`
  - `docs/pulselang.md`, `docs/pulselang_ai_spec.md`
  - `docs/ja/README.md`, `docs/ja/architecture.md`, `docs/ja/pulselang_ai_spec.md`, `docs/ja/scripts_cookbook.md`, `docs/ja/shell_and_editor.md`
  - `docs/lang/PULSELANG_COMPLETE_AI_REFERENCE.md`, `docs/lang/README.md`, `docs/lang/ai_spec.md`, `docs/lang/cookbook.md`, `docs/lang/spec.md`
  - `docs/lang/ja/PULSELANG_COMPLETE_AI_REFERENCE.md`, `docs/lang/ja/ai_spec.md`, `docs/lang/ja/cookbook.md`, `docs/lang/ja/spec.md`
- **変更内容**:
  スクリプト名表記（`stream.pul` -> `stream.pul`, `bench.pul` -> `bench.pul`, `filter.pul` -> `filter.pul`, `jitter.pul` -> `jitter.pul`, `telemetry.pul` -> `telemetry.pul`, `echo.pul` -> `echo.pul` など）および拡張子説明を `.pul` に更新する。

### 3. 全テストスイートの総合実行と確認
- 全ての `xtask` テストを順番に実行し、すべて PASS することを確認する。

## Critical files & anchors
- `xtask/src/main.rs` (lines 1227-1241): `test_px64_architecture` 内の Test 7 スクリプト生成箇所
- `kernel/src/lang.rs` (lines 3803-3821, 4360-4369): `PX64VM::run` におけるタイムアウト判定と `NATIVE_BUSY_WAIT` 実装
- `kernel/src/editor.rs` (lines 90-95, 520-560): PulseEditor のスクロール描画実装（修正済み）
- `docs/` ディレクトリ配下: PulseLang ドキュメント群

## Verification
以下のコマンドを全て実行し、全て exit code 0 かつ PASS 出力となることを確認する:
1. `cargo check --package kernel --target x86_64-unknown-none`
2. `cargo check --package xtask`
3. `cargo xtask test-boot`
4. `cargo xtask test-editor-scroll`
5. `cargo xtask test-editor-delete`
6. `cargo xtask test-paste`
7. `cargo xtask test-compile-error`
8. `cargo xtask test-px64`
9. `cargo xtask test-standalone`

## Assumptions & contingencies
- **QEMU 上の TSC 挙動**: `@busy_wait` は CPU の TSC レジスタをポーリングして待機するため、QEMU 環境下でも確実にハードウェアタイムアウト（5.0ms）に到達する。もし仮に TSC 周波数が低く見積もられてステップ判定に引っかかる場合は、`for` ループ内で数回 `@busy_wait(2000000)` を呼ぶ構成とする。
