# LatencyOS コマンド挙動・逆アセンブル追加調査

**対象**: 新しい `runtime.zip` の `kernel`（LatencyOS 0.0.5、`/etc/config.json`表示は0.0.22）**実施方式**: QEMU x86_64 / TCG、4 vCPU、128 MiB、e1000、外部通信制限**目的**: シェルコマンド、PulseLang実行、`hex`、`disasm`、異常バイトコード検証器の実挙動を追加確認する。

> **結論**: シェルの正常系・異常系、PulseLangの引数処理、コンパイル、バイトコード検証器は概ね堅牢に動作した。特に不正opcodeと定数プール範囲外参照は安全に診断・停止する。一方で、相対`mv`の失敗、予算超過なのに`optimal`と表示するタイムライン、PX64 version 3と仕様version 2の不一致は前回どおり残っている。追加で、逆アセンブラの範囲外定数表示と、負荷時のシリアル出力混線を改善候補として確認した。

## 1. コマンド挙動サマリー

| コマンド群 | 正常系 | 異常系 | 評価 |
| --- | --- | --- | --- |
| `pwd`, `ls -l`, `ls -t`, `cat` | ルート、詳細一覧、WCET形式一覧、`config.json`を正常表示 | 存在しないファイルは明確なエラー | 良好 |
| `cd`, `mkdir`, `tree`, `touch`, `rm` | 動的な`/qa`を作成・移動・ツリー反映できる | 非空ディレクトリの`rm`は即時に拒否 | 前回から改善 |
| `cp`, `mv` | `cp`と絶対パス`mv`は成功 | 相対`mv alpha_copy beta`は`FileNotFound` | **残存バグ** |
| `within` | 100 msで`status`実行・時間計測に成功 | 1 nsはdeadline violation、無効時間文字列を拒否 | 良好 |
| `compile`, `run`, `disasm`, `hex` | `.pul`コンパイル、`.bin`実行・解析・hex表示に成功 | 存在しない入力を一貫して拒否 | 良好 |
| `echo.pul` | 引数なしの既定メッセージ、複数引数の連結出力に成功 | — | 良好 |

## 2. シェル・LatencyFSの詳細

### 2.1 正常に動いた操作

以下は期待どおりに機能した。

```
% mkdir qa
% cd qa
% pwd
/qa
% touch alpha
% cp alpha alpha_copy
% cd /
% tree
|-- /qa/
|   |-- alpha
|   |-- alpha_copy
```

非空ディレクトリを削除しようとした際も、以前の無応答ではなく、明示的に拒否した。

```
% rm qa
rm: cannot remove 'qa': Directory not empty
```

`within`も実行時間と判定を表示した。

```
% within 100ms status
[within 100ms: PASSED (actual: 2.4ms)]

% within 1ns status
[within 1ns: DEADLINE VIOLATED (actual: 1.1ms, delta: +1.1ms)]

% within invalid status
within: invalid time specification: 'invalid'
```

### 2.2 残存: 相対`mv`のパス解決不良

**ID: BL-11（継続）／重要度: Medium**

カレントディレクトリが`/qa`の状態で、存在するファイルの相対リネームが失敗した。

```
% cp alpha alpha_copy
% mv alpha_copy beta
mv: error: FileNotFound
```

同じ操作を絶対パスで行うと成功した。

```
% mv /qa/alpha_copy /qa/beta
% ls -l
-rw-r--r-- ... alpha
-rw-r--r-- ... beta
```

`mv`が両引数に対してカレントディレクトリを正規化していない可能性が高い。`cp`、`mv`、`rm`、`cd`に同一の`resolve_path(cwd, input)`を適用するべきである。

### 2.3 低優先度: シリアル出力の一時的な混線

最初の`help`および、複数コマンドを連続投入した際の`compile`出力に、一部文字列の崩れ／行間混線が観測された。単独の再実行では`help`は完全に正しく表示されたため、常時再現する機能不全ではない。

この現象は、SMPワーカーまたはシリアル画面更新とシェル出力が同じUART書込み経路を共有し、行単位の排他制御がない可能性を示す。実機UARTでも同様なら、ログの解析性と監査証跡を損なう。

**推奨**: UART出力を行バッファ化し、スピンロックまたはCore 0への単一ライタキューで直列化する。負荷中に`help`、`disasm`、`pipeline`を同時実行する回帰試験を追加する。

## 3. PulseLangスクリプト実行

| スクリプト | 実行結果 | 所見 |
| --- | --- | --- |
| `echo.pul` | 引数なしで既定文、`alpha beta gamma`で同文言を出力 | `@argc` / `@arg`の基本経路は動作 |
| `stream.pul` / `stream.bin` | エラーなしで完了 | `pipeline`のCapture/Encode/Networkカウンタが増加 |
| `jitter.pul` | `Consecutive TSC Delta: 46128 cycles`、`Jitter detected` | QEMU TCG上の非決定性を正しく可視化 |
| `telemetry.pul` | TSCとRTT（101504 ns）を出力 | テレメトリーintrinsicの基本経路は動作 |
| `bench.pul` / `bench.bin` | 平方和でなく、仕様どおりの加算結果9900を出力 | 算術コード生成の修正を再確認 |

## 4. バイトコード形式と逆アセンブル

### 4.1 `bench.bin`

`hex bench.bin`の先頭は次の通りで、`PX64`マジックとversion 3を確認した。

```
00000000: 50 58 36 34 00 03 00 6c 00 35 00 00 00 14 00 00
           P  X  6  4     3
```

逆アセンブルでは、乗算の左右オペランドが`$r15`と`$r14`に分離されている。

```
001c: MOV  $r15, $rbx
0020: MOV  $r14, 2
0024: MUL  $r15, $r15, $r14
```

これは、前ビルドの`MUL $r15, $r15, $r15`による算術誤りが修正済みであることを示す。

### 4.2 `stream.bin`

`stream.pul`のコンパイル結果は104 B、静的WCETは約2300 nsと表示された。逆アセンブルは、ハードウェアハンドル、時間定数、デッドライン、RTT分岐、レート制御、送信を明示している。

```
0000: CALL_NAT     #f0 = @capture($rax)
0004: LDC          $rax, const[0] (500000)
0008: WITHIN_START budget:$rax
0014: LDC          $r15, const[1] (200000)
0018: CMPGT        $rax, $rax, $r15
0024: CALL_NAT     $rax = @rate($r15)
0038: CALL_NAT     $rax = @send($r15)
003c: WITHIN_END
0040: DROP
0044: HALT
```

時間リテラルは`500us → 500000ns`、`200us → 200000ns`に正しく畳み込まれている。`DROP`は仕様上、締切超過時にのみ実際のリソース回収を行う命令であり、逆アセンブルに常に存在すること自体は不具合ではない。

### 4.3 残存: ABI版数の不整合

**ID: BL-13（継続）／重要度: Medium**

生成済み`bench.bin`、`stream.bin`、異常系テストバイナリはいずれも`PX64 Version: 3`と表示される。添付されたPulseLang仕様書はversion 2を必須としているため、仕様・コンパイラ・VM検証器の版数方針を統一する必要がある。

## 5. バイトコード検証器と異常系

| テスト | `disasm` | `run` | 評価 |
| --- | --- | --- | --- |
| `/bin/test_invalid_op.bin` | `UNKNOWN_OP_0xfe`を表示 | `ERR_PX64_INVALID_OPCODE`で停止 | 良好 |
| `/bin/test_oob_const.bin` | `LDC $rax, const[99] (0)`を表示 | `ERR_PX64_CONST_OUT_OF_BOUNDS`で停止 | 実行検証は良好、表示改善余地 |
| 存在しない`.pul`/`.bin` | — | `compile`/`run`/`disasm`/`hex`が`cannot access` | 良好 |

### 5.1 低優先度: 範囲外定数の逆アセンブル表示

**ID: BL-15（新規）／重要度: Low**

`test_oob_const.bin`はヘッダ上`ConstPool: 0 entries`であるにもかかわらず、逆アセンブラは以下のように表示する。

```
LDC $rax, const[99] (0)
```

実行時VMは正しく`ERR_PX64_CONST_OUT_OF_BOUNDS`を返すため、安全性の欠陥ではない。しかし、`(0)`は実在する定数値のように見え、デバッグ時の誤解を招く。

**推奨**: `LDC $rax, const[99] (<out of bounds>)`と表示し、必要なら「const-pool length: 0」を同じ行に含める。

## 6. パイプライン反映

`stream.bin`実行後、`pipeline`と`ring`は実データを示した。

```
capture: 4882871 frames, latency 37526 ns
encode:  4882872 frames, frame_id 4882870
network: 14596 frames, 701028 packets, latency 11920976 ns, drops: 9

ring: capacity 8, occupancy 0/8, head 4882872, tail 4882872
```

Capture/Encode/Networkカウンタが進行しており、前ビルドのゼロ固定表示は解消されている。ただしNetwork遅延は約11.9 msであり、設定上の8 ms E2E目標と500 µs Network目標に対する実機／KVMでの再評価は必要である。

## 7. 追加後の優先順位

| 優先度 | ID | 対象 | 対応 |
| --- | --- | --- | --- |
| P1 | BL-12 | タイムラインがE2E超過でも`optimal`を表示 | 判定式・表示基準をベンチマークと統一 |
| P2 | BL-11 | 相対`mv`失敗 | 共通の相対パス正規化を導入 |
| P2 | BL-13 | PX64 v2/v3仕様不整合 | 仕様・バイトコードヘッダ・検証器を統一 |
| P3 | BL-15 | 範囲外constを`(0)`と表示 | `<out of bounds>`へ明示表示 |
| P3 | BL-16 | 高負荷時のUART出力混線の可能性 | UART出力を単一ライタ／行バッファ化して負荷試験 |

## 参考

[1]: 本追加調査で取得したLatencyOS QEMUシリアルコンソール出力[2]: ユーザー提供 `PULSELANG_COMPLETE_AI_REFERENCE.md`