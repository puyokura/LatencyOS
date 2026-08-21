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

### テンプレート 1: ゼロコピー GPU-to-NIC パイプライン (`stream.pl`)
```pulse
// stream.pl - Zero-Copy Ultra-Low-Latency Pipeline
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

### テンプレート 2: 有界反復数学ベンチマーク (`bench.pl`)
```pulse
// bench.pl - Real-Time Bounded Iteration Benchmark
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

### テンプレート 3: 適応型輻輳制御ガード (`filter.pl`)
```pulse
// filter.pl - Adaptive Congestion Controller
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

### テンプレート 4: コマンドライン引数エコー (`echo.pl`)
```pulse
// echo.pl - PulseLang Echo Script with Command-Line Argument Support
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
[FILE]: /home/err_syntax.pl
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
[ERROR_CODE]: ERR_PX64_TIMEOUT_EXCEEDED
[MESSAGE]: Execution exceeded 5.0ms wall-clock execution deadline (watchdog safety violation)
[FILE]: /loop_cap.pl
[EXECUTION_DOMAIN]: px64 Real-Time Register Virtual Machine
[RUNTIME_FAULT_CATEGORY]: Wall-Clock Watchdog Deadline Violation
[TIMEOUT_LIMIT]: 5,000,000 ns (5.0 ms wall-clock)
[ROOT_CAUSE]: Script execution exceeded 5.0ms wall-clock threshold (infinite loop or long-running intrinsics)
[AI_REPAIR_HINT]: Bound while loops with finite counter or insert @within temporal deadline guards
=============================================================================================
```

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

