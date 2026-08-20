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
| `ls` | `-l`, `-t`, `[dir]` | ファイル一覧。`-l` でパーミッション・サイズ、`-t` で種別と静的 WCET を表示 |
| `cat` | `readme.txt` | 任意のテキストファイル（`.txt`, `.json`, `.log`, `.pl`）の内容を出力 |
| `edit` | `test.txt` | 任意のファイルを内蔵エディタ PulseEditor で作成・編集（Ctrl ショートカット対応） |
| `compile` / `build` | `stream.pl [out.bin]` | PulseLang スクリプトをスタンドアロンのバイトコードバイナリ（`.bin`）へコンパイル |
| `run` / `exec` | `stream.pl` または `stream.bin` | スクリプトまたはコンパイル済みバイナリを即時実行（バイナリはコンパイル遅延ゼロ） |
| `disasm` | `stream.bin` | バイトコードバイナリを逆アセンブルし、Opcode とオペランドを一覧表示 |
| `hex` / `xxd` | `stream.bin` | バイナリ・テキストファイルの 16 進ダンプ (Hex Dump) を ASCII と共に出力 |
| `pwd` | なし | 現在のカレントワーキングディレクトリを表示 |
| `cd` | `/bin`, `..`, `/` | カレントディレクトリを移動 |
| `mkdir` | `/custom` | LatencyFS に新しいディレクトリを作成 |
| `tree` | なし | 階層的ディレクトリ構造をツリー形式で表示 |
| `touch` | `notes.txt` | 空のファイルを LatencyFS に新規作成 |
| `rm` / `del` | `old.bin` | ファイルまたはディレクトリを LatencyFS から削除 |
| `cp` | `src.pl dst.pl` | ファイルを複製 |
| `mv` | `old.pl new.pl` | ファイル名を変更・移動 |
| `within` | `500us run filter.bin` | 指定時間以内にコマンドが完了するかハードウェア検証 |
| `timeline` | なし | 6 つのパイプラインステージのマイクロ秒内訳を表示 |
| `ring` | なし | SPSC Lock-Free リングバッファの占有率とポインタを表示 |
| `cores` | なし | 各 CPU コアの APIC ID、役割、C0 ロック状態を表示 |
| `tsc` | なし | ハードウェア TSC のシリアル化現在値とクロック分解能を表示 |
| `status` | なし | 各コアのリアルタイムループ回数とシステム稼働時間を表示 |
| `pipeline` | なし | ストリーミングフレームの送受信統計カウンタを表示 |
| `latency` | なし | ハードウェア・ドライバ・ネットワークの遅延内訳を表示 |
| `benchmark` | なし | 1,000 サンプルのハードウェア遅延ベンチマークを実行 |
| `congestion` | なし | 輻輳制御アルゴリズムの RTT 統計と帯域スロットル状態を表示 |
| `power` | なし | RAPL 電力測定と CPU 温度テレメトリを表示 |
| `pci` | なし | PCI バスのデバイス一覧（Intel e1000 NIC 等）を表示 |
| `clear` | なし | ターミナル画面をクリア |
| `doc pulse` | なし | カーネル内蔵の PulseLang v2 形式言語仕様書を表示 |
| `exit` / `poweroff`| なし | ACPI ハードウェア電源切断を行いホストへ復帰 |

---

### 1.3 コンパイル済みバイナリの実行方法 (`compile` & `run`)
PulseLang のスクリプト（`.pl`）は、その場でコンパイルして実行できるほか、事前にバイトコードバイナリ（`.bin`）へビルドしておくことで、**コンパイル時間ゼロの $O(1)$ 最速起動**が可能です。

1. **コンパイル**:
   ```text
   [c0|14ns] % compile stream.pl stream.bin
   [BUILD] Compiled stream.pl -> stream.bin (108 B binary bytecode, wcet ~2400 ns)
   ```
2. **実行**:
   ```text
   [c0|12ns] % run stream.bin
   [STREAM] Glass-to-Glass Pipeline executing on Core 1-3...
   ```
   ※ `run` コマンドは引数のファイル先頭のマジック（`PULS`）を自動検知するため、`.pl` と `.bin` のいずれも同じ `run <ファイル名>` で実行可能です。

3. **バイナリビューア (`hex` / `xxd` / `disasm`)**:
   - `hex stream.bin`: バイトコードの 16 進ダンプを出力
   - `disasm stream.bin`: バイトコードの Opcode とオペランドを逆アセンブル

---

### 1.4 階層的ファイルシステムと初期ディレクトリ構成
LatencyOS は起動時に以下の標準ディレクトリ階層を自動構築します。すべてのスクリプト（`.pl`）とコンパイル済みバイナリ（`.bin`）は `/bin/` 配下に格納されています。

```text
/ (ルート)
├── /bin/
│   ├── stream.pl       # ゼロコピー GPU-to-NIC パイプライン
│   ├── stream.bin      # コンパイル済みバイナリ (即時実行用)
│   ├── bench.pl        # リアルタイム演算ベンチマーク
│   ├── bench.bin       # コンパイル済みバイナリ
│   ├── filter.pl       # 輻輳制御ガード
│   ├── filter.bin      # コンパイル済みバイナリ
│   ├── jitter.pl       # ジッター計測
│   ├── jitter.bin      # コンパイル済みバイナリ
│   ├── telemetry.pl    # ハードウェアテレメトリ
│   └── telemetry.bin   # コンパイル済みバイナリ
├── /etc/
│   └── config.json     # システム・コア設定ファイル
├── /var/
│   └── /log/
│       └── system.log  # カーネル初期化ログ
└── /home/
    └── readme.txt      # テキストガイド
```

### 1.5 カレントディレクトリ限定 `ls` の動作
`ls` コマンドは**現在のカレントワーキングディレクトリ内のファイル・ディレクトリのみ**を表示します。

```text
[c0|12ns] % pwd
/
[c0|10ns] % ls
bin/  etc/  var/  home/

[c0|14ns] % cd /bin
[c0|10ns] % pwd
/bin
[c0|12ns] % ls
stream.pl  stream.bin  bench.pl  bench.bin  filter.pl  filter.bin  jitter.pl  jitter.bin  telemetry.pl  telemetry.bin

[c0|15ns] % ls -t
stream.pl        (wcet: ~3.2us , size:  192 B)
stream.bin       (wcet: ~0.8us , size:  108 B)
bench.pl         (wcet: ~3.2us , size:  248 B)
bench.bin        (wcet: ~0.8us , size:   96 B)
...

[c0|10ns] % cd /etc
[c0|11ns] % ls
config.json
```

---

## 2. PulseEditor (カーネル内蔵フルスクリーンエディタ)

PulseEditor は、外部の依存関係を持たずに Core 0 上で直接動作する ANSI フルスクリーンテキストエディタです。`.pl` スクリプトのほか、`.txt`、`.json`、`.log`、`.md` などあらゆるテキストファイルを編集できます。
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
[MSG] Ready. (Ctrl+S: Save, Ctrl+R: Run, Ctrl+Q: Quit, Ctrl+C: Clear)
 [^S Save]  [^R Run]  [^Q Quit]  [^C Clear]  [^X Save&Quit]  [^K KillLine]
```

### 2.1 キーバインド一覧 (Ctrl 操作完全対応)

| キー操作 | 機能 | 説明 |
|---|---|---|
| `Ctrl + S` | **保存** | 現在のバッファを LatencyFS へ即時永続化 |
| `Ctrl + R` | **コンパイル & 実行** | エディタを開いたままスクリプトを実行し結果を確認 |
| `Ctrl + Q` | **終了** | エディタを終了し Pulse Shell へ復帰 |
| `Ctrl + X` | **保存して終了** | バッファを保存したうえで即座にシェルへ復帰 |
| `Ctrl + C` | **バッファクリア** | 編集中のテキストを一括消去 |
| `Ctrl + A` / `Home` | **行頭移動** | 現在行の先頭へカーソルを移動 |
| `Ctrl + E` / `End` | **行末移動** | 現在行の末尾へカーソルを移動 |
| `Ctrl + K` | **行末まで削除 (Kill Line)** | カーソル位置から現在行末までを一括削除 |
| `Ctrl + U` | **行頭まで削除** | カーソル位置から現在行頭までを一括削除 |
| `Ctrl + D` | **1文字削除** | カーソル位置の文字を削除（Delete 相当） |
| `Ctrl + L` | **画面再描画** | エディタ画面を強制リフレッシュ |
| `Ctrl + ←` | **左単語ジャンプ** | カーソルを前の単語の先頭へ移動 |
| `Ctrl + →` | **右単語ジャンプ** | カーソルを次の単語の先頭へ移動 |
| `Tab` | **インデント挿入** | 4 つのスペースを自動挿入 |
| `Backspace` | **文字削除** | カーソル直前の文字を削除 |
| `Delete` | **文字削除** | カーソル位置の文字を削除 |

---

### 2.2 Windows からのコード貼り付け (Paste)
- Windows / Zed / VS Code / PowerShell からコードをコピーし、エディタ上で `Ctrl + V`（または右クリック / `Shift + Insert`）で貼り付けた場合、UART 受信バッファの自動一括ドレインと CRLF 重複排除機能により、**1 文字も欠落することなくインデントを維持したまま瞬時にコードが挿入** されます。
