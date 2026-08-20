# Pulse Shell & PulseEditor ユーザーズガイド (日本語)

LatencyOS の対話環境は、マイクロ秒・ナノ秒単位のハードウェア観測性を最優先にした **Pulse Shell** と、カーネル内蔵フルスクリーンテキストエディタ **PulseEditor** で構成されています。

---

## 1. Pulse Shell (時間駆動型 Unix ミニマリストシェル)

### 1.1 プロンプトの構造
```text
[c0|18ns] % 
```
- `c0`: 現在コマンドが実行されている CPU コア番号（Core 0）。
- `18ns`: **直前のコマンドの実行にかかったハードウェア TSC レイテンシ ($\Delta t$)**。
- `%`: コマンド入力待ち記号。

---

### 1.2 コマンド一覧

| コマンド | 引数例 | 説明 |
|---|---|---|
| `help` | なし | 利用可能コマンド一覧と最悪実行時間の表示 |
| `ls` | `-l`, `-t` | ファイル一覧。`-t` で各スクリプトの静的 WCET を表示 |
| `cat` | `bench.pl` | ファイル内容を画面に出力（CRLF 自動正規化） |
| `run` | `stream.pl` | PulseLang スクリプトをコンパイルして即時実行 |
| `edit` | `jitter.pl` | 内蔵テキストエディタ PulseEditor を起動 |
| `within` | `500us run filter.pl` | 指定時間以内にコマンドが完了するかハードウェア検証 |
| `timeline` | なし | 6 つのパイプラインステージのマイクロ秒内訳を表示 |
| `ring` | なし | SPSC Lock-Free リングバッファの占有率とポインタを表示 |
| `cores` | なし | 各 CPU コアの APIC ID、役割、C0 ロック状態を表示 |
| `tsc` | なし | ハードウェア TSC のシリアル化現在値とクロック分解能を表示 |
| `doc pulse` | なし | カーネル内蔵の PulseLang v2 形式言語仕様書を表示 |
| `exit` / `poweroff`| なし | ACPI ハードウェア電源切断を行いホストへ復帰 |

---

### 1.3 `within` デッドラインガードの使い方
コマンドの実行時間が指定した予算内に収まるかをハードウェア TSC で厳密に検証します。
```text
[c0|15ns] % within 500us run filter.pl
[FILTER] Measured RTT (ns):
44772
[ACTION] Optimal latency -> Rate: 100%
[within] Execution time: 14.82us (Budget: 500.00us) -> PASSED
```

---

## 2. PulseEditor (カーネル内蔵フルスクリーンエディタ)

PulseEditor は、外部の依存関係を持たずに Core 0 上で直接動作する ANSI フルスクリーンテキストエディタです。

```text
 LatencyOS PulseEditor | File: stream.pl       | Size:  192B | Line:  1 Col:  1
  1 | // stream.pl - Zero-Copy GPU-to-NIC Ultra-Low-Latency Pipeline
  2 | @pipeline: UltraStream @budget(8000us);
  3 | @on_vblank: {
  4 |     #f := @capture();
  5 |     @within(500us) {
  6 |         $rtt := @rtt();
  7 |         $rtt > 200us ? @rate(80) : @rate(100);
  8 |         @send(#f);
  9 |     } !drop;
 10 | };
--------------------------------------------------------------------------------
[MSG] Ready. (Ctrl+S: Save, Ctrl+R: Run, Ctrl+Q: Quit)
```

### 2.1 キーバインド一覧

| キー操作 | 機能 | 説明 |
|---|---|---|
| `Ctrl + S` | **保存** | 現在のバッファを LatencyFS へ即時永続化 |
| `Ctrl + R` | **コンパイル & 実行** | エディタを開いたままスクリプトを実行し結果を確認 |
| `Ctrl + Q` | **終了** | エディタを終了し Pulse Shell へ復帰 |
| `Ctrl + C` | **バッファクリア** | 編集中のテキストを一括消去 |
| `Ctrl + ←` | **左単語ジャンプ** | カーソルを前の単語の先頭へ移動 |
| `Ctrl + →` | **右単語ジャンプ** | カーソルを次の単語の先頭へ移動 |
| `Home` / `End` | **行頭 / 行末移動** | 現在行の先頭または末尾へ瞬時に移動 |
| `Tab` | **インデント挿入** | 4 つのスペースを自動挿入 |
| `Backspace` | **文字削除** | カーソル直前の文字を削除 |
| `Delete` | **文字削除** | カーソル位置の文字を削除 |

---

### 2.2 高速一括貼り付け (Bracketed Paste)
- ホスト OS（Windows / PowerShell）から大量のコードをコピーし、エディタ上で `Ctrl + V`（または右クリック）で貼り付けた場合でも、UART 受信 FIFO の自動一括ドレイン機能により、**1 文字も欠落することなく瞬時にコードが挿入** されます。
