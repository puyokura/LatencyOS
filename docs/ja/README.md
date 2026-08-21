# LatencyOS ドキュメントポータル (日本語)

LatencyOS は、汎用性を完全に排し、**「入力から出力までの最悪実行遅延（WCET）の最小化と決定論性（ジッタの完全排除）」** だけを目的関数としてゼロから設計された専用ハードリアルタイムOSです。

---

## 1. ドキュメント一覧

| ドキュメント | 概要 | 主な内容 |
|---|---|---|
| ドキュメント | 概要 | 主な内容 |
|---|---|---|
| [**アーキテクチャ仕様書**](file:///C:/Users/User/Desktop/LatencyOS/docs/ja/architecture.md) | OSの内部構造・コア間パイプライン | 4コア固定割当、SPSC Lock-Free通信、8.0msレイテンシ予算、C-Stateロック |
| [**PulseLang v2 言語ポータル**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ja/README.md) | 言語仕様書・AIリファレンス・ISA・クックブック | 形式文法、型システム、線形型 `#handle`、px64 ISA、スクリプト集 |
| [**シェル & エディタガイド**](file:///C:/Users/User/Desktop/LatencyOS/docs/ja/shell_and_editor.md) | Pulse Shell & PulseEditor 操作手引 | 時間ネイティブプロンプト、`within` ガード、高速一括ペースト、キーバインド、px64 逆アセンブラ |

---

## 2. LatencyOS の基本設計原則

1. **レイテンシ予算駆動（Latency-Budget Driven Design）**:
   - 1080p 60fps における Glass-to-Glass（入力から表示・送信まで）8.0ms 予算を死守。
   - 予算を超過した古いフレームは、後続遅延を悪化させないため即座に破棄（`!drop`）。
2. **動的スケジューラ・コンテキストスイッチの完全排除**:
   - CFS（完全公平スケジューラ）や時分割マルチタスクを廃止。
   - 各 CPU コアを単一のパイプラインステージ（Control / Capture / Encode / Network）にハードウェア固定（Core Affinity）。
3. **動的メモリ確保（malloc/Box）の禁止**:
   - ブート後の実行パスにおけるヒープアロケーションをゼロに固定。
   - すべてのバッファ・リング・ファイルシステムは起動時に静的確保。
4. **ミューテックス・セマフォの禁止**:
   - コア間通信は SPSC（Single-Producer Single-Consumer）Lock-Free リングバッファのみ。
   - アトミックロックによる L3 キャッシュラインバウンスと優先度逆転を構造的に排除。
5. **`px64` 独自ハードリアルタイムレジスタマシンアーキテクチャ**:
   - 20 レジスタ（16 GPR `$rax`〜`$r15` ＋ 4 HW スロット `#f0`〜`#f3`）と 32-bit 固定長命令による確定性 WCET 実行。

---

## 3. クイックスタート (Windows 11 Native)

### 3.1 ツールチェーンの確認
```powershell
$env:PATH = 'C:\Users\User\scoop\apps\llvm\current\bin;C:\Users\User\scoop\apps\rustup\current\.cargo\bin;C:\Users\User\scoop\apps\QEMU\current;C:\Users\User\scoop\shims;' + $env:PATH
cargo run --package xtask -- check
```

### 3.2 対話型モードで起動
```powershell
cargo run --package xtask -- interactive --release
```

### 3.3 基本コマンド例
```text
[c0|12ns] % ls -t
echo.bin         (wcet: ~0.8us , size:  124 B)
stream.bin       (wcet: ~0.8us , size:  108 B)
bench.bin        (wcet: ~0.8us , size:   96 B)
filter.bin       (wcet: ~0.8us , size:  112 B)
jitter.bin       (wcet: ~0.8us , size:   84 B)
telemetry.bin    (wcet: ~0.8us , size:  132 B)

[c0|18ns] % run /bin/echo.bin "px64 register machine active"
px64 register machine active

[c0|24ns] % disasm /bin/echo.bin
=== px64 Real-Time Architecture Disassembly: /bin/echo.bin ===
Magic: PX64 | Version: 2 | Code: 124 B | Registers: 20 GPRs+HW | StringPool: 51 B
OFFSET  HEX          INSTRUCTION  OPERANDS
---------------------------------------------------------------
0000:   12 01 08 00  CALL_NAT     $rcx = @argc($rax)
0004:   02 00 01 00  MOV          $rax, $rcx
...
0078:   16 00 00 00  HALT

[c0|24ns] % edit /pulselang/stream.pl
[c0|15ns] % within 500us run /pulselang/filter.pl
[c0|10ns] % timeline
[c0|14ns] % ring
[c0|11ns] % cores
[c0|08ns] % exit
```

