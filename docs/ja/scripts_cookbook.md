# PulseLang スクリプトクックブック (日本語)

本ドキュメントでは、LatencyOS に標準搭載されているスクリプトの解説と、独自のハードリアルタイム処理を記述するための実践パターン（レシピ）を紹介します。

---

## 1. 標準スクリプト解説

### 1.1 `stream.pl` (GPU-to-NIC ゼロコピーパイプライン)
```pulse
// stream.pl - Zero-Copy GPU-to-NIC Ultra-Low-Latency Pipeline
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
- **解説**:
  - `@pipeline: UltraStream @budget(8000us);`: Glass-to-Glass 全体予算を 8.0ms に宣言。
  - `@on_vblank:`: GPU の垂直同期割り込み（VBLANK）に同期して毎フレーム起動。
  - `#f := @capture();`: キャプチャスロットの線形ハンドルを取得。
  - `@within(500us) { ... } !drop;`: 500 \textmu s 以内にパケット送信まで完了しなかった場合、古いフレームを即座に破棄。
  - `$rtt > 200us ? @rate(80) : @rate(100);`: 往復遅延が 200 \textmu s を超えた場合に送信レートを 80% に自動絞り込み。

---

### 1.2 `bench.pl` (実時間数学 & レイテンシベンチマーク)
```pulse
// bench.pl - Realtime Math & Latency Benchmark [AI-Native Spec]
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
- **解説**:
  - 100 回のループ計算を実行し、前後のハードウェア TSC 差分（消費クロックサイクル数）をナノ秒単位で計測・表示します。

---

### 1.4 `jitter.pl` (連続 TSC によるハードウェアジッタ計測)
```pulse
// jitter.pl - Cycle-Accurate Jitter Analyzer
@contract: @wcet(3us) @budget(30us);
$t1 := @tsc();
$t2 := @tsc();
$delta := $t2 - $t1;
@println("[JITTER] Consecutive TSC Delta (Cycles):");
@println($delta);
$delta < 100 ? {
    @println("[STATUS] Determinism: Optimal (<100 cycles)");
} : {
    @println("[STATUS] Determinism: Jitter detected");
};
```

### 1.5 `echo.pl` (コマンドライン引数エコー & 文字列整形)
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
- **解説**:
  - `@argc()`: シェルから渡された引数の総数を取得。
  - `@arg($i)`: ゼロアロケーションで $i$ 番目の引数文字列を出力。

---

## 2. 実践レシピ集

### レシピ 1: 輻輳検知付きアダプティブパケット送信
```pulse
@contract: @wcet(4us) @budget(50us);
$rtt := @rtt();
$rtt > 1000us ? {
    @println("[WARN] High latency detected -> Throttling to 50%");
    @rate(50);
} : {
    @rate(100);
};
```

### レシピ 2: 厳密な有界ループによる積和演算
```pulse
@contract: @wcet(10us) @budget(100us);
$acc := 0;
$k := 0;
@while($k < 50) {
    $acc += $k * $k;
    $k += 1;
}
@println("[ACCUMULATOR]");
@println($acc);
```

### レシピ 3: デッドライン付きハードウェアテレメトリ収集
```pulse
@contract: @wcet(5us) @budget(20us);
@within(15us) {
    $now := @tsc();
    $net := @rtt();
    @println("[TELEMETRY]");
    @println($now);
    @println($net);
} !drop;
```

