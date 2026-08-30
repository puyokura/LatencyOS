# PulseLang v2 Formal AI Specification & Code Generation Reference

> **Target Audience**: AI Coding Assistants, LLMs, Static Analyzers, Formal Verifiers, and Real-Time Systems Engineers.
> **Language Version**: `2.0.0-hard-realtime`
> **Host Kernel**: `LatencyOS (x86_64 freestanding no_std)`

---

## 1. AI Generation Directives & Operational Invariants

When an AI writes PulseLang v2, it **MUST** enforce the following invariant constraints:

1. **Variables & Handles**:
   - Variables **MUST** start with `$` (e.g., `$rtt`, `$sum`, `$i`, `$t0`).
   - Hardware/DMA Linear Handles **MUST** start with `#` (e.g., `#f`, `#packet`).
   - Directives, contracts, and built-in intrinsics **MUST** start with `@` (e.g., `@contract`, `@within`, `@tsc()`).
2. **Deterministic Linear Types**:
   - Every `#handle` obtained via `#f := @capture();` **MUST be consumed exactly once** in every execution branch (e.g. via `@send(#f);`).
   - Handles cannot be duplicated, leaked, or double-freed.
3. **Time Literals**:
   - Time constants **MUST** include valid unit suffixes: `ns` (nanoseconds), `us` (microseconds), `ms` (milliseconds), `s` (seconds).
   - Time literals are auto-promoted to 64-bit integer nanoseconds at compile time (e.g. `500us` $\to$ `500000`).
4. **Statement Semicolons**:
   - Every statement **MUST** end with a semicolon `;` (including assignment, intrinsic call statements, and block close when followed by a contract or directive).
5. **No Dynamic Memory**:
   - PulseLang has **NO heap allocation, NO pointer arithmetic, NO dynamic recursion, and NO unbound loops**.
   - Maximum loop step count is statically bounded.

---

## 2. Formal Machine-Readable Grammar (EBNF)

```ebnf
(* PulseLang v2 Grammar *)

Script          ::= TopLevelDecl* <EOF>

TopLevelDecl    ::= ContractDecl
                  | PipelineDecl
                  | OnVblankDecl
                  | Statement

ContractDecl    ::= "@contract:" WcetSpec? BudgetSpec? ";"
PipelineDecl    ::= "@pipeline:" Identifier BudgetSpec? (";" | Block)
OnVblankDecl    ::= "@on_vblank:" Block ";"?

WcetSpec        ::= "@wcet(" TimeLiteral ")"
BudgetSpec      ::= "@budget(" TimeLiteral ")"

Statement       ::= AssignStmt
                  | CompoundAssignStmt
                  | WithinStmt
                  | WhileStmt
                  | IfStmt
                  | ExprStmt
                  | Block

AssignStmt      ::= (VarIdent | HardwareIdent) ":=" Expression ";"
CompoundAssign  ::= (VarIdent | HardwareIdent) ( "+=" | "-=" ) Expression ";"
WithinStmt      ::= "@within(" TimeLiteral ")" Block ("!drop")? ";"
WhileStmt       ::= "@while(" Expression ")" Block
IfStmt          ::= "if" "(" Expression ")" Block ( "else" Block )?
ExprStmt        ::= Expression ";"

Block           ::= "{" Statement* "}"

Expression      ::= PipeExpr
PipeExpr        ::= TernaryExpr ( "|>" TernaryExpr )*
TernaryExpr     ::= LogicOrExpr ( "?" ( Block | Expression ) ":" ( Block | Expression ) )?
LogicOrExpr     ::= LogicAndExpr ( "||" LogicAndExpr )*
LogicAndExpr    ::= EqualityExpr ( "&&" EqualityExpr )*
EqualityExpr    ::= RelationalExpr ( ( "==" | "!=" ) RelationalExpr )*
RelationalExpr  ::= AdditiveExpr ( ( "<" | "<=" | ">" | ">=" ) AdditiveExpr )*
AdditiveExpr    ::= Multiplicative ( ( "+" | "-" ) Multiplicative )*
Multiplicative  ::= UnaryExpr ( ( "*" | "/" | "%" ) UnaryExpr )*
UnaryExpr       ::= ( "!" | "-" )? PrimaryExpr

PrimaryExpr     ::= IntegerLiteral
                  | TimeLiteral
                  | StringLiteral
                  | VarIdent
                  | HardwareIdent
                  | IntrinsicCall
                  | "(" Expression ")"

IntrinsicCall   ::= ( "@tsc" | "@rtt" | "@rate" | "@capture" | "@send" | "@print" | "@println" ) "(" ArgList? ")"
ArgList         ::= Expression ( "," Expression )*

(* Lexical Tokens *)
IntegerLiteral  ::= [0-9]+
TimeLiteral     ::= [0-9]+ ("ns" | "us" | "ms" | "s")
StringLiteral   ::= '"' [^"]* '"'
VarIdent        ::= "$" [a-zA-Z0-9_]+
HardwareIdent   ::= "#" [a-zA-Z0-9_]+
Identifier      ::= [a-zA-Z_] [a-zA-Z0-9_]*
```

---

## 3. Type System & Operational Semantics

| Type | Storage Representation | Operations Supported | Constraints |
|---|---|---|---|
| **`i64`** | 64-bit signed integer | `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `<=`, `>`, `>=` | Range: $[-2^{63}, 2^{63}-1]$ |
| **`Time`** | 64-bit unsigned nanoseconds (`u64`) | Comparison, addition, subtraction with `i64` | Immediate compile-time reduction |
| **`Handle`** | 8-bit hardware descriptor ID (`#h`) | Passed to `@send()`, assigned once | **Linear Type** (Must be consumed exactly once) |
| **`String`** | Tagged pointer (`0x7FFF_0000 | offset`) | Passed to `@print()`, `@println()` | Read-only static string pool |

### 3.1 Linear Handle Consumption Rules
```
Rule (Handle-Intro):
    #f := @capture();   ==> Context: { #f: Available }

Rule (Handle-Consume):
    @send(#f);          ==> Context: { #f: Consumed }

Rule (Branch-Consistency):
    If Branch1 consumes #f, Branch2 MUST also consume #f.
    Failure to consume in all branches produces compile-time error: "Linear handle #f leaked".
```

---

## 4. Hardware Intrinsics Catalog

| Intrinsic | Signature | Worst-Case Execution Time (WCET) | Description |
|---|---|---|---|
| `@tsc()` | `() -> i64` | **~15 ns** | Reads serialized hardware Time-Stamp Counter (`rdtscp`). |
| `@rtt()` | `() -> i64` | **~20 ns** | Queries active network round-trip time in nanoseconds from NIC driver. |
| `@rate(pct)` | `(i64) -> ()` | **~10 ns** | Sets congestion throttle percentage (range: `10` to `100`). |
| `@capture()` | `() -> #handle`| **~100 ns**| Claims zero-copy GPU frame buffer descriptor slot. |
| `@send(#h)` | `(#handle) -> ()` | **~200 ns**| Enqueues frame buffer to Intel e1000 NIC TX ring and moves ownership. |
| `@print(v)` | `(Any) -> ()` | **~500 ns**| Emits text/number to UART COM1 port (no newline). |
| `@println(v)`| `(Any) -> ()` | **~500 ns**| Emits text/number to UART COM1 port with automatic CRLF normalization. |

---

## 5. Standard AI Code Generation Templates

### Template 1: Zero-Copy GPU-to-NIC Streaming Pipeline (`stream.pul`)
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

### Template 2: Real-Time Bounded Math Loop (`bench.pul`)
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

### Template 3: Dynamic Congestion Controller (`filter.pul`)
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

### Template 4: Jitter Analyzer (`jitter.pul`)
```pulse
// jitter.pul - Cycle-Accurate Hardware Jitter Analyzer
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

---

## 6. Common AI Generation Mistakes & Antipatterns

| Antipattern | Why it is invalid | Correct Form |
|---|---|---|
| `let x = 10;` | PulseLang uses `$var := expr;` | `$x := 10;` |
| `f := @capture();` | Hardware DMA handles must use `#` | `#f := @capture();` |
| `while ($i < 10) {}` | Directives require `@` prefix | `@while($i < 10) {}` |
| `delay(10);` | Unbounded sleeping is forbidden in hard RT | Use `@within(Time) {}` |
| `malloc(1024);` | Dynamic heap allocation does not exist | Pre-allocated static slots only |
| Missing `@send(#f)` | Leaking a `#handle` causes compile error | `#f` must be sent or discarded |
| `500` (without unit in `@within`) | Time limits require unit suffixes | `@within(500us)` |
