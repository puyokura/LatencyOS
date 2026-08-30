# PulseLang v2 Scripts Cookbook

Practical scripts and real-time development recipes for LatencyOS.

---

## 1. Standard In-Kernel Scripts

### 1.1 `stream.pul` (Zero-Copy GPU-to-NIC Pipeline)
```pulse
// stream.pul - Zero-Copy GPU-to-NIC Ultra-Low-Latency Pipeline
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

### 1.2 `bench.pul` (Cycle-Accurate Latency Benchmark)
```pulse
// bench.pul - Realtime Math & Latency Benchmark [AI-Native Spec]
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

### 1.3 `filter.pul` (Adaptive Congestion Guard)
```pulse
// filter.pul - Adaptive Congestion Guard [AI-Native Spec]
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

### 1.4 `jitter.pul` (Hardware Jitter Analyzer)
```pulse
// jitter.pul - Cycle-Accurate Jitter Analyzer
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

### 1.5 `telemetry.pul` (Real-Time Hardware Telemetry)
```pulse
// telemetry.pul - Real-Time Hardware Telemetry
@contract: @wcet(2us) @budget(20us);
$rtt := @rtt();
$tsc := @tsc();
@println("=== LatencyOS Hardware Telemetry ===");
@println("[CLOCK] Serialized TSC Ticks:");
@println($tsc);
@println("[NET] Active Round-Trip Time (ns):");
@println($rtt);
$rtt < 100us ? @println("[HEALTH] Sub-100us glass-to-glass latency guaranteed.") : @println("[HEALTH] RTT backpressure active.");
```

### 1.6 `echo.pul` (CLI Argument Echo & String Formatter)
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

