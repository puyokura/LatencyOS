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

### Template 1: Zero-Copy Pipeline (`stream.pul`)
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

### Template 2: Bounded Iteration Benchmark (`bench.pul`)
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

### Template 3: Congestion Guard (`filter.pul`)
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
### Template 4: Command-Line Argument Echo (`echo.pul`)
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
[ERROR_CODE]: ERR_PX64_TIMEOUT_EXCEEDED / ERR_PX64_CONST_OUT_OF_BOUNDS / ERR_PX64_INVALID_OPCODE
[MESSAGE]: <Concise runtime violation description>
[FILE]: <Target file path>
[EXECUTION_DOMAIN]: px64 Real-Time Register Virtual Machine
[RUNTIME_FAULT_CATEGORY]: Wall-Clock Watchdog Deadline Violation / Constant Pool Access Violation / Invalid Opcode Execution Fault
[TIMEOUT_LIMIT]: 5,000,000 ns (5.0 ms wall-clock)
[ROOT_CAUSE]: <Precise runtime condition triggering fault>
[AI_REPAIR_HINT]: <Specific actionable repair instruction>
=============================================================================================
```

---

## 4. px64 v3 Binary Header & Instruction Set Layout

### 4.1 16-Byte Binary Header
```text
Offset  Type    Field             Description
0x00    [u8; 4] Magic             b"PX64" (0x50 0x58 0x36 0x34)
0x04    u16     Version           3 (PX64 v3)
0x06    u16     Code Length       Bytecode size in bytes
0x08    u16     String Pool Len   String table size in bytes
0x0A    u16     Const Pool Count  Count of 64-bit (8-byte) integer constants
0x0C    u16     Num Registers     20 (16 GPRs + 4 HW DMA slots)
0x0E    u16     Reserved          0x0000
```

### 4.2 Opcodes Reference Table
| Opcode (Hex) | Name | Format | Description | WCET (Hardware / QEMU) |
|---|---|---|---|---|
| `0x00` | `NOP` | `00 00 00 00` | No operation | 1 ns / 74 ns |
| `0x01` | `MOV_IMM` | `01 Rd imm16` | Move 16-bit unsigned immediate into `$reg` | 2 ns / 80 ns |
| `0x02` | `MOV_REG` | `02 Rd Rs1 00` | Copy `$rs1` into `$rd` | 2 ns / 80 ns |
| `0x03` | `MOV_STR` | `03 Rd off len` | Load string slice tagged descriptor | 3 ns / 82 ns |
| `0x04..0x08` | `ADD/SUB/MUL/DIV/MOD` | `Op Rd Rs1 Rs2` | Integer arithmetic `$rd = $rs1 op $rs2` | 3 ns / 85 ns |
| `0x09..0x0E` | `CMPEQ..CMPGE` | `Op Rd Rs1 Rs2` | Conditional comparison `$rd = ($rs1 op $rs2) ? 1 : 0` | 3 ns / 85 ns |
| `0x0F` | `JMP` | `0f 00 imm16` | Unconditional jump to byte offset `imm16` | 2 ns / 80 ns |
| `0x10..0x11` | `JZ / JNZ` | `Op Rd imm16` | Conditional branch on `$rd == 0` / `$rd != 0` | 3 ns / 82 ns |
| `0x12` | `CALL_NAT` | `12 Rd func Rs2` | Call native kernel hardware intrinsic | Intrinsics dependent |
| `0x13..0x15` | `WITHIN/DROP` | `Op Rd 00 00` | Temporal deadline budget guards | 5 ns / 85 ns |
| `0x16` | `HALT` | `16 00 00 00` | Terminate VM execution | 1 ns / 74 ns |
| `0x17` | `LDC` | `17 Rd const_idx` | Load 64-bit constant from constant pool (`i64`) | **5 ns / 98 ns** |
| `0x18` | `ADDI` | `18 Rd Rs1 imm8` | Add 8-bit unsigned immediate `$rd = $rs1 + imm8` | **3 ns / 89 ns** |
| `0x19` | `SUBI` | `19 Rd Rs1 imm8` | Subtract 8-bit unsigned immediate `$rd = $rs1 - imm8` | **3 ns / 89 ns** |

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

