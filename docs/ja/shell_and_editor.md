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
| `cat` | `readme.txt` | 任意のテキストファイル（`.txt`, `.json`, `.log`, `.pul`）の内容を出力 |
| `edit` | `test.txt` | 任意のファイルを内蔵エディタ PulseEditor で作成・編集（Ctrl ショートカット対応） |
| `compile` / `build` | `stream.pul [out.bin]` | PulseLang スクリプトをスタンドアロンのバイトコードバイナリ（`.bin`）へコンパイル |
| `run` / `exec` | `stream.pul` または `stream.bin` | スクリプトまたはコンパイル済みバイナリを即時実行（バイナリはコンパイル遅延ゼロ） |
| `disasm` | `stream.bin` | バイトコードバイナリを逆アセンブルし、Opcode とオペランドを一覧表示 |
| `hex` / `xxd` | `stream.bin` | バイナリ・テキストファイルの 16 進ダンプ (Hex Dump) を ASCII と共に出力 |
| `pwd` | なし | 現在のカレントワーキングディレクトリを表示 |
| `cd` | `/bin`, `..`, `/` | カレントディレクトリを移動 |
| `mkdir` | `/custom` | LatencyFS に新しいディレクトリを作成 |
| `tree` | なし | 階層的ディレクトリ構造をツリー形式で表示 |
| `touch` | `notes.txt` | 空のファイルを LatencyFS に新規作成 |
| `rm` / `del` | `old.bin` | ファイルまたはディレクトリを LatencyFS から削除 |
| `cp` | `src.pul dst.pul` | ファイルを複製 |
| `mv` | `old.pul new.pul` | ファイル名を変更・移動 |
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
PulseLang のスクリプト（`.pul`）は、その場でコンパイルして実行できるほか、事前に `px64` バイトコードバイナリ（`.bin`）へビルドしておくことで、**コンパイル時間ゼロの $O(1)$ 最速起動**が可能です。また、コマンドライン引数（スペース区切り）を渡すことができます。

1. **コンパイル**:
   ```text
   [c0|14ns] % compile /pulselang/echo.pul /bin/my_echo.bin
   [BUILD] Compiled /pulselang/echo.pul -> /bin/my_echo.bin (191 B binary bytecode, wcet ~4475 ns)
   ```
2. **実行 & 引数受け渡し**:
   ```text
   [c0|12ns] % run /bin/my_echo.bin "hello world"
   hello world

   [c0|12ns] % run /pulselang/echo.pul "arg1" "arg2"
   arg1 arg2
   ```
   ※ `run` コマンドは引数のファイル先頭のマジック（`PX64` または `PULS`）を自動検知するため、`.pul` と `.bin` のいずれも同じ `run <ファイル名> [引数...]` で透過的に実行可能です。

3. **バイナリビューア & 逆アセンブラ (`hex` / `disasm`)**:
   - `hex stream.bin`: バイトコードの 16 進ダンプを出力
   - `disasm stream.bin`: `px64` レジスタマシン命令とオペランド（`$rax`〜`$r15`, `#f0`〜`#f3`）を逆アセンブル表示

```text
[c0|18ns] % disasm /bin/echo.bin
=== px64 Real-Time Architecture Disassembly: /bin/echo.bin ===
Magic: PX64 | Version: 2 | Code: 124 B | Registers: 20 GPRs+HW | StringPool: 51 B
OFFSET  HEX          INSTRUCTION  OPERANDS
---------------------------------------------------------------
0000:   12 01 08 00  CALL_NAT     $rcx = @argc($rax)
0004:   02 00 01 00  MOV          $rax, $rcx
0008:   01 0f 00 00  MOV          $r15, 0
000c:   0d 00 00 0f  CMPGT        $rax, $rax, $r15
0010:   10 00 00 70  JZ           $rax, 0x0070
...
0078:   16 00 00 00  HALT
```

---

### 1.4 階層的ファイルシステムとパス解決
LatencyOS は起動時に以下の標準ディレクトリ階層を自動構築します。
- 絶対パス（`/pulselang/echo.pul`）
- カレントディレクトリ相対パス（`echo.pul`、`pulselang/echo.pul`）
- `cd <dir>` によるカレントワーキングディレクトリの移動と `pwd`

```text
/ (ルート)
├── /pulselang/         # PulseLang v2 スクリプトディレクトリ
│   ├── echo.pul         # コマンドライン引数対応エコー
│   ├── stream.pul       # ゼロコピー GPU-to-NIC パイプライン
│   ├── bench.pul        # リアルタイム演算ベンチマーク
│   ├── filter.pul       # 輻輳制御ガード
│   ├── jitter.pul       # ジッター計測
│   └── telemetry.pul    # ハードウェアテレメトリ
├── /bin/               # コンパイル済み px64 実行可能バイナリ
│   ├── echo.bin        # コンパイル済みバイナリ
│   ├── stream.bin      # コンパイル済みバイナリ
│   ├── bench.bin       # コンパイル済みバイナリ
│   ├── filter.bin      # コンパイル済みバイナリ
│   ├── jitter.bin      # コンパイル済みバイナリ
│   └── telemetry.bin   # コンパイル済みバイナリ
├── /etc/
│   └── config.json     # システム・コア設定ファイル
├── /var/
│   └── /log/
│       └── system.log  # カーネル初期化ログ
└── /home/
    └── readme.txt      # テキストガイド
```

---

## 2. PulseEditor (カーネル内蔵フルスクリーンエディタ)

PulseEditor は、外部の依存関係を持たずに Core 0 上で直接動作する ANSI フルスクリーンテキストエディタです。画面最下部に Nano 風の固定ショートカットバーが常時表示されます。

```text
  1 | // stream.pul - Zero-Copy GPU-to-NIC Ultra-Low-Latency Pipeline
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
[MSG] Ready.
 [^S / F2 Save]  [^R / F5 Run]  [^Q / F10 Quit]  [^X Save&Quit]  [Esc C Clear]
```

### 2.1 キーバインド一覧

| キー操作 | 機能 | 説明 |
|---|---|---|
| `Ctrl + S` / `F2` | **保存** | 現在のバッファを LatencyFS へ即時永続化 |
| `Ctrl + R` / `F5` | **コンパイル & 実行** | エディタを開いたままスクリプトを実行し結果を確認 |
| `Ctrl + Q` / `F10`| **終了** | エディタを終了し Pulse Shell へ復帰 |
| `Ctrl + X` | **保存して終了** | バッファを保存したうえで即座にシェルへ復帰 |
| `Esc C` / `Ctrl + C` | **バッファクリア** | 編集中のテキストを一括消去 |
| `Ctrl + A` / `Home` | **行頭移動** | 現在行の先頭へカーソルを移動 |
| `Ctrl + E` / `End` | **行末移動** | 現在行の末尾へカーソルを移動 |
| `Ctrl + K` | **行末まで削除** | カーソル位置から現在行末までを一括削除 |
| `Ctrl + U` | **行頭まで削除** | カーソル位置から現在行頭までを一括削除 |
| `Ctrl + D` | **1文字削除** | カーソル位置の文字を削除 |
| `Ctrl + L` | **画面再描画** | エディタ画面を強制リフレッシュ |
| `Ctrl + ←` / `Ctrl + →` | **単語ジャンプ** | 前後の単語境界へカーソルを移動 |
| `Tab` | **インデント挿入** | 4 つのスペースを自動挿入 |
| `Backspace` | **文字削除** | カーソル直前の文字を削除 |
| `Delete` (`\x1b[3~`)| **前方文字削除** | カーソル位置の文字を正確に削除（残像なし） |

---

### 2.2 高速コード貼り付け (Paste)
Windows / Mac / Linux からコードをコピーし、エディタ上で `Ctrl + V`（または右クリック / `Shift + Insert`）で貼り付けた場合、UART 受信バッファの自動一括ドレインと CRLF 重複排除機能により、**長いコードやコメント行であっても 1 文字も欠落することなく瞬時に挿入** されます。

---

### 2.3 AI 向け機械可読コンパイルエラーログ
PulseLang コンパイラは、エラー発生時に AI エージェントや自動化ツールが即座に原因特定・修復できるように構造化された診断ログを出力します：

```text
==================== [PULSELANG COMPILE ERROR DIAGNOSTIC (AI-ACTIONABLE)] ====================
[ERROR_CODE]: ERR_SYNTAX_UNEXPECTED_TOKEN
[MESSAGE]: Unexpected token encountered in expression
[FILE]: /home/err_syntax.pul
[LOCATION]: Line 3, Column 10 (ByteOffset: 50)
[TOKEN_FOUND]: Kind: Number(42), Value: "42"
[EXPECTED]: Literal value, variable ($var), hardware handle (#h), or intrinsic call (@fn)
[PARSER_STAGE]: Expression -> Primary
[SOURCE_CONTEXT]:
  Line   2: @contract: @wcet(100us) @budget(500us);
> Line   3: $x := := 42;
                  ^^ [Syntax Error Here]
  Line   4: 
[HEX_DUMP (offset 0x0020..0x0036)]:
  00000020: 28 35 30 30 75 73 29 3b 0a 24 78 20 3a 3d 20 3a  |(500us);.$x := :|
  00000030: 3d 20 34 32 3b 0a                                |= 42;.|
[AI_REPAIR_HINT]: Replace invalid token with a valid variable name, number, or expression
=============================================================================================
```

