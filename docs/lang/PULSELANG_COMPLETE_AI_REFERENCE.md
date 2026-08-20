# PulseLang v2 Complete Monolithic AI Reference & Autonomous Code Generation Manual

> **Document Type**: Monolithic All-in-One AI Specification & In-Context Generation Reference
> **Target Audience**: AI Coding Agents, LLMs, Compilers, Static Analyzers, Formal Verification Engines
> **Language Version**: `2.0.0-hard-realtime`
> **Host Kernel**: `LatencyOS (x86_64 freestanding no_std)`
> **Zero-Dependency Guarantee**: This single document contains the complete syntax, type system, operational semantics, standard intrinsics, ISA, and generation rules required for an AI to generate 100% valid PulseLang v2 programs.

---

## 1. AI System Prompt & Core Invariants

You are an expert real-time systems compiler and autonomous code generation agent for **PulseLang v2**, the native domain-specific language of **LatencyOS**.

When generating PulseLang code, you **MUST ALWAYS** follow these five core invariants without exception:

1. **Prefix Discipline (Zero Ambiguity)**:
   - **`$`** for all Variables (`$rtt`, `$sum`, `$i`, `$t0`, `$dt`).
   - **`#`** for all Linear Hardware/DMA Buffer Handles (`#f`, `#packet`, `#frame`).
   - **`@`** for all Contracts, Control Structures, and Intrinsics (`@contract`, `@pipeline`, `@on_vblank`, `@within`, `@while`, `@tsc()`, `@rtt()`, `@rate()`, `@capture()`, `@send()`, `@print()`, `@println()`).
2. **Linear Type Single Consumption Guarantee**:
   - Every handle obtained via `#f := @capture();` **MUST be consumed exactly once** in every execution branch (typically via `@send(#f);`).
   - A handle cannot be copied, discarded without handling, or double-freed.
3. **Mandatory Time Units**:
   - Every time literal **MUST** include an explicit unit suffix: `ns` (nanoseconds), `us` (microseconds), `ms` (milliseconds), `s` (seconds).
   - Time literals are auto-folded at compile time into 64-bit unsigned integer nanoseconds (e.g. `500us` $\to$ `500_000`).
4. **Mandatory Statement Semicolons**:
   - Every statement **MUST** end with a semicolon `;`.
5. **Zero Dynamic Allocation & Bounded Execution**:
   - PulseLang has **NO heap allocation (`malloc`/`Box` do not exist)**, **NO dynamic pointer arithmetic**, and **NO unbounded recursion**.
   - Loops **MUST** be monotonically bounded. The runtime enforces a hard limit of 10,000 instructions per execution.

---

## 2. Complete Formal Grammar (EBNF)

```ebnf
(* PulseLang v2 Complete Grammar *)

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
                  | CompoundAssign
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

(* Lexical Terminals *)
IntegerLiteral  ::= [0-9]+
TimeLiteral     ::= [0-9]+ ("ns" | "us" | "ms" | "s")
StringLiteral   ::= '"' [^"]* '"'
VarIdent        ::= "$" [a-zA-Z0-9_]+
HardwareIdent   ::= "#" [a-zA-Z0-9_]+
Identifier      ::= [a-zA-Z_] [a-zA-Z0-9_]*
```

---

## 3. Type System & Memory Model

| Type | Internal Representation | Operations Supported | Semantic Rules |
|---|---|---|---|
| **`i64`** | 64-bit signed integer | `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `<=`, `>`, `>=` | General computation, loop counter, arithmetic |
| **`Time`** | 64-bit unsigned integer (ns) | Immediate comparison with `i64`, addition, subtraction | Auto-converted to integer nanoseconds at compile time |
| **`Handle`** | 8-bit descriptor slot ID (`#h`) | Passed to `@send()`, assigned once | **Linear Type**: Must be consumed exactly once in all branches |
| **`String`** | Tagged pointer (`0x7FFF_0000 \| offset`) | Passed to `@print()`, `@println()` | Read-only reference to static 512-byte string pool |

### 3.1 Linear Handle Proof Invariant
```
[Capture]   #f := @capture();   --> #f is ACTIVE
[Branch]    $cond ? {
                @send(#f);      --> #f is CONSUMED (Branch 1 OK)
            } : {
                @send(#f);      --> #f is CONSUMED (Branch 2 OK)
            };
[Invariant] #f MUST be consumed in all code paths. Leaking #f is a static compilation error.
```

---

## 4. Hardware Intrinsics Catalog

| Intrinsic | Type Signature | Worst-Case Execution Time (WCET) | Description |
|---|---|---|---|
| `@tsc()` | `() -> i64` | **~15 ns** | Reads hardware Time-Stamp Counter (`rdtscp`) with full serialization. |
| `@rtt()` | `() -> i64` | **~20 ns** | Queries active network round-trip time (in nanoseconds) from NIC PMD. |
| `@rate(pct)` | `(i64) -> ()` | **~10 ns** | Sets congestion throttle percentage (range: `10` to `100`). |
| `@capture()` | `() -> #handle` | **~100 ns** | Claims zero-copy GPU frame buffer descriptor slot. |
| `@send(#h)` | `(#handle) -> ()` | **~200 ns** | Enqueues frame buffer to Intel e1000 NIC TX ring and moves ownership. |
| `@print(v)` | `(Any) -> ()` | **~500 ns** | Emits value/string to serial port (no newline). |
| `@println(v)`| `(Any) -> ()` | **~500 ns** | Emits value/string to serial port with automatic CRLF normalization. |

---

## 5. Bytecode ISA Specification

The PulseLang VM is a deterministic, stack-based virtual machine running with pre-allocated static resources:
- **Stack Depth**: 64 entries (`i64`)
- **Variable Slots**: 32 entries (`$0` to `$31`)
- **Instruction Step Limit**: 10,000 steps

| Opcode | Mnemonic | Operands | Stack Effect | Description |
|---|---|---|---|---|
| `0x00` | `OP_NOP` | None | `[] -> []` | No operation |
| `0x01` | `OP_PUSH_CONST` | `i64` (8 bytes) | `[] -> [val]` | Push 64-bit constant |
| `0x02` | `OP_LOAD_VAR` | `u8` (1 byte) | `[] -> [var[idx]]` | Load from variable slot |
| `0x03` | `OP_STORE_VAR` | `u8` (1 byte) | `[val] -> []` | Store top of stack to variable slot |
| `0x04` | `OP_ADD` | None | `[a, b] -> [a + b]` | Integer addition |
| `0x05` | `OP_SUB` | None | `[a, b] -> [a - b]` | Integer subtraction |
| `0x06` | `OP_MUL` | None | `[a, b] -> [a * b]` | Integer multiplication |
| `0x07` | `OP_DIV` | None | `[a, b] -> [a / b]` | Integer division (zero-safe) |
| `0x08` | `OP_MOD` | None | `[a, b] -> [a % b]` | Integer modulo |
| `0x09` | `OP_CMP_EQ` | None | `[a, b] -> [a == b]` | Equality comparison |
| `0x0A` | `OP_CMP_NE` | None | `[a, b] -> [a != b]` | Inequality comparison |
| `0x0B` | `OP_CMP_LT` | None | `[a, b] -> [a < b]` | Less than comparison |
| `0x0C` | `OP_CMP_LE` | None | `[a, b] -> [a <= b]` | Less than or equal |
| `0x0D` | `OP_CMP_GT` | None | `[a, b] -> [a > b]` | Greater than comparison |
| `0x0E` | `OP_CMP_GE` | None | `[a, b] -> [a >= b]` | Greater than or equal |
| `0x0F` | `OP_JUMP` | `u16` (2 bytes) | `[] -> []` | Unconditional jump |
| `0x10` | `OP_JUMP_IF_FALSE`| `u16` (2 bytes)| `[cond] -> []` | Conditional branch on false (0) |
| `0x11` | `OP_CALL_NATIVE` | `u8, u8` (2 bytes)| `[args...] -> [res]` | Call hardware intrinsic |
| `0x12` | `OP_WITHIN_START`| `i64` (8 bytes) | `[] -> []` | Push temporal deadline (ns) |
| `0x13` | `OP_WITHIN_END` | None | `[] -> []` | Pop and verify temporal deadline |
| `0x14` | `OP_DROP` | None | `[] -> []` | Drop overdue frame & free descriptors |
| `0x15` | `OP_PUSH_STR` | `u16, u16` (4B) | `[] -> [ptr]` | Push static string pointer |
| `0x16` | `OP_HALT` | None | `[] -> []` | Terminate VM execution |

---

## 6. Canonical Production Script Catalog (10 Complete Examples)

### Example 1: Zero-Copy Ultra-Low-Latency Stream Pipeline (`stream.pl`)
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

### Example 2: Cycle-Accurate Latency & Arithmetic Benchmark (`bench.pl`)
```pulse
// bench.pl - Realtime Math & Latency Benchmark
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

### Example 3: Adaptive Congestion Controller (`filter.pl`)
```pulse
// filter.pl - Adaptive Congestion Guard
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

### Example 4: Hardware Jitter Analyzer (`jitter.pl`)
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

### Example 5: Real-Time Hardware Telemetry (`telemetry.pl`)
```pulse
// telemetry.pl - Real-Time Hardware Telemetry Inspector
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

### Example 6: Dual-Threshold Dynamic Rate Controller (`rate_guard.pl`)
```pulse
// rate_guard.pl - Multi-Tier Adaptive Rate Guard
@contract: @wcet(3us) @budget(40us);

$rtt := @rtt();

$rtt > 1000us ? {
    @println("[TIER-3] Severe Congestion -> Rate: 30%");
    @rate(30);
} : {
    $rtt > 400us ? {
        @println("[TIER-2] Moderate Delay -> Rate: 70%");
        @rate(70);
    } : {
        @println("[TIER-1] Optimal Path -> Rate: 100%");
        @rate(100);
    };
};
```

### Example 7: Burst Packet Counter (`packet_burst.pl`)
```pulse
// packet_burst.pl - High-Frequency Packet Loop with Deadline
@contract: @wcet(8us) @budget(80us);

$count := 0;
$t_start := @tsc();

@within(50us) {
    @while($count < 64) {
        $count += 1;
    }
} !drop;

$elapsed := @tsc() - $t_start;
@println("[BURST] Processed Packets:");
@println($count);
@println("[BURST] Elapsed Cycles:");
@println($elapsed);
```

### Example 8: Monotonic Linear Search (`search.pl`)
```pulse
// search.pl - Bounded Array Target Search Simulation
@contract: @wcet(4us) @budget(30us);

$target := 42;
$found := 0;
$i := 0;

@while($i < 50) {
    $i == $target ? {
        $found := 1;
    } : {};
    $i += 1;
}

$found == 1 ? @println("[SEARCH] Target 42 found.") : @println("[SEARCH] Target not found.");
```

### Example 9: Deadline Health Watchdog (`watchdog.pl`)
```pulse
// watchdog.pl - Real-Time Pipeline Watchdog
@contract: @wcet(2us) @budget(15us);

$rtt := @rtt();
$tsc := @tsc();

@within(10us) {
    $rtt > 5000us ? {
        @println("[CRITICAL] RTT exceeded 5ms! Triggering safe throttle.");
        @rate(10);
    } : {
        @rate(100);
    };
} !drop;
```

### Example 10: Matrix Vector Product Simulation (`matvec.pl`)
```pulse
// matvec.pl - Fixed 4x4 Dot Product Inner Loop
@contract: @wcet(6us) @budget(60us);

$dot := 0;
$row := 0;

@while($row < 4) {
    $dot += $row * 10 + 5;
    $row += 1;
}

@println("[MATVEC] Dot product result:");
@println($dot);
```

---

## 7. Common AI Generation Mistakes & Fixes

| Incorrect Code | Cause of Failure | Correct Code |
|---|---|---|
| `let x = 10;` | PulseLang does not use `let`/`var` keywords | `$x := 10;` |
| `f := @capture();` | Hardware DMA handles must use `#` prefix | `#f := @capture();` |
| `while ($i < 10) {}` | Directives require `@` prefix | `@while($i < 10) {}` |
| `delay(10);` | Unbounded sleep functions do not exist | Use `@within(Time) {}` |
| `malloc(1024);` | Dynamic heap memory is strictly forbidden | Use pre-allocated `$var` slots |
| `print("hi")` | Built-in functions require `@` prefix | `@print("hi");` |
| Missing `@send(#f)` | Leaking a linear handle `#f` causes compile error | `#f` must be sent or consumed |
| `@within(500)` (no unit) | Time constants require unit suffix | `@within(500us)` |
| `$sum = $sum + 1` (no `;`) | Every statement requires a trailing semicolon | `$sum += 1;` |
| `$cond ? a : b` (as stmt) | Ternary statements require terminating semicolon | `$cond ? $a : $b;` |

---

## 8. AI Code Generation Pre-Flight Checklist

Before finalizing any generated PulseLang v2 code, verify every item on this checklist:

- [ ] Every variable starts with `$` (`$var`).
- [ ] Every hardware handle starts with `#` (`#handle`).
- [ ] Every directive / intrinsic starts with `@` (`@contract`, `@tsc()`, etc.).
- [ ] Every statement terminates with `;`.
- [ ] Every `#handle` is consumed exactly once in every possible branch.
- [ ] Every time constant includes a valid unit (`ns`, `us`, `ms`, `s`).
- [ ] Every `@while` loop has a strictly monotonic increment/decrement (e.g. `$i += 1;`).
- [ ] No `let`, `var`, `function`, `def`, `class`, `malloc`, `free`, or `return` keywords are used.
