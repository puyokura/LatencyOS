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
