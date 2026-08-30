# PulseLang v2 AI向け形式仕様書 & コード生成リファレンス (日本語)

> **対象読者**: AIコーディングエージェント、LLM、静的解析器、形式検証器
> **言語バージョン**: `2.0.0-hard-realtime`
> **実行環境**: `LatencyOS (x86_64 freestanding no_std)`

---

## 1. AI コード生成不変条件 (Invariants)

1. **接頭辞規則**:
   - 変数: 必ず `$`（例: `$rtt`, `$sum`, `$i`, `$t0`）
   - ハードウェアハンドル: 必ず `#`（例: `#f`, `#packet`）
   - ディレクティブ・組み込み命令: 必ず `@`（例: `@contract`, `@within`, `@while`, `@tsc()`）
2. **線形型 `#handle` の単一消費保証**:
   - `#f := @capture();` で取得したハンドルは、すべての実行分岐で厳密に 1 回消費（`@send(#f);`）すること。
3. **時間単位の必須化**:
   - 時間定数には必ず `ns`, `us`, `ms`, `s` を付与すること。
4. **文末セミコロンの必須化**:
   - すべての文末には必ず `;` を付与すること。

---

## 2. 標準コード生成テンプレート

### テンプレート 1: ゼロコピー GPU-to-NIC パイプライン (`stream.pul`)
```pulse
// stream.pul - Zero-Copy Ultra-Low-Latency Pipeline
@pipeline: UltraStream @budget(8000us);

@on_vblank: {
    #f := @capture();
    @within(500us) {
        $rtt := @rtt();
        $rtt > 200us ? @rate(80) : @rate(100);
        @send(#f);
    } !drop;
};
```

### テンプレート 2: 有界反復数学ベンチマーク (`bench.pul`)
```pulse
// bench.pul - Real-Time Bounded Iteration Benchmark
@contract: @wcet(5us) @budget(50us);

$t0 := @tsc();
$sum := 0;
$i := 0;

@while($i < 100) {
    $sum += $i * 2;
    $i += 1;
}

$dt := @tsc() - $t0;
@println("[BENCH] Iterations: 100");
@println("[RESULT] Sum:");
@println($sum);
@println("[LATENCY] Cycles:");
@println($dt);
```

### テンプレート 3: 適応型輻輳制御ガード (`filter.pul`)
```pulse
// filter.pul - Adaptive Congestion Controller
@contract: @wcet(2us) @budget(100us);

$rtt := @rtt();
@println("[FILTER] Measured RTT (ns):");
@println($rtt);

$rtt > 300us ? {
    @println("[ACTION] Congestion detected -> Rate: 60%");
    @rate(60);
} : {
    @println("[ACTION] Optimal latency -> Rate: 100%");
    @rate(100);
};
```

### テンプレート 4: コマンドライン引数エコー (`echo.pul`)
```pulse
// echo.pul - PulseLang Echo Script with Command-Line Argument Support
@contract: @wcet(2us) @budget(20us);
$argc := @argc();
$argc > 0 ? {
    $i := 0;
    @while($i < $argc) {
        @print(@arg($i));
        $i += 1;
        $i < $argc ? @print(" ") : @print("");
    }
    @println("");
} : {
    @println("LatencyOS PulseLang Real-Time Script Engine Active");
};
```

---

## 3. AI 向け機械可読エラー診断フォーマット

エラー発生時、PulseLang は AI エージェントが自律修復可能な構造化ログを出力します。構文解析エラーと実行時エラー（タイムアウト・WCET超過等）でテンプレートが分離されています：

### 3.1 コンパイル時構文エラー診断フォーマット
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

### 3.2 実行時エラー・タイムアウト診断フォーマット
```text
==================== [PULSELANG RUNTIME ERROR DIAGNOSTIC (AI-ACTIONABLE)] ====================
[ERROR_CODE]: ERR_PX64_TIMEOUT_EXCEEDED / ERR_PX64_CONST_OUT_OF_BOUNDS / ERR_PX64_INVALID_OPCODE
[MESSAGE]: <実行時違反の簡潔な説明>
[FILE]: <対象ファイルパス>
[EXECUTION_DOMAIN]: px64 Real-Time Register Virtual Machine
[RUNTIME_FAULT_CATEGORY]: Wall-Clock Watchdog Deadline Violation / Constant Pool Access Violation / Invalid Opcode Execution Fault
[TIMEOUT_LIMIT]: 5,000,000 ns (5.0 ms wall-clock)
[ROOT_CAUSE]: <障害をトリガーした厳密な実行時条件>
[AI_REPAIR_HINT]: <AIが実行すべき具体的な修復手順>
=============================================================================================
```

---

## 4. px64 v3 バイナリヘッダー & 命令セット仕様

### 4.1 16-Byte バイナリヘッダー
```text
Offset  Type    Field             Description
0x00    [u8; 4] Magic             b"PX64" (0x50 0x58 0x36 0x34)
0x04    u16     Version           3 (PX64 v3)
0x06    u16     Code Length       バイトコード命令長（バイト単位）
0x08    u16     String Pool Len   文字列テーブル長（バイト単位）
0x0A    u16     Const Pool Count  64-bit整数定数（i64）のエントリ数
0x0C    u16     Num Registers     20（16 GPR ＋ 4 HW DMAスロット）
0x0E    u16     Reserved          0x0000
```

### 4.2 オペコードリファレンステーブル
| オペコード (Hex) | 命令名 | フォーマット | 説明 | WCET (ベアメタル / QEMU) |
|---|---|---|---|---|
| `0x00` | `NOP` | `00 00 00 00` | 何もしない | 1 ns / 74 ns |
| `0x01` | `MOV_IMM` | `01 Rd imm16` | 16-bit符号なし即値をレジスタにロード | 2 ns / 80 ns |
| `0x02` | `MOV_REG` | `02 Rd Rs1 00` | `$rs1` を `$rd` にコピー | 2 ns / 80 ns |
| `0x03` | `MOV_STR` | `03 Rd off len` | 文字列スライスタグ付き記述子をロード | 3 ns / 82 ns |
| `0x04..0x08` | `ADD/SUB/MUL/DIV/MOD` | `Op Rd Rs1 Rs2` | 整数算術演算 `$rd = $rs1 op $rs2` | 3 ns / 85 ns |
| `0x09..0x0E` | `CMPEQ..CMPGE` | `Op Rd Rs1 Rs2` | 条件比較 `$rd = ($rs1 op $rs2) ? 1 : 0` | 3 ns / 85 ns |
| `0x0F` | `JMP` | `0f 00 imm16` | オフセット `imm16` への無条件ジャンプ | 2 ns / 80 ns |
| `0x10..0x11` | `JZ / JNZ` | `Op Rd imm16` | 条件付き分岐（`$rd == 0` / `$rd != 0`） | 3 ns / 82 ns |
| `0x12` | `CALL_NAT` | `12 Rd func Rs2` | カーネルハードウェアIntrinsics呼び出し | Intrinsics依存 |
| `0x13..0x15` | `WITHIN/DROP` | `Op Rd 00 00` | デッドライン予算ガード | 5 ns / 85 ns |
| `0x16` | `HALT` | `16 00 00 00` | VM実行終了 | 1 ns / 74 ns |
| `0x17` | `LDC` | `17 Rd const_idx` | 定数プールから64-bit定数をロード (`i64`) | **5 ns / 98 ns** |
| `0x18` | `ADDI` | `18 Rd Rs1 imm8` | 8-bit即値加算 `$rd = $rs1 + imm8` | **3 ns / 89 ns** |
| `0x19` | `SUBI` | `19 Rd Rs1 imm8` | 8-bit即値減算 `$rd = $rs1 - imm8` | **3 ns / 89 ns** |

---

## 4. よくある AI 生成ミスと回避策

| 不正なパターン | 不正な理由 | 正しい記述 |
|---|---|---|
| `let x = 10;` | PulseLang では `$var := expr;` を使用 | `$x := 10;` |
| `f := @capture();` | DMA ハンドルには `#` が必須 | `#f := @capture();` |
| `while ($i < 10) {}` | ループには `@while(...) {}` が必須 | `@while($i < 10) {}` |
| `args[0]` | 引数取得には `@arg(i)` を使用 | `@arg(0)` |
| `delay(10);` | 無制限のスリープは禁止 | `@within(Time) {}` を使用 |
| `malloc(1024);` | 動的ヒープ確保は存在しない | 静的スロットのみ使用 |
| `@send(#f)` の欠落 | `#handle` のリークはコンパイルエラー | 送信または明示的に破棄 |
| `500`（単位なし） | 時間には単位接尾辞が必須 | `@within(500us)` |
| `if $x > 0 { ... }` | `if` には丸括弧が必要 | `if ($x > 0) { ... }` または `$x > 0 ? { ... } : { ... };` |

