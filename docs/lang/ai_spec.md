# PulseLang v2 AI Specification & Code Generation Reference

> **Target Audience**: AI Coding Assistants, LLMs, Static Analyzers, Formal Verifiers.
> **Language Version**: `2.0.0-hard-realtime`
> **Host Kernel**: `LatencyOS (x86_64 freestanding no_std)`

---

## 1. AI Generation Invariant Constraints

1. **Variables & Handles**:
   - Variables **MUST** start with `$` (e.g. `$rtt`, `$sum`, `$i`, `$t0`).
   - Hardware Linear Handles **MUST** start with `#` (e.g. `#f`, `#packet`).
   - Directives and built-in intrinsics **MUST** start with `@` (e.g. `@contract`, `@within`, `@tsc()`).
2. **Linear Type Single Consumption**:
   - Every `#handle` obtained via `#f := @capture();` **MUST be consumed exactly once** in every execution branch (e.g. via `@send(#f);`).
3. **Time Literals**:
   - Time constants **MUST** include valid unit suffixes: `ns`, `us`, `ms`, `s`.
4. **Statement Semicolons**:
   - Every statement **MUST** end with a semicolon `;`.
5. **No Dynamic Memory**:
   - No heap allocation, no pointer arithmetic, no unbounded recursion.

---

## 2. Standard Code Generation Templates

### Template 1: Zero-Copy Pipeline (`stream.pl`)
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

### Template 2: Bounded Iteration Benchmark (`bench.pl`)
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

### Template 3: Congestion Guard (`filter.pl`)
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

---

## 3. Common AI Mistakes to Avoid

| Invalid Pattern | Why it is invalid | Correct Form |
|---|---|---|
| `let x = 10;` | PulseLang uses `$var := expr;` | `$x := 10;` |
| `f := @capture();` | Hardware DMA handles must use `#` | `#f := @capture();` |
| `while ($i < 10) {}` | Directives require `@` prefix | `@while($i < 10) {}` |
| `delay(10);` | Unbounded sleeping is forbidden | Use `@within(Time) {}` |
| `malloc(1024);` | Dynamic heap allocation does not exist | Pre-allocated static slots only |
| Missing `@send(#f)` | Leaking a `#handle` causes compile error | `#f` must be sent or discarded |
| `500` (without unit in `@within`) | Time limits require unit suffixes | `@within(500us)` |
