# PulseLang Formal Specification & Architecture Manual

> **Release Version**: `v0.0.40`  
> **Language Specification**: `PulseLang v3.2 (px64 v3 Architecture)`  
> **Host Kernel**: `LatencyOS (x86_64 freestanding no_std)`  
> **Compiler & Toolchain**: `pulc` (`pulc <file.pul>`, `compile`, `disasm`, `check`, `run`)  
> **Core Crate**: `pulselang-core` (`no_std` zero-heap crate with optional `alloc`/`std` support)  
> **Official Extension**: `.pul` (Compiled Bytecode: `.bin`)  

---

## 1. Overview & Design Philosophy

PulseLang is an AI-Native, temporal-first reactive domain-specific language (DSL) designed for **hard real-time streaming pipelines** on LatencyOS.

### Five Invariants
1. **Zero Dynamic Allocation**: 100% pre-allocated static token buffers, fixed-size AST-free single-pass bytecode generation, and zero runtime heap allocations.
2. **Guaranteed Bounded Execution**: Every loop is statically bounded or watchdog-monitored; maximum execution bound of 10,000 instructions and 5.0ms wall-clock hard limit.
3. **Strict Linear Ownership**: Hardware DMA handles (`#f0`..`#f3`) enforce strict single-consumption semantics.
4. **Strict Mutability Guard**: Variables declared with `let` are immutable by default; mutation requires `let mut`.
5. **Exact Prefix Taxonomy**: Local variables (`$`), hardware handles (`#`), and compiler directives/intrinsics (`@`).

---

## 2. Syntax & Language Semantics

### 2.1 Variables & Mutability
```pulse
let $immutable = 10;
let mut $counter = 0;
$counter += 1;
```

### 2.2 Data Types
| Type | Syntax Example | Storage Representation |
|---|---|---|
| **64-bit Integer** | `let $x = 42;` | `px64` Virtual Register (`$rax`..`$r15`) |
| **Time Literal** | `500us`, `10ms` | Nanosecond integer (`500_000`) |
| **Fixed Array** | `let $a = [1, 2, 3];` | Static Array Slot Bank (`array_slots[256]`) |
| **Inline String** | `let $s = "READY";` | Tagged Pointer in String Pool (`STR_TAG`) |
| **Hardware Handle**| `#f := @capture();` | Linear Descriptor Slot (`16`..`19`) |
| **Tagged Result** | `@ok(10)`, `@err(404)` | Bit-60 Tagged Result (`ERR_TAG`) |

### 2.3 Control Flow
```pulse
// 1. Conditionals (Strictly block-delimited)
if ($rtt < 100us) {
    @rate(100);
} else {
    @rate(80);
}

// 2. Bounded Static For Loop
for $i in 0..10 {
    $sum += $i;
}

// 3. Watchdog-Monitored While Loop
while ($x > 0) {
    $x -= 1;
}

// 4. Exhaustive Pattern Matching
match $res {
    @ok($val) => { @println($val); },
    @err($err) => { @println("Error occurred"); },
    _ => { @println("Default"); },
}
```

### 2.4 Static Functions, Structs & Tables
```pulse
// Static Function
fn add($a, $b) -> $ret {
    return $a + $b;
}

// Static Struct
struct Point { x: i64, y: i64 }
let mut $pt = Point { x: 10, y: 20 };
$pt.x := 30;

// Const Lookup Table
const LUT: [i64; 4] = [0, 64, 128, 255];
let $val = LUT[2];
```

### 2.5 AI-Native Declarative Combinators & Stride Views
```pulse
// Stride Views
let $row_i = @row($mat_a, $i, 3);
let $col_j = @col($mat_b, $j, 3);

// Declarative Loop Fusion: Zip, Map, and Sum Reduction
let $dot = @zip_with($row_i, $col_j, mul) |> @sum();
```

---

## 3. Hardware Intrinsics Catalog (29 Intrinsics)

| Category | Intrinsics | Description |
|---|---|---|
| **Telemetry & System** | `@core_id()`, `@tsc_freq()`, `@uptime_ns()`, `@busy_wait($ns)`, `@ring_depth($id)`, `@tsc()`, `@argc()`, `@arg($i)` | Hardware timer, LAPIC ID, and system telemetry. |
| **Math & Bitwise** | `@min($a, $b)`, `@max($a, $b)`, `@abs($a)`, `@clamp($v, $min, $max)`, `@popcnt($v)`, `@lzcnt($v)`, `@crc32($s, $v)` | Branchless math and hardware bit manipulation. |
| **VRAM & DMA** | `@vram_read($slot, $offset)`, `@vram_write($slot, $offset, $val)` | Zero-copy VRAM direct memory access. |
| **Result Handling** | `@ok($v)`, `@err($c)`, `@is_ok($r)`, `@is_err($r)`, `@unwrap($r)` | Tagged Result constructors and checkers. |
| **Streaming Pipeline** | `@capture()`, `@send(#f)`, `@rtt()`, `@rate($pct)` | VBLANK GPU frame capture and NIC transmission. |
| **Console Output** | `@print($v)`, `@println($v)` | Serial and terminal logging. |

---

## 4. `px64` v3 Instruction Set Architecture (ISA)

- **Header Layout (16 Bytes)**: Magic (`PX64`), Version (`3`), `code_len`, `str_pool_len`, `const_pool_len`, `num_registers` (20), `reserved`.
- **Register Map (20 Registers)**:
  - 16 General Purpose Virtual Registers: `$rax` (0) .. `$r15` (15).
  - 4 Linear Hardware DMA Slots: `#f0` (16) .. `#f3` (19).
- **Instruction Encoding**: 32-bit (4-byte) fixed-length instructions (`[Opcode, Rd, Rs1, Rs2]` or `[Opcode, Rd, Imm_hi, Imm_lo]`).

---

## 5. Toolchain CLI (`pulc`)

```bash
# Direct compile to .bin
pulc <file.pul> [-o <out.bin>]

# Execute in host VM
pulc run <file.bin|file.pul> [args...]

# Static syntax, type & WCET check
pulc check <file.pul>

# Disassemble bytecode binary
pulc disasm <file.bin>
```
