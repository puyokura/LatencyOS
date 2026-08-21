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
### Template 4: Command-Line Argument Echo (`echo.pl`)
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

## 3. AI-Actionable Machine-Readable Diagnostic Format

When an error occurs, PulseLang emits structured, deterministic diagnostic logs specifically engineered for autonomous AI agents to repair code without human intervention. Compile-time syntax errors and runtime VM execution faults use distinct, specialized templates:

### 3.1 Compile-Time Syntax Diagnostic Format
```text
==================== [PULSELANG COMPILE ERROR DIAGNOSTIC (AI-ACTIONABLE)] ====================
[ERROR_CODE]: <Machine readable error identifier: e.g. ERR_SYNTAX_UNEXPECTED_TOKEN>
[MESSAGE]: <Concise description of the invariant violation>
[FILE]: <Target file path>
[LOCATION]: Line <N>, Column <M> (ByteOffset: <B>)
[TOKEN_FOUND]: Kind: <TokenKind>, Value: "<Literal>"
[EXPECTED]: <Exact expected token or grammatical construction>
[PARSER_STAGE]: <Compiler pipeline stage>
[SOURCE_CONTEXT]:
  Line   L-1: <Previous line>
> Line   L:   <Error line>
              ^^^^ [Syntax Error Here]
  Line   L+1: <Next line>
[HEX_DUMP (offset 0x...)]:
  <Hex and ASCII bytes around error offset>
[AI_REPAIR_HINT]: <Precise, actionable repair recipe for the AI agent>
=============================================================================================
```

### 3.2 Runtime Execution & Timeout Diagnostic Format
```text
==================== [PULSELANG RUNTIME ERROR DIAGNOSTIC (AI-ACTIONABLE)] ====================
[ERROR_CODE]: ERR_PX64_TIMEOUT_EXCEEDED
[MESSAGE]: Execution exceeded 5.0ms wall-clock execution deadline (watchdog safety violation)
[FILE]: <Target file path>
[EXECUTION_DOMAIN]: px64 Real-Time Register Virtual Machine
[RUNTIME_FAULT_CATEGORY]: Wall-Clock Watchdog Deadline Violation
[TIMEOUT_LIMIT]: 5,000,000 ns (5.0 ms wall-clock)
[ROOT_CAUSE]: Script execution exceeded 5.0ms wall-clock threshold (infinite loop or long-running intrinsics)
[AI_REPAIR_HINT]: Bound while loops with finite counter or insert @within temporal deadline guards
=============================================================================================
```

---

## 4. Common AI Mistakes to Avoid

| Invalid Pattern | Why it is invalid | Correct Form |
|---|---|---|
| `let x = 10;` | PulseLang uses `$var := expr;` | `$x := 10;` |
| `f := @capture();` | Hardware DMA handles must use `#` | `#f := @capture();` |
| `while ($i < 10) {}` | Directives require `@` prefix | `@while($i < 10) {}` |
| `args[0]` | Command-line arguments use `@arg(i)` | `@arg(0)` |
| `delay(10);` | Unbounded sleeping is forbidden | Use `@within(Time) {}` |
| `malloc(1024);` | Dynamic heap allocation does not exist | Pre-allocated static slots only |
| Missing `@send(#f)` | Leaking a `#handle` causes compile error | `#f` must be sent or discarded |
| `500` (without unit in `@within`) | Time limits require unit suffixes | `@within(500us)` |
| `if $x > 0 { ... }` | `if` requires parentheses | `if ($x > 0) { ... }` or `$x > 0 ? { ... } : { ... };` |

