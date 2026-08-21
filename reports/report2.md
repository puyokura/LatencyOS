# LatencyOS 回帰バグ監査レポート（新ビルド）

**対象配布物**: `LatencyOS.exe`、`runtime.zip`（2026-08-21受領）**検証環境**: QEMU x86_64 / TCG、4 vCPU、128 MiB、Intel 82540EM e1000、外部通信制限**目的**: 前回監査で確認した主要不具合の修正確認と、新ビルドの残存不具合の特定

> **総評**: 今回のビルドでは、PulseLangの算術コード生成、時間単位変換、線形ハンドル検査、静的ループ検査、TSC周波数の一貫性、SMPワーカーの実稼働、LatencyFSのディレクトリ解決という、前回の主要障害が大幅に改善されています。一方で、**相対パスの****`mv`****失敗**と、**8 msを超えるE2E遅延を****`margin: optimal`****と表示するタイムライン判定**が残っています。さらに、QEMU上ではNetwork TX区間が目標値を大幅に超過しています。

## 1. 配布物の確認

| 項目 | 結果 |
| --- | --- |
| `LatencyOS.exe` | 77 MiB、PE32+ x86-64 console executable |
| `LatencyOS.exe` SHA-256 | `58818171c817a2f224627f05095ee41d8d309420bd14865a45aa48ec683f1b97` |
| `runtime.zip` | 76 MiB、完全性検査成功 |
| `runtime.zip` SHA-256 | `8b15f24d2a3e89f94a16bd7fd445e750cd4f4f1a8a3c87167bb64cd43f43caff` |
| 起動カーネル | x86-64 static ELF、SHA-256 `89ff586f4dfea84eee1b111e29a7c61b46e267639670c6e08b17d90a5191feb0` |

## 2. 前回の主要不具合に対する回帰結果

| 旧ID | 前回の問題 | 新ビルドの結果 | 判定 |
| --- | --- | --- | --- |
| BL-01 | 乗算コード生成で両オペランドが同じ仮想レジスタを使い、`bench.pl`が400を出力 | `bench.pl`と`bench.bin`がどちらも**9900**を出力。`MUL $r15, $r15, $r14`となり別レジスタを使用。 | **修正済み** |
| BL-02 | `300us`が300としてコンパイルされる | `filter.bin`は`LDC const[0] (300000)`を出力。RTT 101759 nsで100%レートを選択。 | **修正済み** |
| BL-03 | 未消費ハンドル・二重送信が成功する | 未消費は`ERR_LINEAR_UNCONSUMED_HANDLE`、二重送信は`ERR_LINEAR_DOUBLE_SEND`でコンパイル時拒否。 | **修正済み** |
| BL-04 | `@while(1)`が実行時10,000ステップで初めて停止 | `ERR_UNBOUNDED_LOOP`として、Static Loop Bound Verification段階で拒否。 | **修正済み** |
| BL-05 | `mkdir`直後に`cd`不可、`tree`非同期、非空`rm`が無応答 | `cd qa`は`/qa`へ成功、`tree`に動的ファイルを表示、非空`rm`は即時に`Directory not empty`。 | **大部分修正**（BL-11参照） |
| BL-06 | ブート時と`status`のTSC周波数が2496 MHz対3400 MHzで不一致 | ブート、`status`、`tsc`の全てで**2497 MHz**。 | **修正済み** |
| BL-07 | Core 1〜3はbootedだがinactive・loop 0、パイプライン進捗0 | 全APが`active: true`、loop数が増加。Capture/Encodeカウンタも増加。 | **修正済み** |
| BL-08 | テレメトリーに0値・相互矛盾が多い | パイプラインの実フレーム数と遅延が表示される。だがタイムライン合否に残存不整合あり。 | **部分修正** |
| BL-09 | 予算超過の最大値に`PASS`と表示される | `PASS (p99)`を明示し、`EXCEEDED`も表示。判定基準を併記。 | **改善済み** |
| BL-10 | 添付仕様はPX64 v2、出力はv3 | 出力は引き続き**PX64 Version 3**。 | **未解決** |

## 3. 修正を確認した詳細

### 3.1 PulseLang算術コード生成

`bench.pl`の期待値は `2 × (0 + ... + 99) = 9900` である。新ビルドではソース実行と生成済みバイトコード実行の両方で9900を返した。

```
[BENCH] Iterations: 100
[RESULT] Sum:
9900
```

逆アセンブルは、前回のレジスタ別名問題を解消している。

```
001c: MOV  $r15, $rbx
0020: MOV  $r14, 2
0024: MUL  $r15, $r15, $r14
```

### 3.2 時間リテラル

`filter.pl`の`$rtt > 300us`は、正しく300,000 nsとして生成された。

```
0018: LDC $r15, const[0] (300000)
```

実測RTT 101,759 nsに対する結果も仕様と整合している。

```
[ACTION] Optimal latency -> Rate: 100%
```

### 3.3 線形ハンドルとループの静的検証

次の未消費プログラムはコンパイル時に拒否された。

```
#f := @capture();
```

同様に、以下の二重送信は`ERR_LINEAR_DOUBLE_SEND`、定数無限ループは`ERR_UNBOUNDED_LOOP`となった。

```
#f := @capture();
@send(#f);
@send(#f);

$i := 0;
@while(1) { $i += 1; }
```

これらは、リアルタイムDSLとして重要な所有権・実行境界の保証を大きく改善している。

## 4. 残存不具合

### BL-11: 相対パスの`mv`が失敗する

**重要度: Medium****領域**: LatencyFS / シェルのパス解決

#### 再現手順

```
% mkdir qa
% cd qa
% touch alpha
% cp alpha alpha_copy
% mv alpha_copy beta
mv: error: FileNotFound
```

失敗後も`ls -l`には`alpha_copy`が存在する。絶対パスに変えると成功した。

```
% mv /qa/alpha_copy /qa/beta
% ls -l
-rw-r--r-- ... alpha
-rw-r--r-- ... beta
```

#### 影響

相対パスでのリネーム／移動が信頼できず、シェル操作とスクリプトからのファイル管理を壊す。

#### 推定原因と修正方針

`mv`がカレントディレクトリを片方または両方の引数へ適用していない可能性が高い。`cp`、`mv`、`rm`、`cd`で共通の`resolve_path(cwd, input)`を使い、相対・絶対・`.`・`..`の回帰テストを追加する。

### BL-12: タイムラインの合否ラベルがE2E予算と矛盾する

**重要度: High****領域**: テレメトリー／ユーザー向け判定

#### 観察結果

`timeline`は以下を表示した。

```
stage 5 (network): 10.8ms
 total e2e:        10.8ms (budget: 8.00ms, margin: optimal)
```

E2E 10.8 msは表示上の8.00 ms予算を超過しているため、`margin: optimal`は論理的に不正である。同じブートの1000サンプルベンチマークでも、Network TXおよびE2Eは`EXCEEDED`だった。

| ステージ | 予算 | 平均 | p99 | 最大 | 判定 |
| --- | --- | --- | --- | --- | --- |
| NVENC → Network TX | 500 µs | 4,367 µs | 16,617 µs | 17,184 µs | EXCEEDED |
| E2E | 5,000 µs | 4,403 µs | 16,653 µs | 17,214 µs | EXCEEDED |

#### 影響

「optimal」という表示は、実際にはデッドライン違反のデータパスを健全と誤認させる。リアルタイム運用では重大な観測上の誤りである。

#### 修正方針

`timeline`は`e2e <= budget`を同じ単位・同じデータ源で判定し、超過時は`EXCEEDED`と超過量を表示する。p99判定を採用するなら、`PASS (p99)`のように明示し、最大値・平均値と混同させない。

### BL-13: PX64バイトコード版数と提供仕様の不整合

**重要度: Medium****領域**: ABI・ドキュメント

添付仕様書はPX64バイトコードversion 2を必須としているが、新ビルドの`bench.bin`、`filter.bin`はいずれも`Version: 3`である。実装をv3へ移行済みなら、ヘッダ、opcode、検証器、移行方針を含めて配布仕様を更新する必要がある。

### BL-14: QEMU/TCG環境でNetwork TX目標を満たさない

**重要度: 要実機再検証（現時点では性能問題）****領域**: ネットワークパイプライン

Network TXのp99は16.6 ms、最大17.2 msであり、500 µs目標を大きく超えた。QEMU TCGのホストスケジューリングと仮想e1000が主因となり得るため、これだけで実機カーネルのバグと断定はできない。

ただし、QEMUでも`EXCEEDED`を正しく表示する今回のベンチマーク判定は改善点である。次はKVMまたは実機e1000で、同じトレースID・同じクロック校正を用いて再計測すべきである。

## 5. 改善された起動・SMP・計測

新ビルドでは、4コア全てでワーカーループが観測された。

| Core | ロール | active | ループ数（観測時） |
| --- | --- | --- | --- |
| 0 | Control | true | 8,190,806 |
| 1 | Capture | true | 407,230 |
| 2 | Encode | true | 17,086,961 |
| 3 | Network | true | 1,147 |

パイプライン表示も、Capture/Encode/Networkに実カウントを表示した。

```
capture: 407233 frames, latency 27734 ns
encode:  407234 frames, frame_id 407232
network: 1143 frames, 55080 packets, latency 11143125 ns
```

TSC周波数は起動、`status`、`tsc`で2497 MHzに統一され、`0.40 ns/cycle`と整合した。

## 6. 修正優先度

| 優先度 | 対象 | 完了条件 |
| --- | --- | --- |
| P1 | BL-12 タイムライン判定 | 8 ms超過時に`optimal`を出さず、ベンチマークと同一基準で`EXCEEDED`を表示。 |
| P2 | BL-11 相対`mv` | `mv alpha_copy beta`が`/qa/alpha_copy`を`/qa/beta`へ移動し、絶対指定と同じ結果になる。 |
| P2 | BL-13 PX64版数 | 仕様書・コンパイラ・検証器のversion方針をv2またはv3に統一。 |
| 実機検証 | BL-14 Network TX | KVM/実機でp99および最大値を測定し、500 µs目標との差を評価。 |

## 7. 結論

新ビルドは、前回のP0相当だったPulseLang算術、時間単位、所有権、無限ループ、TSC、SMPの問題を**実際の実行結果で修正確認**できた。現時点で最優先の残存不具合は、予算超過を`optimal`と表示するタイムラインの合否ロジックである。相対`mv`とPX64仕様版数も、早期に回帰テストと文書更新の対象にするべきである。

## 参考

[1]: 本セッションで取得したLatencyOS 0.0.5のQEMUシリアルコンソール出力[2]: ユーザー提供 `PULSELANG_COMPLETE_AI_REFERENCE.md`