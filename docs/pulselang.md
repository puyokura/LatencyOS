# PulseLang v2 Formal Specification & Architecture Manual
### AI-Native Temporal Reactive DSL & Hard Real-Time Virtual Machine

---

## 1. Overview & Design Philosophy

PulseLang v2 is a domain-specific language (DSL) and deterministic execution engine designed exclusively for **ultra-low-latency, zero-copy streaming pipelines**.

Unlike traditional human-oriented programming languages that emphasize verbose syntactic sugar (`function`, `let`, `if`, `while`), PulseLang v2 is engineered for:
1. **AI Generation & Formal Verification**: Unambiguous, mathematically dense syntax with explicit hardware register bindings (`$`), hardware slot handles (`#`), and compiler contracts (`@`).
2. **Deterministic Time-Budget Contracts**: Worst-Case Execution Time (WCET) bounds and temporal deadlines (`@within(500us) !drop;`) compiled directly into bytecode operations.
3. **Zero Dynamic Allocation**: 100% pre-allocated static token buffers, fixed-size AST-free single-pass bytecode generation, and zero runtime heap allocations.
4. **Hardware Direct Access**: Direct zero-copy integration with CPU TSC counters, GPU frame buffers, and Intel e1000 Poll-Mode Drivers (PMD).

---

## 2. Syntax & Formal Grammar

### 2.1 Lexical Elements

| Category | Syntax | Description | Example |
|---|---|---|---|
| **Directives & Contracts** | `@<identifier>` | Static compiler contracts and pipeline definitions | `@contract:`, `@pipeline:`, `@budget()`, `@wcet()` |
| **Hardware Intrinsics** | `@<func>(...)` | Direct CPU/GPU/NIC hardware invocations | `@tsc()`, `@rtt()`, `@rate(100)`, `@capture()`, `@send(#f)` |
| **Registers & Variables** | `$<name>` | Static memory slot binding | `$rtt`, `$sum`, `$i`, `$t0`, `$dt` |
| **Hardware Handles** | `#<name>` | Zero-copy DMA slot / frame descriptor | `#f`, `#frame`, `#slot0` |
| **Walrus Assignment** | `:=` | Register value assignment | `$rtt := @rtt();` |
| **Compound Mutations** | `+=`, `-=` | In-place arithmetic mutation | `$sum += $i * 2;` |
| **Ternary Guards** | `<cond> ? { ... } : { ... };` | Branching without keywords | `$rtt > 300us ? @rate(60) : @rate(100);` |
| **Stream Pipe** | `\|>` | Zero-copy stage pipelining | `#f := @capture() \|> @send(#f);` |
| **Deadline Guard** | `@within(<time>) { ... } !drop;` | Hard deadline enforcement | `@within(500us) { @send(#f); } !drop;` |
| **Time Literals** | `<number>(ns\|us\|ms\|s)` | First-class nanosecond time values | `50ns`, `200us`, `5ms`, `1s` |

---

## 3. Intrinsics Reference

| Intrinsic | Signature | Worst-Case Execution Time | Description |
|---|---|---|---|
| `@tsc()` | `() -> i64` | ~12 ns | Reads hardware serialized Time Stamp Counter (`lfence; rdtsc`). |
| `@rtt()` | `() -> i64` | ~8 ns | Reads minimum measured network Round-Trip Time in nanoseconds. |
| `@rate(pct)` | `(i64) -> ()` | ~15 ns | Adjusts network congestion throttle percentage (10% - 100%). |
| `@capture()` | `() -> i64` | ~700 ns | Zero-copy GPU frame capture synchronized with VBLANK edge. Returns hardware slot ID. |
| `@send(#handle)` | `(i64) -> ()` | ~1200 ns | Transmits frame via kernel-bypass Intel e1000 driver with SRTP/AES-GCM encryption. |
| `@argc()` | `() -> i64` | ~5 ns | Returns the count of CLI arguments passed to the script (0..8). |
| `@arg(idx)` | `(i64) -> Tagged` | ~10 ns | Returns tagged pointer to CLI argument at index `idx`. |
| `@print(val)` | `(any) -> ()` | ~800 ns | Prints string literal, argument, or integer value to serial console without heap allocation. |
| `@println(val)` | `(any) -> ()` | ~900 ns | Prints string/argument/integer followed by CRLF to serial console. |

---

## 4. Standard Scripts (`.pl`)

### 4.1 `bench.pl` (Realtime Math & Micro-Benchmark)
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

### 4.2 `filter.pl` (Adaptive Congestion Guard)
```pulse
// filter.pl - Adaptive Congestion Guard [AI-Native Spec]
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

### 4.3 `stream.pl` (Zero-Copy Pipeline Stream)
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

---

## 5. Virtual Machine Architecture

- **Stack Size**: 64 entries (64-bit signed integers).
- **Static Variable Slots**: 32 entries (`$0` to `$31`).
- **Static String Pool**: 512 bytes with tagged pointer representation (`0x7FFF_...`).
- **Step Limit**: 10,000 instructions max (hard infinite loop & WCET breach prevention).
- **Deadline Stack**: 8-level nested temporal deadline stack with hardware TSC comparison.
