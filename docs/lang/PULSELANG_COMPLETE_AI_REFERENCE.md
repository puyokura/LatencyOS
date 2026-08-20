# PulseLang v2 Complete Monolithic AI Reference & System Architecture Specification

> **Document Type**: Exhaustive All-in-One AI Specification, Formal Semantics & Low-Level Architectural Reference
> **Target Audience**: AI Coding Assistants, LLMs, Static Analyzers, Formal Verification Engines, Kernel Engineers
> **Language Version**: `2.0.0-hard-realtime`
> **Host Kernel**: `LatencyOS (x86_64 freestanding no_std)`
> **Zero-Dependency Guarantee**: This single document defines the complete syntax, type system, linear logic, hardware intrinsics, bytecode ISA, memory ordering, DMA coherency, and 43 formal architectural contracts.

---

## 1. AI System Prompt & Five Invariants

When generating PulseLang v2 code, an AI agent **MUST ALWAYS** follow these five core invariants:

1. **Prefix Taxonomy (Zero Ambiguity)**:
   - **`$`** for all Variables (`$rtt`, `$sum`, `$i`, `$t0`, `$dt`).
   - **`#`** for all Linear Hardware/DMA Buffer Handles (`#f`, `#packet`, `#frame`).
   - **`@`** for all Contracts, Control Structures, and Intrinsics (`@contract`, `@pipeline`, `@on_vblank`, `@within`, `@while`, `@tsc()`, `@rtt()`, `@rate()`, `@capture()`, `@send()`, `@print()`, `@println()`).
2. **Linear Type Single Consumption Proof**:
   - Every handle obtained via `#f := @capture();` **MUST be consumed exactly once** in every possible execution path (typically via `@send(#f);`).
   - Handles cannot be duplicated, leaked, or double-freed.
3. **Mandatory Explicit Time Units**:
   - Time constants **MUST** include valid unit suffixes: `ns` (nanoseconds), `us` (microseconds), `ms` (milliseconds), `s` (seconds).
   - Time literals are auto-folded at compile time into 64-bit unsigned integer nanoseconds (`500us` $\to$ `500_000`).
4. **Mandatory Statement Semicolons**:
   - Every statement **MUST** terminate with a semicolon `;`.
5. **Zero Dynamic Allocation & Bounded Execution**:
   - No heap allocation (`malloc`/`Box`), no pointer arithmetic, no unbounded recursion.
   - Every loop is statically bounded. Runtime hard limit: 10,000 bytecode instructions.

---

## 2. Complete Formal Grammar (EBNF)

```ebnf
Script          ::= TopLevelDecl* <EOF>

TopLevelDecl    ::= ContractDecl
                  | PipelineDecl
                  | OnVblankDecl
                  | Statement

ContractDecl    ::= "@contract:" ("@wcet(" TimeLiteral ")")? ("@budget(" TimeLiteral ")")? ";"
PipelineDecl    ::= "@pipeline:" Identifier ("@budget(" TimeLiteral ")")? (";" | Block)
OnVblankDecl    ::= "@on_vblank:" Block ";"?

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

IntegerLiteral  ::= [0-9]+
TimeLiteral     ::= [0-9]+ ("ns" | "us" | "ms" | "s")
StringLiteral   ::= '"' [^"]* '"'
VarIdent        ::= "$" [a-zA-Z0-9_]+
HardwareIdent   ::= "#" [a-zA-Z0-9_]+
Identifier      ::= [a-zA-Z_] [a-zA-Z0-9_]*
```

---

## 3. The 43 Master Architectural & Semantic Specifications

### 1. Specification WCET Value Alignment
All documentation, compiler models, and shell telemetry use unified worst-case execution times:
- Bytecode Instruction Base Dispatch: **25 ns**
- `@tsc()`: **15 ns**
- `@rtt()`: **20 ns**
- `@rate()`: **10 ns**
- `@capture()`: **100 ns**
- `@send()`: **200 ns**
- `@print()` / `@println()`: **500 ns**
- End-to-End Glass-to-Glass Budget: **8,000 \textmu s (8.00 ms)**

### 2. Intrinsic WCET vs VM Instruction WCET
Total program WCET is computed statically as:
$$\text{WCET}_{\text{total}} = \sum (\text{Opcode Count} \times 25\text{ ns}) + \sum (\text{Intrinsic WCET})$$

### 3. Time and i64 Typing Rules
`Time` is a compile-time dimensional wrapper over `u64` nanoseconds. In VM bytecode, `Time` literals are folded into immediate 64-bit signed integers (`i64`). Relational and arithmetic operations between `Time` and `i64` evaluate with zero runtime casting overhead.

### 4. String Tagged Pointer Semantics
Strings stored in the static 512-byte string pool are represented on the VM stack as tagged 64-bit pointers:
$$\text{Ptr} = \mathtt{0x7FFF\_0000\_0000\_0000} \mid (\text{len} \ll 16) \mid \text{offset}$$
The VM verifies that $\text{offset} + \text{len} \le 512$ before accessing the pool.

### 5. Handle and DMA Completion
`#f := @capture()` claims an index from the GPU capture ring buffer. `@send(#f)` moves the descriptor into the Intel e1000 NIC TX ring and executes an `sfence` barrier. DMA completion is detected via descriptor writeback status flags (`E1000_TXD_STAT_DD`).

### 6. Handle Lifecycle on `!drop`
When an `@within(Time) { ... } !drop;` block breaches its deadline, the runtime executes `OP_DROP`. If `#f` has not been sent, the DMA descriptor slot is immediately marked free and reclaimed, preventing stale packet transmission.

### 7. ABI of `OP_CALL_NATIVE`
- Opcode: `0x11`
- Operand 1 (`u8`): `func_id` (`1`..`7`)
- Operand 2 (`u8`): `argc` (number of arguments)
Arguments are popped from the VM evaluation stack in right-to-left order.

### 8. Return Value Rules for `OP_CALL_NATIVE`
- Void intrinsics (`@rate`, `@println`, `@send`): Push `0` or nothing.
- Value intrinsics (`@tsc`, `@rtt`, `@capture`): Push return value (`i64` or `handle_id`) onto the evaluation stack.

### 9. Nesting Rules for `OP_WITHIN_START` / `OP_WITHIN_END`
The VM maintains an 8-level hardware deadline stack (`deadline_stack[0..7]`). Inner deadlines must have equal or shorter expiration timestamps than outer deadlines ($\text{Deadline}_{\text{inner}} \le \text{Deadline}_{\text{outer}}$).

### 10. `OP_DROP` Execution Condition
`OP_DROP` (`0x14`) executes if and only if $\text{read\_tsc}() > \text{deadline\_tsc}$.

### 11. Resource Recovery on VM Abort / Timeout
If execution exceeds the 10,000-step limit or encounters a fatal error:
1. The deadline stack pointer (`dl_sp`) is reset to 0.
2. All 32 variable slots are zeroed.
3. Any unconsumed `#handle` descriptor is reclaimed.

### 12. Unification of Control Constructs
- `if (cond) { ... } else { ... }` is compiled to `OP_JUMP_IF_FALSE` and `OP_JUMP`.
- Ternary expressions `$cond ? expr1 : expr2` and ternary blocks `$cond ? { ... } : { ... };` follow identical jump semantics.
- Directives (`@contract`, `@within`, `@while`) provide temporal and real-time bounding.

### 13. Type Checking of Handles in Conditional Branches
If a `#handle` is acquired prior to a branch, **both** branches must consume `#handle`. Leaking a handle in one branch produces a compile-time error.

### 14. Handle Rules inside Loops
A `#handle` captured inside an `@while` loop must be consumed within the same iteration. Handles cannot escape the loop body.

### 15. Handle Behavior on `@capture` Failure
If the GPU frame ring is exhausted, `@capture()` returns `0` (null descriptor). The script can check `$handle != 0` or rely on zero-overhead kernel drop.

### 16. Handle Behavior on `@send` Failure
If the NIC TX ring is full, `@send()` drops the frame, increments backpressure counters, and marks the handle as consumed.

### 17. Division by Zero Semantics
`OP_DIV` (`0x07`) and `OP_MOD` (`0x08`) with a divisor of `0` return `0` without triggering a CPU trap or panic.

### 18. Integer Arithmetic Overflow Semantics
All 64-bit integer arithmetic (`OP_ADD`, `OP_SUB`, `OP_MUL`) wraps using two's complement arithmetic (`wrapping_add`, `wrapping_sub`, `wrapping_mul`).

### 19. Boolean Representation in Comparison Operations
Comparison operations (`OP_CMP_EQ` through `OP_CMP_GE`) push `1` for true and `0` for false.

### 20. Internal Boolean Type
Booleans are represented as `i64`: `0` is false, any non-zero value is true.

### 21. String Pointer Memory Safety
String pointers are offset-based indices into a fixed read-only buffer. Raw arbitrary pointers cannot be constructed from user space.

### 22. Static String Pool Overflow
If total literal string bytes exceed 512 bytes, the compiler aborts with `String pool overflow`.

### 23. VM Stack Overflow Protection
The VM stack is fixed at 64 elements. Attempting to push to a full stack returns `Err("Stack overflow")`.

### 24. Bytecode Verification
Before execution, the VM verifies:
- File starts with magic `PULS` (`0x50554C53`).
- Bytecode length matches file header.
- Jump targets reside within valid instruction boundaries.

### 25. Bytecode Versioning
Version `2` (`0x0002`) is required in bytes 4-5 of the binary header.

### 26. Intrinsic ID ABI Specification
- `1`: `NATIVE_PRINT` (`@print`)
- `2`: `NATIVE_PRINTLN` (`@println`)
- `3`: `NATIVE_SYS_TSC` (`@tsc`)
- `4`: `NATIVE_NET_RTT` (`@rtt`)
- `5`: `NATIVE_NET_SET_RATE` (`@rate`)
- `6`: `NATIVE_GPU_CAPTURE` (`@capture`)
- `7`: `NATIVE_NET_SEND` (`@send`)

### 27. Hardware Target Specification
- CPU: x86_64 with invariant TSC (`CPUID.80000007H:EDX[8] = 1`).
- NIC: Intel 82540EM / 82545EM (e1000) PMD.
- GPU: Linear Framebuffer 1920x1080 @ 32bpp.
- Cores: 4 SMP cores with dedicated roles.

### 28. TSC Time Unit
1 TSC tick = 1 CPU clock cycle (e.g. 0.294 ns at 3.40 GHz).

### 29. TSC Ticks to Nanoseconds Conversion
$$\text{Nanoseconds} = \frac{\text{Ticks} \times 1,000,000,000}{\text{TSC Frequency (Hz)}}$$

### 30. CPU Frequency Scaling & C-State Invariance
All 4 cores are locked in C0 state via MSR `0x1A0` (`MISC_ENABLE`) and MSR `0x1B0` (`ENERGY_PERF_BIAS = 0x0`). Invariant TSC ensures cycle counts remain constant across temperature changes.

### 31. Interrupts and ISR WCET Bounds
- Cores 1-3 run with interrupts disabled (`cli`).
- Core 0 handles APIC timer and UART interrupts, with ISR execution time bounded to $\le 150\text{ ns}$.

### 32. Cache Miss WCET Modeling
Worst-case execution time modeling assumes L1/L2 cache residency for hot loops (< 4 ns), and cold DRAM fetches bounded to 100 ns.

### 33. DMA Cache Coherency
DMA memory regions are allocated in uncached (UC) or write-combining (WC) pages. `sfence` and `clflush` ensure CPU-to-NIC coherency without bus snooping stalls.

### 34. `sfence` and `mfence` Issuance Conditions
- `sfence` is issued after writing frame descriptor headers.
- `mfence` is issued when updating SPSC lock-free ring buffer tail pointers.

### 35. 4-Core Memory Ordering Model
Core-to-core communication uses single-producer single-consumer (SPSC) ring buffers with atomic `Acquire`/`Release` ordering matching x86 TSO (Total Store Order).

### 36. VBLANK Event Contention Avoidance
Core 1 exclusively polls the GPU VBLANK status register, eliminating SMP lock contention.

### 37. Pipeline Buffer Lifecycle
`Stage 0 (ISR)` $\to$ `Stage 1 (Userspace)` $\to$ `Stage 2 (VBLANK)` $\to$ `Stage 3 (Capture)` $\to$ `Stage 4 (Encode)` $\to$ `Stage 5 (Network TX)` $\to$ `Ring Release`.

### 38. DMA Buffer Lifecycle
`Slot Free` $\to$ `Allocated to Capture` $\to$ `DMA Transfer` $\to$ `TX Complete` $\to$ `Reclaimed to Free Pool`.

### 39. NIC TX Completion Polling
Core 3 polls the e1000 TX descriptor status bit `E1000_TXD_STAT_DD` in a lock-free loop without hardware interrupts.

### 40. GPU Frame Buffer Completion
GPU frame slots are recycled upon receiving the subsequent frame's VBLANK edge.

### 41. Compiler Error Recovery
Single-pass compiler returns structured `Result<(), &'static str>` with token offset and line number on the first syntax error.

### 42. Formal Specification of Bounded Loop Proofs
Every `@while(cond)` loop must mutate its condition variable monotonically (e.g. `$i += 1;`). Loops without progress are terminated by the 10,000-instruction VM limit.

### 43. Static WCET vs Measured Dynamic TSC Discrepancy
Static WCET provides a conservative upper bound. Dynamic TSC evaluates real-time latency at runtime. If dynamic latency exceeds `@within`, `!drop` fires immediately.

---

## 4. Canonical Production Scripts

### 4.1 Zero-Copy Stream Pipeline (`stream.pl`)
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

### 4.2 Latency Benchmark (`bench.pl`)
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

### 4.3 Adaptive Congestion Controller (`filter.pl`)
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
