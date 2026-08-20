# Design Spec: LatencyOS Pulse Shell (Hard Real-Time & Time-Native Shell)

## 1. Overview & Vision
`Pulse Shell` は、Linux/POSIX シェルの模倣から脱却し、**「すべての操作がハードウェア時間と直結する」** という LatencyOS 独自の哲学を具現化したハードリアルタイム対話型シェルである。

Linux のような重厚なバナーや無駄な装飾を排しつつ、**「全コマンドのナノ秒計測」「ゼロアロケーション検証」「`within` デッドライン実行ガード」「SPSC リングバッファ & コア計装」** を第一級機能として提供する。

---

## 2. Core Features & Syntax

### 2.1 Time-Native Prompt & Execution Telemetry
- **プロンプト**:
  `[c0 | 24ns] latencyos> `
  - 現在の実行コア番号（`c0`）と、直前に実行されたコマンドの精密なハードウェア計測時間（`24ns`, `1.2us` 等）を表示。
- **全コマンド自動計装**:
  - コマンド実行の前後でシリアライズされた TSC（`rdtscp` / `lfence`）を読み取り、実行時間・メモリ確保量（常時 0B）を確定。

### 2.2 Hard Real-Time Deadline Guard (`within <time> <cmd>`)
- 構文: `within <time_literal> <command>`
  - 例: `within 500us run filter.flow`
  - 例: `within 2ms benchmark`
- 動作:
  - 指定された時間予算（`50ns`, `500us`, `5ms`, `1s`）をナノ秒換算。
  - コマンド実行時間が予算内であれば `[within 500us: PASSED (actual: 312us)]` を報告。
  - 超過した場合は `[within 500us: DEADLINE VIOLATED (actual: 620us, delta: +120us)]` を報告。

### 2.3 Hard Real-Time Telemetry Commands
1. **`timeline` / `trace`**:
   - コア間パイプライン（ISR $\to$ ユーザー空間 $\to$ VBlank $\to$ キャプチャ $\to$ エンコード $\to$ ネットワーク送信）のステージ別レイテンシをマイクロ秒精度で視覚化。
2. **`ring`**:
   - コア間 Lock-free SPSC リングバッファ（`CAPTURE_TO_ENCODE_RING` 等）の占有率、Head/Tail ポインタ、キャッシュライン状態を表示。
3. **`cores`**:
   - APIC ID、アサインされた役割、C0ステート固定状態、スピンループ回数を表示。
4. **`tsc`**:
   - 直近のシリアライズド TSC、計測周波数（MHz）、1サイクルあたりのナノ秒を表示。
5. **`ls -t`**:
   - ファイルサイズに加え、PulseLang スクリプトの推定 WCET（最悪実行時間）を表示。

---

## 3. Architecture & Zero-Allocation Guarantee

- **メモリ**:
  - 動的ヒープ（`alloc` / `malloc`）は一切使用しない。
  - 静的固定バッファ（`[u8; 128]` コマンドライン、ヒストリバッファ）のみ。
- **最悪実行時間 (WCET)**:
  - シェルディスパッチ: $\le 100\text{ ns}$
  - `within` ガードオーバーヘッド: $\le 20\text{ ns}$（TSC 読み取り2回のみ）

---

## 4. Verification Plan
1. `cargo check --package kernel --target x86_64-unknown-none --release`
2. `cargo xtask run --release`
3. 動作確認:
   - `[c0 | ...ns] latencyos>` プロンプトの動的更新
   - `within 500us run filter.flow` のデッドライン判定
   - `timeline`, `ring`, `cores`, `tsc` コマンドの正常出力
