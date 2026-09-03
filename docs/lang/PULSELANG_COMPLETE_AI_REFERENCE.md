# PulseLang & px64 Complete Monolithic AI Reference & System Architecture Specification

> **Document Type**: Exhaustive All-in-One AI Specification, Formal Semantics, Instruction Set Architecture (ISA) & Low-Level Architectural Reference  
> **Target Audience**: AI Coding Assistants, LLMs, Static Analyzers, Formal Verification Engines, Kernel Engineers  
> **Release Version**: `v0.0.40`  
> **Language Specification**: `PulseLang v3.2 (px64 v3 Architecture)`  
> **Host Kernel**: `LatencyOS (x86_64 freestanding no_std)`  
> **Official File Extension**: Source: `.pul` | Compiled Bytecode: `.bin`  
> **Shared Core Library**: `pulselang-core` (`no_std` zero-heap crate with optional `alloc`/`std` features)  
> **Host Compiler Toolchain**: `pulc` (`pulc <file.pul>`, `pulc run`, `pulc compile`, `pulc check`, `pulc disasm`, `--json`)  
> **Zero-Dependency Guarantee**: This single document defines the complete syntax, type system, linear logic, 29 hardware intrinsics, 43 px64 opcodes, memory ordering, DMA coherency, and 43 formal architectural contracts.

---

## Table of Contents

1. [AI System Prompt, Core Tenets & Five Invariants](#1-ai-system-prompt-core-tenets--five-invariants)
2. [Complete Formal Grammar (EBNF)](#2-complete-formal-grammar-ebnf)
3. [Comprehensive Language Semantics & Syntax Guide](#3-comprehensive-language-semantics--syntax-guide)
   - 3.1 [Variables, Mutability & Scope (`let`, `let mut`)](#31-variables-mutability--scope-let-let-mut)
   - 3.2 [Data Types & Value Representations](#32-data-types--value-representations)
   - 3.3 [Fixed-Size Static Arrays (`[i64; N]`)](#33-fixed-size-static-arrays-i64-n)
   - 3.4 [Static Structs & Field Manipulation](#34-static-structs--field-manipulation)
   - 3.5 [Constant Lookup Tables (`const LUT = [...]`)](#35-constant-lookup-tables-const-lut---)
   - 3.6 [Inline Strings & String Equality](#36-inline-strings--string-equality)
   - 3.7 [Arithmetic, Bitwise, Shift & Relational Operators](#37-arithmetic-bitwise-shift--relational-operators)
   - 3.8 [Tagged Results & Robust Error Handling](#38-tagged-results--robust-error-handling)
   - 3.9 [Control Flow: Conditionals, Bounded Loops & Deadlines](#39-control-flow-conditionals-bounded-loops--deadlines)
   - 3.10 [Pattern Matching (`match`)](#310-pattern-matching-match)
   - 3.11 [Static Functions & Call Semantics (`fn`)](#311-static-functions--call-semantics-fn)
   - 3.12 [Design-by-Contract & Formal Directives](#312-design-by-contract--formal-directives)
   - 3.13 [Linear Type Ownership & DMA Safety Proofs](#313-linear-type-ownership--dma-safety-proofs)
   - 3.14 [AI-Native Declarative Combinators & Stride Views](#314-ai-native-declarative-combinators--stride-views)
   - 4.1 [Telemetry & Real-Time System Intrinsics](#41-telemetry--real-time-system-intrinsics)
   - 4.2 [Mathematical & Bitwise Intrinsics](#42-mathematical--bitwise-intrinsics)
   - 4.3 [Zero-Copy VRAM & Framebuffer Intrinsics](#43-zero-copy-vram--framebuffer-intrinsics)
   - 4.4 [Result & Error Handling Intrinsics](#44-result--error-handling-intrinsics)
   - 4.5 [Hardware Zero-Copy Streaming Pipeline Intrinsics](#45-hardware-zero-copy-streaming-pipeline-intrinsics)
   - 4.6 [Console & Telemetry Output Intrinsics](#46-console--telemetry-output-intrinsics)
5. [Complete `px64` v3 ISA & Binary Format](#5-complete-px64-v3-isa--binary-format)
   - 5.1 [Architectural Invariants & Execution Limits](#51-architectural-invariants--execution-limits)
   - 5.2 [20-Register File Map](#52-20-register-file-map)
   - 5.3 [32-Bit Fixed Instruction Format & Encoding](#53-32-bit-fixed-instruction-format--encoding)
   - 5.4 [Complete Opcode Instruction Table (`0x00`..`0x2A`)](#54-complete-opcode-instruction-table-0x000x2a)
   - 5.5 [16-Byte Fixed Header & Binary Container Layout](#55-16-byte-fixed-header--binary-container-layout)
   - 5.6 [Disassembly & Assembly Representation](#56-disassembly--assembly-representation)
6. [The 43 Master Architectural & Semantic Contracts](#6-the-43-master-architectural--semantic-contracts)
7. [Toolchain & Developer Experience](#7-toolchain--developer-experience)
   - 7.1 [`pulc` Host Compiler CLI Reference](#71-pulc-host-compiler-cli-reference)
   - 7.2 [Structured JSON Diagnostic Protocol (`--json`)](#72-structured-json-diagnostic-protocol---json)
   - 7.3 [AI-Actionable Diagnostic Output Format](#73-ai-actionable-diagnostic-output-format)
   - 7.4 [Exhaustive Compiler & Runtime Error Catalog](#74-exhaustive-compiler--runtime-error-catalog)
   - 7.5 [PulseEditor In-Kernel Editor & Shortcut Bar](#75-pulseeditor-in-kernel-editor--shortcut-bar)
8. [Standard Production Script Templates (`.pul`)](#8-standard-production-script-templates-pul)
   - 8.1 [`stream.pul`: Zero-Copy GPU-to-NIC Pipeline](#81-streampul-zero-copy-gpu-to-nic-pipeline)
   - 8.2 [`bench.pul`: Latency & Realtime Math Benchmark](#82-benchpul-latency--realtime-math-benchmark)
   - 8.3 [`filter.pul`: Adaptive Congestion Guard](#83-filterpul-adaptive-congestion-guard)
   - 8.4 [`echo.pul`: CLI Argument Echo & String Formatter](#84-echopul-cli-argument-echo--string-formatter)
   - 8.5 [`math_demo.pul`: Hardware Bit & Math Demonstration](#85-math_demopul-hardware-bit--math-demonstration)
   - 8.6 [`telemetry_ext.pul`: Extended Multi-Core Telemetry](#86-telemetry_extpul-extended-multi-core-telemetry)
   - 8.7 [`vram_test.pul`: Direct Zero-Copy VRAM Framebuffer Test](#87-vram_testpul-direct-zero-copy-vram-framebuffer-test)
   - 8.8 [`fn_test.pul`: Static Function Calling & Contract Validation](#88-fn_testpul-static-function-calling--contract-validation)
   - 8.9 [`struct_test.pul`: Static Struct Manipulation](#89-struct_testpul-static-struct-manipulation)
   - 8.10 [`match_test.pul`: Tagged Result Pattern Matching](#810-match_testpul-tagged-result-pattern-matching)
   - 8.11 [`fizzbuzz.pul`: Multi-Branch Conditionals & Nested If-Else](#811-fizzbuzzpul-multi-branch-conditionals--nested-if-else)

---

## 1. AI System Prompt, Core Tenets & Five Invariants

When generating, compiling, verifying, or refactoring PulseLang v3.1 code, an AI agent **MUST ALWAYS** follow these fundamental principles and five immutable invariants:

```
================================================================================
                    PULSELANG v3.1 AI GENERATION RULES
================================================================================
1. Zero Dynamic Allocation: Everything is statically preallocated. No malloc/Box.
2. Guaranteed Bounded WCET: All loops must be provably bounded; execution steps <= 10,000.
3. Strict Linear Ownership: DMA handles (#f) must be consumed exactly once.
4. Strict Mutability Enforcement: Variables declared with 'let' are immutable.
5. Exact Prefix Taxonomy: Variables ($), Linear Handles (#), Intrinsics/Directives (@).
================================================================================
```

### The Five Invariants:

1. **Prefix Taxonomy (Zero Ambiguity)**:
   - **`$`** for all Variables and function parameters (`$rtt`, `$sum`, `$i`, `$t0`, `$dt`, `$pt`).
   - **`#`** for all Linear Hardware/DMA Buffer Handles (`#f0`, `#f1`, `#f2`, `#f3`, `#frame`).
   - **`@`** for all Contracts, Formal Directives, and Built-in Intrinsics (`@contract`, `@pipeline`, `@within`, `@tsc()`, `@ok()`, `@err()`, `@vram_read()`, `@popcnt()`, `@println()`).

2. **Linear Type Single Consumption Proof**:
   - Every hardware descriptor acquired via `#f := @capture();` **MUST be consumed exactly once** on every possible execution branch (e.g. via `@send(#f);`).
   - Handles cannot be duplicated, leaked, dropped implicitly, or double-freed.

3. **Mandatory Explicit Time Units**:
   - Time constants **MUST** include valid unit suffixes: `ns` (nanoseconds), `us` (microseconds), `ms` (milliseconds), `s` (seconds).
   - Time literals are folded at compile time into 64-bit unsigned integer nanoseconds (`500us` $\to$ `500_000`, `2ms` $\to$ `2_000_000`).

4. **Mandatory Statement Semicolons & Mutability Rules**:
   - Every statement **MUST** terminate with a semicolon `;`.
   - Immutable variables (`let $x = 10;`) cannot be reassigned. Attempting mutation causes `ERR_MUTABILITY_VIOLATION`. Mutable variables must be explicitly declared as `let mut $x = 0;`.

5. **Static Bounded Execution & Watchdog Guarantee**:
   - No dynamic recursion beyond a call depth of 8 frames (`MAX_CALL_DEPTH = 8`).
   - Pure arithmetic loops are bounded by the 10,000 instruction step limit (`MAX_VM_STEPS`).
   - Wall-clock execution is monitored by an invariant TSC hardware watchdog set to 5.0 ms (`MAX_SCRIPT_TIMEOUT_NS = 5_000_000`).

---

## 2. Complete Formal Grammar (EBNF)

```ebnf
Script          ::= TopLevelDecl* <EOF>

TopLevelDecl    ::= ContractDecl
                  | PipelineDecl
                  | OnVblankDecl
                  | StructDefStmt
                  | ConstTableStmt
                  | FnDeclStmt
                  | Statement

ContractDecl    ::= "@contract:" ("@wcet(" TimeLiteral ")")? ("@budget(" TimeLiteral ")")? ( RelationalExpr )? ";"
PipelineDecl    ::= "@pipeline:" Identifier ("@budget(" TimeLiteral ")")? (";" | Block)
OnVblankDecl    ::= "@on_vblank:" Block ";"?

Statement       ::= LetDeclStmt
                  | AssignStmt
                  | CompoundAssign
                  | ArrayDeclStmt
                  | ArrayAssignStmt
                  | StructDefStmt
                  | StructDeclStmt
                  | StructAssignStmt
                  | ConstTableStmt
                  | MatchStmt
                  | FnDeclStmt
                  | ReturnStmt
                  | AssertStmt
                  | WithinStmt
                  | WhileStmt
                  | ForStmt
                  | IfStmt
                  | ExprStmt
                  | Block

LetDeclStmt     ::= "let" "mut"? VarIdent ( ":" TypeSpec )? ( "=" ( StructInitExpr | ArrayInitExpr | Expression ) )? ";"
TypeSpec        ::= Identifier | "[" "i64" ";" IntegerLiteral "]"

AssignStmt      ::= ( VarIdent | HardwareIdent ) ( ":=" | "=" ) Expression ";"
CompoundAssign  ::= ( VarIdent | HardwareIdent ) ( "+=" | "-=" ) Expression ";"

ArrayDeclStmt   ::= "let" VarIdent ":" "[" "i64" ";" IntegerLiteral "]" ";"
ArrayAssignStmt ::= VarIdent "[" Expression "]" ( ":=" | "=" ) Expression ";"
ArrayInitExpr   ::= "[" ( Expression ( "," Expression )* )? "]"

StructDefStmt   ::= "struct" Identifier "{" StructFieldList? "}" ";"?
StructFieldList ::= StructField ( "," StructField )* ","?
StructField     ::= Identifier ( ":" Identifier )?
StructDeclStmt  ::= "let" VarIdent ":" Identifier ";"
StructInitExpr  ::= Identifier "{" ( StructFieldInit ( "," StructFieldInit )* ","? )? "}"
StructFieldInit ::= Identifier ":" Expression
StructAssignStmt::= VarIdent "." Identifier ( ":=" | "=" ) Expression ";"

ConstTableStmt  ::= "const" Identifier ( ":" "[" "i64" ";" IntegerLiteral "]" )? "=" "[" ConstElemList? "]" ";"
ConstElemList   ::= ( IntegerLiteral | TimeLiteral ) ( "," ( IntegerLiteral | TimeLiteral ) )* ","?

FnDeclStmt      ::= "fn" Identifier "(" ParamList? ")" ( "->" VarIdent )? ( "@requires(" Expression ")" )* Block
ParamList       ::= VarIdent ( "," VarIdent )*
ReturnStmt      ::= "return" Expression? ";"

MatchStmt       ::= "match" Expression "{" MatchArm+ "}" ";"?
MatchArm        ::= Pattern "=>" ( Block | Statement ) ","?
Pattern         ::= "Ok(" VarIdent ")"
                  | "Err(" VarIdent ")"
                  | "@ok(" VarIdent ")"
                  | "@err(" VarIdent ")"
                  | "_"
                  | Expression

AssertStmt      ::= "@assert(" Expression ")" ";"
WithinStmt      ::= "@within(" TimeLiteral ")" Block ("!drop")? ";"
WhileStmt       ::= ( "@while" | "while" ) "(" Expression ")" Block
ForStmt         ::= ( "for" | "@for" ) VarIdent "in" Expression ".." Expression Block
IfStmt          ::= "if" "(" Expression ")" Block ( "else" ( IfStmt | Block ) )?
ExprStmt        ::= Expression ";"

Block           ::= "{" Statement* "}"

Expression      ::= PipeExpr
PipeExpr        ::= TernaryExpr ( "|>" TernaryExpr )*
TernaryExpr     ::= LogicOrExpr ( "?" ( Block | Expression ) ":" ( Block | Expression ) )?
LogicOrExpr     ::= LogicAndExpr ( "||" LogicAndExpr )*
LogicAndExpr    ::= BitwiseOrExpr ( "&&" BitwiseOrExpr )*
BitwiseOrExpr   ::= BitwiseXorExpr ( "|" BitwiseXorExpr )*
BitwiseXorExpr  ::= BitwiseAndExpr ( "^" BitwiseAndExpr )*
BitwiseAndExpr  ::= EqualityExpr ( "&" EqualityExpr )*
EqualityExpr    ::= RelationalExpr ( ( "==" | "!=" ) RelationalExpr )*
RelationalExpr  ::= ShiftExpr ( ( "<" | "<=" | ">" | ">=" ) ShiftExpr )*
ShiftExpr       ::= AdditiveExpr ( ( "<<" | ">>" ) AdditiveExpr )*
AdditiveExpr    ::= Multiplicative ( ( "+" | "-" ) Multiplicative )*
Multiplicative  ::= UnaryExpr ( ( "*" | "/" | "%" ) UnaryExpr )*
UnaryExpr       ::= ( "!" | "-" )? PrimaryExpr

PrimaryExpr     ::= IntegerLiteral
                  | TimeLiteral
                  | StringLiteral
                  | VarIdent "[" Expression "]"
                  | Identifier "[" Expression "]"
                  | VarIdent "." Identifier
                  | VarIdent
                  | HardwareIdent
                  | Identifier "(" ArgList? ")"
                  | IntrinsicCall
                  | "(" Expression ")"

IntrinsicCall   ::= ( "@core_id" | "@tsc_freq" | "@uptime_ns" | "@busy_wait" | "@ring_depth"
                    | "@tsc" | "@argc" | "@arg"
                    | "@min" | "@max" | "@abs" | "@clamp" | "@popcnt" | "@lzcnt" | "@crc32"
                    | "@vram_read" | "@vram_write"
                    | "@ok" | "@err" | "@is_ok" | "@is_err" | "@unwrap" | "@streq"
                    | "@capture" | "@send" | "@rtt" | "@rate"
                    | "@print" | "@println" ) "(" ArgList? ")"

ArgList         ::= Expression ( "," Expression )*

IntegerLiteral  ::= [0-9]+ | "0x" [0-9a-fA-F]+ | "0b" [01]+
TimeLiteral     ::= [0-9]+ ("ns" | "us" | "ms" | "s")
StringLiteral   ::= '"' [^"]* '"'
VarIdent        ::= "$" [a-zA-Z0-9_]+
HardwareIdent   ::= "#" [a-zA-Z0-9_]+
Identifier      ::= [a-zA-Z_] [a-zA-Z0-9_]*
```

---

## 3. Comprehensive Language Semantics & Syntax Guide

### 3.1 Variables, Mutability & Scope (`let`, `let mut`)

PulseLang v3.1 enforces strict static single assignment or explicit mutability declaration. Variable slots are mapped directly to the `px64` register file (`$rax` through `$r15`).

```pulse
// 1. Immutable variable declaration (Default)
let $x = 42;
let $t_limit = 500us;

// 2. Mutable variable declaration
let mut $counter = 0;
$counter += 1;
$counter = $counter * 2;
$counter := 100; // Walrus assignment is also valid

// 3. Mutability Violation Example:
// let $fixed = 10;
// $fixed = 20; // COMPILE ERROR: ERR_MUTABILITY_VIOLATION
```

### 3.2 Data Types & Value Representations

All basic runtime values in PulseLang are 64-bit signed/unsigned words (`i64` / `u64`). High bits are reserved for tagged representations:

| Type | Syntax Example | In-Memory Representation | Semantic Notes |
|---|---|---|---|
| **Integer** | `100`, `0xFF`, `-50` | `i64` two's complement | Wrapping arithmetic, 0-safe division. |
| **Time Constant** | `500us`, `10ms`, `2s` | `u64` nanoseconds | Auto-folded at compile-time to nanoseconds. |
| **Inline String** | `"Hello, LatencyOS"` | Tagged Pointer: `0x4000_...` | Points into static 512-byte string pool. |
| **CLI Argument** | `@arg(0)` | Tagged Pointer: `0x2000_...` | References kernel CLI argument buffer. |
| **Hardware Handle** | `#f0 := @capture();` | Descriptor Slot (`16`..`19`) | Linear type, strict single consumption. |
| **Tagged Result** | `@ok(10)`, `@err(404)` | Bit 60 Tagged: `0x1000_...` | Zero-allocation Ok/Err tagged status. |

### 3.3 Fixed-Size Static Arrays (`[i64; N]`)

Fixed-size static arrays provide deterministic, hardware bounds-checked $O(1)$ memory access using kernel/VM static array storage (up to 8 distinct arrays, max 256 total elements across the script).

> **Memory & Register Model**:
> - **Zero GPR Exhaustion**: Arrays do **not** consume general-purpose variable registers ($rax..$r15). They live in the dedicated static array slot bank (`array_slots[256]`). This allows storing large datasets and matrices without hitting the 13 GPR distinct-variable limit.
> - **Hardware Bounds Guard**: Every read (`ARR_LOAD`) and write (`ARR_STORE`) verifies $0 \le \text{index} < N$. Out-of-bounds access immediately raises `ERR_PX64_ARRAY_OUT_OF_BOUNDS`.
> - **Three Declaration & Initialization Modes**:
>   1. **Uninitialized (Zero-Filled)**: `let $a: [i64; 9];` or `let mut $a: [i64; 9];`
>   2. **Typed Initialized**: `let $a: [i64; 9] = [1, 2, 3, 4, 5, 6, 7, 8, 9];`
>   3. **Direct Array Literal**: `let $a = [1, 2, 3, 4, 5, 6, 7, 8, 9];`

```pulse
// 1. Array declaration styles
let $zeros: [i64; 4];                            // 4 elements, initialized to 0
let $typed: [i64; 3] = [10, 20, 30];            // Typed with inline elements
let $matrix = [                                 // Inferred size (9 elements)
    1, 2, 3,
    4, 5, 6,
    7, 8, 9
];

// 2. Array Element Mutation & Loading
let mut $out: [i64; 9];
$out[0] := 100;
let $val = $matrix[4];                          // Loads 5

// 3. Multi-Dimensional 3x3 Matrix Multiplication Example
let $mat_a = [1, 2, 3, 4, 5, 6, 7, 8, 9];
let $mat_b = [9, 8, 7, 6, 5, 4, 3, 2, 1];
let mut $mat_c: [i64; 9];

for $i in 0..3 {
    for $j in 0..3 {
        let mut $sum = 0;
        for $k in 0..3 {
            let $a_idx = ($i * 3) + $k;
            let $b_idx = ($k * 3) + $j;
            $sum += $mat_a[$a_idx] * $mat_b[$b_idx];
        }
        let $c_idx = ($i * 3) + $j;
        $mat_c[$c_idx] := $sum;
    }
}
```
### 3.4 Static Structs & Field Manipulation

Static Structs allow composite data structures without heap allocation. Struct types and field offsets are verified at compile time (up to 8 struct types, 8 fields each, 8 active instances):

```pulse
// 1. Struct Definition
struct Point {
    x: i64,
    y: i64,
}

struct TelemetryPacket {
    timestamp,
    rtt,
    status
}

// 2. Struct Instantiation
let mut $pt = Point { x: 100, y: 200 };
let $packet: TelemetryPacket;

// 3. Field Access & Mutation
$pt.x := 150;
$pt.y += 50;

let $dist = $pt.x + $pt.y;
@println($dist);
```

### 3.5 Constant Lookup Tables (`const LUT = [...]`)

Constant lookup tables embed immutable arrays directly into the `px64` 64-bit constant pool (`PX64_OP_TBL_DEF` & `PX64_OP_TBL_LOAD`):

```pulse
const SINE_LUT: [i64; 4] = [0, 707, 1000, 707];
const RATES = [10, 25, 50, 100];

let $idx = 2;
let $val = SINE_LUT[$idx]; // O(1) table load: 1000
@println($val);
```

### 3.6 Inline Strings & String Equality

Strings are read-only literals placed into the static string pool (`MAX_STRING_POOL = 512` bytes). String comparisons evaluate with bounded $O(1)$ execution time via `PX64_OP_STREQ` (0x2A):

```pulse
let $s1 = "STREAM_READY";
let $s2 = "STREAM_READY";

if ($s1 == $s2) {
    @println("[STATUS] Identical string signature confirmed.");
}

let $arg = @arg(0);
if ($arg == "bench") {
    @println("[MODE] Running benchmark suite.");
}
```

### 3.7 Arithmetic, Bitwise, Shift & Relational Operators

All operations execute in constant time ($\le 3$ ns, division/modulo $\le 12$ ns):

```pulse
let $a = 0xF0;
let $b = 0x0F;

// Bitwise operations
let $and_val = $a & $b;  // 0x00 (AND)
let $or_val  = $a | $b;  // 0xFF (OR)
let $xor_val = $a ^ $b;  // 0xFF (XOR)
let $shl_val = 1 << 4;   // 16   (SHL)
let $shr_val = $a >> 2;  // 0x3C (SHR)

// Arithmetic operations (Wrapping two's complement)
let $sum = 10 + 20;
let $diff = 50 - 15;
let $prod = 8 * 9;
let $div = 100 / 0;      // Protected: returns 0 without CPU trap
let $mod = 100 % 0;      // Protected: returns 0 without CPU trap

// Relational operations (1 = true, 0 = false)
let $is_eq = (10 == 10); // 1
let $is_lt = (5 < 10);   // 1
```

### 3.8 Tagged Results & Robust Error Handling

PulseLang incorporates zero-overhead Result types using bit 60 tagging (`ERR_TAG = 0x1000_0000_0000_0000`):

```pulse
fn compute_ratio($numerator, $denominator) {
    if ($denominator == 0) {
        return @err(400); // Return error tagged value
    }
    return @ok($numerator / $denominator); // Return Ok tagged value
}

let $res = compute_ratio(100, 5);
if (@is_ok($res)) {
    let $val = @unwrap($res);
    @println($val);
}
```

### 3.9 Control Flow: Conditionals, Bounded Loops & Deadlines

#### 1. Conditionals (`if` / `else if` / `else`)

PulseLang v3.2 fully supports `if`, `else if` chaining, and `else` branches. An `if` statement requires parentheses around its condition, followed by a block enclosed in curly braces `{ ... }`. `else if` branches are automatically desugared by the compiler into structured nested blocks with zero runtime overhead.

**Standard If / Else If / Else Chaining:**
```pulse
let $x = 2;
let mut $res = 0;

if ($x == 1) {
    $res := 10;
} else if ($x == 2) {
    $res := 20;
} else if ($x == 3) {
    $res := 30;
} else {
    $res := 99;
}
```

**Multi-Branch FizzBuzz Recipe (1-Level Nesting with `else if`):**
```pulse
// fizzbuzz.pul - Real-Time Multi-Branch FizzBuzz Recipe
@contract: @wcet(10us) @budget(100us);

for $i in 1..16 {
    if (($i % 15) == 0) {
        @println("FizzBuzz");
    } else if (($i % 3) == 0) {
        @println("Fizz");
    } else if (($i % 5) == 0) {
        @println("Buzz");
    } else {
        @println($i);
    }
}
```
#### 2. Ternary Operator & Ternary Blocks
```pulse
let $rtt = @rtt();

// Simple ternary expression
let $rate = ($rtt < 200us) ? 100 : 60;

// Multi-statement ternary block
$rtt > 300us ? {
    @println("[ALERT] Congestion detected!");
    @rate(50);
} : {
    @rate(100);
};
```

#### 3. Statically Bounded For Loop (Proven WCET)
```pulse
let mut $acc = 0;
for $i in 0..10 {
    $acc += $i * 2;
}
```

#### 4. While Loop (Watchdog & Step Bounded)
```pulse
let mut $k = 0;
while ($k < 50) {
    $k += 1;
}
```

#### 5. Temporal Deadline Block with Automatic Overrun Reclaim
```pulse
@within(500us) {
    #f := @capture();
    @send(#f);
} !drop;
```

### 3.10 Pattern Matching (`match`)

Pattern matching supports Tagged Results (`Ok($v)` / `@ok($v)`, `Err($e)` / `@err($e)`), numeric literals, and wildcard fallback (`_`):

```pulse
let $res = @ok(42);

match $res {
    Ok($val) => {
        @print("[RESULT OK]: ");
        @println($val);
    },
    Err($code) => {
        @print("[RESULT ERROR CODE]: ");
        @println($code);
    },
    0 => {
        @println("[ZERO]");
    },
    _ => {
        @println("[UNKNOWN]");
    }
};
```

### 3.11 Static Functions & Call Semantics (`fn`)

Functions execute on the static 8-frame call stack (`PX64_OP_CALL` / `PX64_OP_RET`). Parameters are passed via dedicated register bindings, and the return value is returned in `$rax`:

```pulse
// Function declaration with Design-by-Contract precondition
fn clamp_rate($input_rate, $min_bound, $max_bound) @requires($min_bound <= $max_bound) {
    if ($input_rate < $min_bound) {
        return $min_bound;
    }
    if ($input_rate > $max_bound) {
        return $max_bound;
    }
    return $input_rate;
}

// Invocation
let $safe_rate = clamp_rate(120, 10, 100); // Returns 100
@println($safe_rate);
```

### 3.12 Design-by-Contract & Static WCET Bound Verification

PulseLang enforces deterministic execution timing using a dual-layer strategy: compile-time static step estimation and runtime watchdog caps.

#### 1. Compile-Time Static WCET Step Bound Verification
For statically bounded `for $var in start..end` loops, the compiler mathematically calculates the exact worst-case instruction steps before emitting bytecode:

- **Implementation Reference**: `Compiler::statement` in `pulselang-core/src/compiler.rs:1833-1848`.
- **Calculation Model**:
  $$\text{Body Instructions} = \frac{\text{body\_code\_end} - \text{body\_code\_start}}{4}$$
  $$\text{Instructions Per Iteration} = \text{Body Instructions} + 4 \quad (\text{CMP\_LT} + \text{JZ} + \text{ADDI} + \text{JMP})$$
  $$\text{Total Estimated Steps} = \text{Instructions Per Iteration} \times \text{Iterations}$$
- **Static Rejection Rule**: If $\text{Total Estimated Steps} > \text{MAX\_VM\_STEPS}\ (10,000\ \text{steps})$, the compiler immediately rejects the script with `ERR_FOR_WCET_EXCEEDED`.

#### 2. Static Loop Bound Verification for `while`
- **Implementation Reference**: `Compiler::statement` in `pulselang-core/src/compiler.rs:1659-1671`.
- Constant infinite loops (e.g. `@while(1)`) are rejected at compile time with `ERR_UNBOUNDED_LOOP`.

#### 3. Dual Runtime Step & Hardware TSC Watchdog Caps
- **Implementation Reference**: `PX64VM::run` in `pulselang-core/src/vm.rs:245-255` and `kernel/src/lang.rs:595-605`.
- **Step Cap**: Execution is forcibly terminated with `ERR_PX64_WCET_EXCEEDED` if $\text{steps} \ge 10,000$.
- **Wall-Clock Cap**: Every 8 instructions, the hardware serialized TSC (`read_tsc_serialized()`) is checked. Execution exceeding 5.0ms (5,000,000 ns) is terminated with `ERR_PX64_TIMEOUT_EXCEEDED`.

```pulse
// Top-Level Script Contract
@contract: @wcet(5us) @budget(50us);

// Pipeline Declaration with Glass-to-Glass Latency Budget
@pipeline: CameraToNetworkStream @budget(8000us);

// Function Invariant Contract
fn process_sample($sample) @requires($sample >= 0) {
    @assert($sample < 65536); // Runtime invariant assertion
    return $sample * 2;
}
```
### 3.13 Linear Type Ownership & DMA Safety Proofs

Handles pointing to zero-copy GPU/NIC descriptors (`#f0`..`#f3`) enforce linear ownership. The compiler performs static flow analysis to guarantee single consumption:

```pulse
// VALID: Captured and consumed exactly once
#f := @capture();
@send(#f);

// INVALID (ERR_LINEAR_UNCONSUMED_HANDLE): Handle leaked without @send
// #f := @capture();

// INVALID (ERR_LINEAR_DOUBLE_SEND): Handle transmitted multiple times
// #f := @capture();
// @send(#f);
// @send(#f);

// INVALID (ERR_LINEAR_OVERWRITE): Overwriting handle before transmission
// #f := @capture();
// #f := @capture();
// @send(#f);
```

---

## 4. Exhaustive Intrinsics Catalog (All 29 Intrinsics)

Every intrinsic compiles directly to a specialized `PX64_OP_CALL_NAT` instruction (`0x12`), executing deterministically within the LatencyOS kernel with verified worst-case execution time bounds:

### 3.14 AI-Native Declarative Combinators & Stride Views

PulseLang v3.2 introduces first-class zero-allocation stream combinators and stride view intrinsics. Instead of writing imperative, error-prone 3-level nested loops with manual 1D indexing, AI agents can directly express mathematical intent (dot products, vector transformations, reductions) using declarative functional pipelines.

> **Key Architectural Guarantees**:
> - **Zero Heap Allocation**: No dynamic closures or intermediate vector arrays are allocated.
> - **Compile-Time Loop Fusion**: Chained combinators (e.g. `@zip_with(...) |> @sum()`) are statically fused into a single tightly-optimized `px64` loop with register accumulation.
> - **Static WCET & Shape Proof**: The compiler calculates the exact execution time and step count at compile time, rejecting shape mismatches before runtime.

#### 1. Stride View Intrinsics
| Intrinsic | Parameters | Description |
|---|---|---|
| `@row($arr, $i, $cols)` | Array, Row Index, Column Count | Creates a zero-copy row view of `$cols` elements starting at index `$i * $cols` with stride 1. |
| `@col($arr, $j, $cols)` | Array, Column Index, Column Count | Creates a zero-copy column view spanning the array with stride `$cols`. |
| `@slice($arr, $start, $len)` | Array, Start Index, Length | Creates a contiguous sub-array view of `$len` elements starting at `$start`. |

#### 2. Declarative Combinators
| Combinator | Signature | Description |
|---|---|---|
| `@zip_with($v1, $v2, fn)` | `(View, View, Function) -> Loop` | Applies binary function `fn` to corresponding elements from `$v1` and `$v2`. |
| `@sum($view)` or `\|> @sum()` | `(View) -> i64` | Computes the scalar sum of view elements using register accumulation. |
| `@reduce($view, $init, fn)` | `(View, Initial, Function) -> i64` | Reduces view elements into a single scalar value using accumulator function `fn`. |

#### 3. Complete Declarative 3x3 Matrix Multiplication Example
```pulse
// matrix_mul_v32.pul - AI-Native 3x3 Matrix Multiplication with Combinators
@contract: @wcet(25us) @budget(50us);

fn mul($x, $y) -> $ret {
    return $x * $y;
}

let $a = [
    1, 2, 3,
    4, 5, 6,
    7, 8, 9
];

let $b = [
    9, 8, 7,
    6, 5, 4,
    3, 2, 1
];

let mut $c: [i64; 9];

for $i in 0..3 {
    let $row_i = @row($a, $i, 3);

    for $j in 0..3 {
        let $col_j = @col($b, $j, 3);
        
        // Dot Product: zip row and column, apply 'mul', and compute sum
        let $dot = @zip_with($row_i, $col_j, mul) |> @sum();

        $c[($i * 3) + $j] := $dot;
    }
}

// Output results (30, 24, 18, 84, 69, 54, 138, 114, 90)
for $k in 0..9 {
    @println($c[$k]);
}
```
### 4.1 Telemetry & Real-Time System Intrinsics

| ID | Intrinsic | Signature | WCET | Description & Register Behavior |
|---|---|---|---|---|
| `3` | `@tsc()` | `() -> i64` | **~15 ns** | Reads hardware serialized CPU Time-Stamp Counter (`lfence; rdtsc`). |
| `8` | `@argc()` | `() -> i64` | **~5 ns** | Returns number of CLI arguments passed to script (0..8). |
| `9` | `@arg($idx)` | `(i64) -> Tagged` | **~5 ns** | Returns tagged pointer `0x2000_...` to $idx$-th CLI argument string. |
| `16` | `@core_id()` | `() -> i64` | **~10 ns** | Queries Local APIC ID of current executing CPU core (0..3). |
| `17` | `@tsc_freq()` | `() -> i64` | **~5 ns** | Returns calibrated CPU TSC frequency in megahertz (MHz). |
| `18` | `@uptime_ns()` | `() -> i64` | **~20 ns** | Returns elapsed nanoseconds since kernel boot derived from serialized TSC. |
| `19` | `@busy_wait($ns)` | `(i64) -> 0` | **$\approx \$ns$** | Performs cycle-accurate spin loop for exactly `$ns` nanoseconds. |

| `20` | `@ring_depth($id)`| `(i64) -> i64` | **~10 ns** | Queries depth of lock-free ring (`0`: Capture-to-Encode, `1`: Encode-to-Net). |

```pulse
let $core = @core_id();
let $freq = @tsc_freq();
let $uptime = @uptime_ns();
let $depth = @ring_depth(0);

@print("[SYSTEM] Core ID: ");
@println($core);
@print("[SYSTEM] Clock Frequency (MHz): ");
@println($freq);
```

### 4.2 Mathematical & Bitwise Intrinsics

| ID | Intrinsic | Signature | WCET | Description & Register Behavior |
|---|---|---|---|---|
| `21` | `@min($a, $b)` | `(i64, i64) -> i64` | **~4 ns** | Computes minimum of `$a` and `$b` using branchless conditional moves. |
| `22` | `@max($a, $b)` | `(i64, i64) -> i64` | **~4 ns** | Computes maximum of `$a` and `$b` using branchless conditional moves. |
| `23` | `@abs($a)` | `(i64) -> i64` | **~3 ns** | Computes absolute value with two's complement saturation. |
| `24` | `@clamp($v, $min, $max)` | `(i64, i64, i64) -> i64` | **~6 ns** | Clamps value `$v` within inclusive range `[$min, $max]`. |
| `25` | `@popcnt($v)` | `(i64) -> i64` | **~3 ns** | Hardware population count (number of set 1-bits) via CPU instruction. |
| `26` | `@lzcnt($v)` | `(i64) -> i64` | **~3 ns** | Hardware leading zero count via CPU instruction. |
| `27` | `@crc32($seed, $val)` | `(i64, i64) -> i64` | **~8 ns** | Computes IEEE 802.3 CRC32 checksum over 8-byte value with seed. |

```pulse
let $val = -450;
let $abs_val = @abs($val);              // 450
let $clamped = @clamp(120, 0, 100);     // 100
let $bits = @popcnt(0b1101_0011);       // 5
let $lz = @lzcnt(0x0000_0001);          // 63
let $crc = @crc32(0, 0x1234_5678_9ABC); // CRC32 hash
```

### 4.3 Zero-Copy VRAM & Framebuffer Intrinsics

| ID | Intrinsic | Signature | WCET | Description & Register Behavior |
|---|---|---|---|---|
| `28` | `@vram_read($slot, $offset)` | `(i64, i64) -> i64` | **~25 ns** | Reads 64-bit word from GPU zero-copy framebuffer slot at byte offset. |
| `29` | `@vram_write($slot, $offset, $val)` | `(i64, i64, i64) -> 0` | **~30 ns** | Writes 64-bit word into GPU zero-copy framebuffer slot with cache coherency. |

```pulse
// Read first 8 bytes of captured frame in slot 0
let $magic_header = @vram_read(0, 0);

// Write timestamp overlay into frame metadata region (offset 1024)
let $now = @tsc();
@vram_write(0, 1024, $now);
```

### 4.4 Result & Error Handling Intrinsics

| ID | Intrinsic | Signature | WCET | Description & Register Behavior |
|---|---|---|---|---|
| `10` | `@ok($val)` | `(i64) -> Tagged` | **~2 ns** | Wraps value in Ok result (strips `ERR_TAG` bit). |
| `11` | `@err($code)` | `(i64) -> Tagged` | **~2 ns** | Wraps error code in Err result (sets `ERR_TAG = 0x1000_0000_0000_0000`). |
| `12` | `@is_ok($res)` | `(Tagged) -> i64` | **~2 ns** | Returns `1` if result is Ok, `0` if Err. |
| `13` | `@is_err($res)`| `(Tagged) -> i64` | **~2 ns** | Returns `1` if result is Err, `0` if Ok. |
| `14` | `@unwrap($res)`| `(Tagged) -> i64` | **~3 ns** | Extracts payload from Ok result; triggers `ERR_PX64_UNWRAP_FAILED` if Err. |
| `15` | `@streq($s1, $s2)` | `(str, str) -> i64` | **~5 ns** | Constant-time string / CLI argument equality comparison. |

```pulse
let $res = @err(503);
if (@is_err($res)) {
    @print("[FAULT] Service unavailable code: ");
    @println($res & !0x1000_0000_0000_0000);
}
```

### 4.5 Hardware Zero-Copy Streaming Pipeline Intrinsics

| ID | Intrinsic | Signature | WCET | Description & Register Behavior |
|---|---|---|---|---|
| `4` | `@rtt()` | `() -> i64` | **~20 ns** | Queries active minimum network round-trip time in nanoseconds from NIC PMD. |
| `5` | `@rate($pct)` | `(i64) -> 0` | **~10 ns** | Sets UDP network streaming congestion throttle percentage (10%..100%). |
| `6` | `@capture()` | `() -> #handle` | **~100 ns** | Claims zero-copy GPU frame descriptor index synchronized to VBLANK edge. |
| `7` | `@send(#handle)` | `(#handle) -> 1` | **~200 ns** | Enqueues descriptor to Intel e1000 TX ring with kernel-bypass DMA `sfence`. |

```pulse
#f := @capture();
let $rtt = @rtt();
if ($rtt > 300us) {
    @rate(75);
} else {
    @rate(100);
}
@send(#f);
```

### 4.6 Console & Telemetry Output Intrinsics

| ID | Intrinsic | Signature | WCET | Description & Register Behavior |
|---|---|---|---|---|
| `1` | `@print($val)` | `(any) -> 0` | **~400 ns** | Emits string literal, tagged CLI argument, or integer to UART COM1 without newline. |
| `2` | `@println($val)` | `(any) -> 0` | **~500 ns** | Emits value followed by CRLF (`\r\n`) to UART COM1 port. |

```pulse
@print("LatencyOS Version: ");
@println(3);
```

---

## 5. Complete `px64` v3 ISA & Binary Format

### 5.1 Architectural Invariants & Execution Limits

1. **Fixed 32-bit (4-Byte) Instruction Alignment**: Instructions are decoded in $O(1)$ constant time without variable-length instruction decoding overhead.
2. **20-Register Flat Register File**: 16 General Purpose Registers (`$rax`..`$r15`) + 4 Dedicated Hardware DMA Handle Registers (`#f0`..`#f3`).
3. **Execution Limits**:
   - `MAX_VM_STEPS`: **10,000 instruction steps** (prevents infinite loops).
   - `MAX_SCRIPT_TIMEOUT_NS`: **5,000,000 ns (5.0 ms)** wall-clock watchdog limit.
   - `MAX_CALL_DEPTH`: **8 call stack frames**.
   - `MAX_BYTECODE_SIZE`: **1,024 bytes**.
   - `MAX_STRING_POOL`: **512 bytes**.
   - `MAX_CONST_POOL`: **64 entries (512 bytes)**.

### 5.2 20-Register File Map

| Index | Canonical Register | Alias / Hardware Slot | Architectural Function |
|---|---|---|---|
| `0` | `$rax` | `$r0` | Accumulator, Primary Expression Result, Function Return Value |
| `1` | `$rcx` | `$r1` | Counter, 1st User Variable Slot |
| `2` | `$rdx` | `$r2` | Data Register, 2nd User Variable Slot |
| `3` | `$rbx` | `$r3` | Base Register, 3rd User Variable Slot |
| `4` | `$rsp` | `$r4` | Stack Pointer Alias, 4th User Variable Slot |
| `5` | `$rbp` | `$r5` | Base Pointer Alias, 5th User Variable Slot |
| `6` | `$rsi` | `$r6` | Source Index, 6th User Variable Slot |
| `7` | `$rdi` | `$r7` | Destination Index, 7th User Variable Slot |
| `8` | `$r8` | `$r8` | 8th User Variable Slot |
| `9` | `$r9` | `$r9` | 9th User Variable Slot |
| `10` | `$r10` | `$r10` | 10th User Variable Slot |
| `11` | `$r11` | `$r11` | 11th User Variable Slot |
| `12` | `$r12` | `$r12` | 12th User Variable Slot |
| `13` | `$r13` | `$r13` | 13th User Variable Slot |
| `14` | `$r14` | `$r14` | Secondary Internal Calculation Scratch Register |
| `15` | `$r15` | `$r15` | Primary Internal Calculation Scratch Register |
| `16` | `#f0` | `#slot0` | Hardware Zero-Copy Frame Slot 0 Descriptor |
| `17` | `#f1` | `#slot1` | Hardware Zero-Copy Frame Slot 1 Descriptor |
| `18` | `#f2` | `#slot2` | Hardware Zero-Copy Frame Slot 2 Descriptor |
| `19` | `#f3` | `#slot3` | Hardware Zero-Copy Frame Slot 3 Descriptor |

### 5.3 32-Bit Fixed Instruction Format & Encoding

Every `px64` instruction occupies exactly 4 contiguous bytes:

```text
+----------------+----------------+----------------+----------------+
| Byte 0 (Opcode)| Byte 1 (Rd)    | Byte 2 (Rs1)   | Byte 3 (Rs2)   |
| [7:0]          | [7:0]          | [7:0] / Imm_hi | [7:0] / Imm_lo |
+----------------+----------------+----------------+----------------+
```

- **Byte 0 (`Opcode`)**: `PX64_OP_*` opcode identifier (`0x00`..`0x2A`).
- **Byte 1 (`Rd`)**: Destination register index (`0`..`19`).
- **Byte 2 (`Rs1`)**: First source register index (`0`..`19`) OR high byte of 16-bit immediate (`Imm[15:8]`).
- **Byte 3 (`Rs2`)**: Second source register index (`0`..`19`) OR low byte of 16-bit immediate (`Imm[7:0]`).

### 5.4 Complete Opcode Instruction Table (`0x00`..`0x2A`)

| Opcode | Mnemonic | Operands | Encoding Format | Formal Semantics & Operation | WCET |
|---|---|---|---|---|---|
| `0x00` | `NOP` | None | `00 00 00 00` | No operation | ~1 ns |
| `0x01` | `MOV` | `Rd, Imm16` | `01 Rd Ih Il` | `Rd = (Ih << 8) \| Il` | ~2 ns |
| `0x02` | `MOV` | `Rd, Rs1` | `02 Rd Rs 00` | `Rd = Rs1` | ~2 ns |
| `0x03` | `MOVS` | `Rd, Offset, Len` | `03 Rd Of Ln` | `Rd = STR_TAG \| (Of << 32) \| Ln` | ~3 ns |
| `0x04` | `ADD` | `Rd, Rs1, Rs2` | `04 Rd S1 S2` | `Rd = Rs1.wrapping_add(Rs2)` | ~2 ns |
| `0x05` | `SUB` | `Rd, Rs1, Rs2` | `05 Rd S1 S2` | `Rd = Rs1.wrapping_sub(Rs2)` | ~2 ns |
| `0x06` | `MUL` | `Rd, Rs1, Rs2` | `06 Rd S1 S2` | `Rd = Rs1.wrapping_mul(Rs2)` | ~3 ns |
| `0x07` | `DIV` | `Rd, Rs1, Rs2` | `07 Rd S1 S2` | `Rd = (Rs2 != 0) ? Rs1 / Rs2 : 0` (zero-safe) | ~12 ns |
| `0x08` | `MOD` | `Rd, Rs1, Rs2` | `08 Rd S1 S2` | `Rd = (Rs2 != 0) ? Rs1 % Rs2 : 0` (zero-safe) | ~12 ns |
| `0x09` | `CMPEQ` | `Rd, Rs1, Rs2` | `09 Rd S1 S2` | `Rd = (Rs1 == Rs2) ? 1 : 0` | ~2 ns |
| `0x0A` | `CMPNE` | `Rd, Rs1, Rs2` | `0a Rd S1 S2` | `Rd = (Rs1 != Rs2) ? 1 : 0` | ~2 ns |
| `0x0B` | `CMPLT` | `Rd, Rs1, Rs2` | `0b Rd S1 S2` | `Rd = (Rs1 < Rs2) ? 1 : 0` | ~2 ns |
| `0x0C` | `CMPLE` | `Rd, Rs1, Rs2` | `0c Rd S1 S2` | `Rd = (Rs1 <= Rs2) ? 1 : 0` | ~2 ns |
| `0x0D` | `CMPGT` | `Rd, Rs1, Rs2` | `0d Rd S1 S2` | `Rd = (Rs1 > Rs2) ? 1 : 0` | ~2 ns |
| `0x0E` | `CMPGE` | `Rd, Rs1, Rs2` | `0e Rd S1 S2` | `Rd = (Rs1 >= Rs2) ? 1 : 0` | ~2 ns |
| `0x0F` | `JMP` | `Target16` | `0f 00 Th Tl` | `IP = (Th << 8) \| Tl` | ~2 ns |
| `0x10` | `JZ` | `Rs1, Target16` | `10 Rs Th Tl` | `if Rs1 == 0 { IP = (Th << 8) \| Tl }` | ~3 ns |
| `0x11` | `JNZ` | `Rs1, Target16` | `11 Rs Th Tl` | `if Rs1 != 0 { IP = (Th << 8) \| Tl }` | ~3 ns |
| `0x12` | `CALL_NAT` | `Rd, FuncId, ArgReg` | `12 Rd Fn Ar` | `Rd = call_native(Fn, ArgReg)` | Varies |
| `0x13` | `WITHIN_START` | `Rs1` | `13 Rs 00 00` | Push deadline `TSC + ns_to_tsc(Rs1 * 1000)` | ~15 ns |
| `0x14` | `WITHIN_END` | None | `14 00 00 00` | Pop deadline stack | ~2 ns |
| `0x15` | `DROP` | None | `15 00 00 00` | If `TSC > deadline`, drop overdue frame | ~10 ns |
| `0x16` | `HALT` | None | `16 00 00 00` | Terminate VM execution | ~1 ns |
| `0x17` | `LDC` | `Rd, ConstIdx16` | `17 Rd Ch Cl` | `Rd = const_pool[(Ch << 8) \| Cl]` | ~2 ns |
| `0x18` | `ADDI` | `Rd, Rs1, Imm8` | `18 Rd Rs Im` | `Rd = Rs1.wrapping_add(Im as i64)` | ~2 ns |
| `0x19` | `SUBI` | `Rd, Rs1, Imm8` | `19 Rd Rs Im` | `Rd = Rs1.wrapping_sub(Im as i64)` | ~2 ns |
| `0x1A` | `AND` | `Rd, Rs1, Rs2` | `1a Rd S1 S2` | `Rd = Rs1 & Rs2` (64-bit bitwise AND) | ~2 ns |
| `0x1B` | `OR` | `Rd, Rs1, Rs2` | `1b Rd S1 S2` | `Rd = Rs1 \| Rs2` (64-bit bitwise OR) | ~2 ns |
| `0x1C` | `XOR` | `Rd, Rs1, Rs2` | `1c Rd S1 S2` | `Rd = Rs1 ^ Rs2` (64-bit bitwise XOR) | ~2 ns |
| `0x1D` | `SHL` | `Rd, Rs1, Rs2` | `1d Rd S1 S2` | `Rd = Rs1 << (Rs2 & 63)` (64-bit shift left) | ~2 ns |
| `0x1E` | `SHR` | `Rd, Rs1, Rs2` | `1e Rd S1 S2` | `Rd = (Rs1 as u64 >> (Rs2 & 63)) as i64` (logical right) | ~2 ns |
| `0x1F` | `ARR_DEF` | `ArrId, Len16` | `1f Ar Lh Ll` | `array_lens[Ar] = (Lh << 8) \| Ll` | ~3 ns |
| `0x20` | `ARR_LOAD` | `Rd, ArrId, Rs_idx` | `20 Rd Ar Rs` | `Rd = array_slots[base + Rs_idx]` (bounds checked) | ~4 ns |
| `0x21` | `ARR_STORE` | `ArrId, Rs_idx, Rs_val` | `21 Ar R1 R2` | `array_slots[base + R1] = R2` (bounds checked) | ~4 ns |
| `0x22` | `ASSERT` | `Rs1` | `22 Rs 00 00` | If `Rs1 == 0`, halt with `ERR_PX64_ASSERTION_FAILED` | ~2 ns |
| `0x23` | `CALL` | `Target16` | `23 00 Th Tl` | Push return IP + frame, `IP = (Th << 8) \| Tl` (depth $\le 8$) | ~4 ns |
| `0x24` | `RET` | None | `24 00 00 00` | Pop return IP + restore frame, return to caller | ~4 ns |
| `0x25` | `STRUCT_DEF` | `InstId, FieldCount` | `25 In Fc 00` | `struct_field_counts[In] = Fc` | ~2 ns |
| `0x26` | `STRUCT_LOAD` | `Rd, InstId, FieldOffset` | `26 Rd In Of` | `Rd = struct_slots[base + Of]` (bounds checked) | ~3 ns |
| `0x27` | `STRUCT_STORE` | `InstId, FieldOffset, Rs_val` | `27 In Of Rs` | `struct_slots[base + Of] = Rs` (bounds checked) | ~3 ns |
| `0x28` | `TBL_DEF` | `TblId, Base8, Len8` | `28 Tb Ba Le` | `table_bases[Tb] = Ba, table_lens[Tb] = Le` | ~2 ns |
| `0x29` | `TBL_LOAD` | `Rd, TblId, Rs_idx` | `29 Rd Tb Rs` | `Rd = const_pool[table_base + Rs_idx]` (bounds checked) | ~3 ns |
| `0x2A` | `STREQ` | `Rd, Rs1, Rs2` | `2a Rd S1 S2` | `Rd = streq(Rs1, Rs2) ? 1 : 0` (bounded comparison) | ~5 ns |

### 5.5 16-Byte Fixed Header & Binary Container Layout

```text
+-------------------------------------------------------------------------------+
| Bytes 0..3   : Magic Bytes ("PX64" -> 0x50, 0x58, 0x36, 0x34)                 |
| Bytes 4..5   : Version (0x0003, big-endian)                                   |
| Bytes 6..7   : Bytecode Section Length in Bytes (CodeLen: u16 big-endian)     |
| Bytes 8..9   : String Pool Section Length in Bytes (StrLen: u16 big-endian)   |
| Bytes 10..11 : Constant Pool Entries Count (u16 big-endian)                   |
| Byte  12     : Register Count (0x14 = 20 Registers)                           |
| Bytes 13..15 : Reserved (0x00, 0x00, 0x00)                                    |
+-------------------------------------------------------------------------------+
| Bytecode Payload (CodeLen bytes, 4-byte aligned px64 instructions)            |
+-------------------------------------------------------------------------------+
| String Pool Payload (StrLen bytes of UTF-8 string data)                       |
+-------------------------------------------------------------------------------+
| Constant Pool Payload (ConstCount * 8 bytes of 64-bit big-endian constants)   |
+-------------------------------------------------------------------------------+
```

### 5.6 Disassembly & Assembly Representation

Disassembly output generated by `pulc disasm <file.bin>` or LatencyOS shell `disasm <file.bin>`:

```text
=== [px64 Virtual Register Machine Disassembly] /bin/bench.bin ===
Magic: PX64 | Version: 3 | Code: 96 B | Registers: 20 GPRs+HW | StringPool: 48 B | ConstPool: 1 entries
OFFSET  HEX          INSTRUCTION  OPERANDS
---------------------------------------------------------------
0000:   12 00 03 00  CALL_NAT     $rax = @tsc()
0004:   02 01 00 00  MOV          $rcx, $rax
0008:   01 02 00 00  MOV          $rdx, 0
000c:   01 03 00 00  MOV          $rbx, 0
0010:   02 00 03 00  MOV          $rax, $rbx
0014:   01 0f 00 64  MOV          $r15, 100
0018:   0b 00 00 0f  CMPLT        $rax, $rax, $r15
001c:   10 00 00 3c  JZ           $rax, 0x003c
0020:   02 00 03 00  MOV          $rax, $rbx
0024:   01 0f 00 02  MOV          $r15, 2
0028:   06 00 00 0f  MUL          $rax, $rax, $r15
002c:   04 02 02 00  ADD          $rdx, $rdx, $rax
0030:   18 03 03 01  ADDI         $rbx, $rbx, 1
0034:   0f 00 00 10  JMP          0x0010
0038:   ...
005c:   16 00 00 00  HALT
```

---

## 6. The 43 Master Architectural & Semantic Contracts

1. **Specification WCET Value Alignment**: All instruction and intrinsic WCET values in documentation, compilers, and telemetry match kernel execution models exactly. Base dispatch: 25 ns; `@tsc()`: 15 ns; `@rtt()`: 20 ns; `@capture()`: 100 ns; `@send()`: 200 ns.
2. **Static WCET Upper Bound Formula**: Total script WCET is computed as $\text{WCET}_{\text{total}} = \sum (\text{Opcode Count} \times 2.5\text{ ns}) + \sum (\text{Intrinsic WCET})$.
3. **Time Dimension Typing**: Time literals (`500us`) fold to unsigned 64-bit integer nanoseconds at compile-time with zero runtime casting overhead.
4. **Tagged Pointer String Encoding**: Strings in the 512-byte static pool are tagged with `STR_TAG` (`0x4000_0000_0000_0000 | (offset << 32) | len`).
5. **Linear Handle Allocation**: `#f := @capture()` claims an active GPU zero-copy ring descriptor.
6. **Deadline Breach Reclamation (`!drop`)**: When `@within(t) { ... } !drop;` expires, `PX64_OP_DROP` frees unclaimed frame descriptors to prevent stale frames from reaching the NIC.
7. **Native Call ABI**: `PX64_OP_CALL_NAT` (`0x12`) passes destination register `Rd`, intrinsic ID `FuncId`, and argument register `ArgReg`.
8. **Native Return Semantics**: Value intrinsics return their result in `Rd`; void intrinsics return `0`.
9. **8-Level Deadline Stack**: The runtime maintains an 8-slot hardware deadline stack for nested `@within` scopes.
10. **Zero-Overhead Deadline Check**: `PX64_OP_DROP` executes in ~10 ns by comparing serialized TSC against the current deadline.
11. **Fault State Cleanup**: When execution halts on error or timeout, call stacks and deadline stacks are reset, and unconsumed descriptors are reclaimed.
12. **Branching Jump Semantics**: `if/else` and ternary expressions compile directly to `PX64_OP_JZ` (`0x10`) and `PX64_OP_JMP` (`0x0F`).
13. **Handle Ownership in Branches**: If a handle is acquired prior to a branch, all execution branches must consume it.
14. **Loop Handle Confinement**: Handles acquired inside a loop must be consumed within the same iteration.
15. **Capture Failure Handling**: If GPU capture rings are exhausted, `@capture()` returns null descriptor (0).
16. **Send Failure Backpressure**: If NIC TX rings are full, `@send()` drops the frame, increments backpressure counters, and marks the handle as consumed.
17. **Division by Zero Protection**: `PX64_OP_DIV` and `PX64_OP_MOD` with divisor `0` return `0` without CPU exceptions.
18. **Two's Complement Integer Overflow**: All integer operations wrap using standard two's complement arithmetic (`wrapping_add`, `wrapping_sub`, `wrapping_mul`).
19. **Boolean Representation**: Comparison operations return `1` for true and `0` for false.
20. **Internal Truthiness**: Any non-zero integer evaluates to true in conditional jumps.
21. **String Pool Bounds Safety**: String offsets are checked against the string pool length before slicing.
22. **Static String Pool Limit**: Total literal string bytes cannot exceed 512 bytes (`ERR_STRING_POOL_OVERFLOW`).
23. **Register Bound Protection**: Register indices outside `0..19` are rejected at compile time.
24. **Bytecode Verification**: Binaries must match the 16-byte `PX64` header and contain 4-byte aligned instructions.
25. **Bytecode Versioning**: Binaries require Version `3` (`0x0003`). Legacy `PULS` binaries are rejected with `ERR_BINARY_VERSION_MISMATCH`.
26. **Unified Intrinsic ID Map**: All 29 intrinsics map to fixed numeric IDs `1`..`29`.
27. **Target Hardware Profile**: x86_64 CPU with Invariant TSC, Intel 82540EM e1000 NIC, and 1080p 32bpp linear framebuffer.
28. **TSC Time Unit**: 1 TSC tick = 1 CPU clock cycle (e.g. 0.294 ns at 3.40 GHz).
29. **Nanoseconds to TSC Conversion**: $\text{Ticks} = \frac{\text{Nanoseconds} \times \text{TSC Freq (Hz)}}{1,000,000,000}$.
30. **C-State & Frequency Lock**: CPU cores are locked in C0 performance state to eliminate frequency scaling jitter.
31. **Interrupt Isolation**: Cores 1–3 run with `cli` (interrupts disabled); Core 0 handles APIC/UART interrupts with ISR WCET $\le 150$ ns.
32. **Cache Residency Assumption**: Hot loop code assumes L1 instruction cache residency (< 4 ns latency).
33. **DMA Cache Coherency**: Frame descriptors and ring buffers reside in Uncached (UC) or Write-Combining (WC) memory with `sfence` barriers.
34. **Memory Barrier Protocol**: `sfence` is issued after descriptor updates; `mfence` is issued on SPSC ring buffer updates.
35. **Core Interconnect Model**: Core-to-core communication uses single-producer single-consumer lock-free rings.
36. **VBLANK Contention Elimination**: Core 1 polls GPU VBLANK status register exclusively.
37. **Zero-Copy Pipeline Lifecycle**: `Stage 0 (Capture)` $\to$ `Stage 1 (Filter/Script)` $\to$ `Stage 2 (Network DMA)` $\to$ `Release`.
38. **DMA Descriptor Ring Lifecycle**: `Free` $\to$ `Allocated to Capture` $\to$ `TX Descriptor Writeback` $\to$ `Recycled`.
39. **NIC Completion Polling**: Core 3 polls the e1000 TX descriptor status bit `E1000_TXD_STAT_DD` without hardware interrupts.
40. **Framebuffer VBLANK Recycling**: Frame buffers recycle upon the subsequent VBLANK vertical sync edge.
41. **AI Diagnostic Protocol**: Compiler errors emit structured diagnostic headers (`[ERROR_CODE]`, `[LOCATION]`, `[AI_REPAIR_HINT]`).
42. **Loop Monotonicity Proof**: While loops must modify loop control variables towards termination to satisfy static analysis.
43. **Static vs Dynamic Verification**: Compile-time contracts guarantee conservative bounds; dynamic TSC guards enforce real-time guarantees at runtime.

---

## 7. Toolchain & Developer Experience

### 7.1 `pulc` Host Compiler CLI Reference

`pulc` is the official standalone compiler, validator, and disassembler toolchain for PulseLang and `px64`:

### Usage:
```bash
# Compile source to px64 binary artifact
pulc compile script.pul -o script.bin

# Fast compilation shorthand
pulc script.pul

# Execute pre-compiled px64 bytecode binary or source script
# Trailing CLI arguments are passed to @argc() and @arg(i)
pulc run fizzbuzz.bin
pulc run stream.pul "arg1" "arg2"

# Validate syntax, linear ownership, and WCET bounds without writing binary
pulc check script.pul

# Disassemble px64 binary bytecode into assembly instructions
pulc disasm script.bin
pulc -d script.bin

# Emit machine-readable JSON output for AI coding agents
pulc compile script.pul --json
pulc check script.pul --json
```

#### Subcommands:
| Subcommand | Description |
| :--- | :--- |
| `compile` | Compile PulseLang source into `px64` bytecode binary. |
| `run` | Execute bytecode binary or source script directly in the host virtual machine. |
| `check` | Validate syntax, types, linear ownership, and WCET bounds. |
| `disasm` | Disassemble `px64` bytecode into readable assembly instructions. |

#### Exit Codes:
- `0`: Success.
- `1`: Compilation, syntax, linear ownership, or WCET constraint error.
- `2`: IO, file access, or command-line argument error.

### 7.2 Structured JSON Diagnostic Protocol (`--json`)

When invoked with `--json`, `pulc` emits machine-readable JSON diagnostics tailored for AI agents:

#### Successful Compilation JSON:
```json
{
  "success": true,
  "input_path": "stream.pul",
  "output_path": "stream.bin",
  "code_size": 124,
  "instruction_count": 31,
  "string_pool_size": 42,
  "const_pool_count": 2,
  "estimated_wcet_ns": 450
}
```

#### Error Diagnostic JSON:
```json
{
  "success": false,
  "input_path": "fault.pul",
  "error": {
    "code": "ERR_MUTABILITY_VIOLATION",
    "message": "Variable is immutable, declare with 'let mut'",
    "line": 4,
    "col": 5,
    "byte_offset": 62,
    "token_kind": "VarIdent",
    "token_len": 4,
    "expected": "Mutable variable declaration",
    "stage": "Statement -> Assignment",
    "suggestion": "Declare variable with 'let mut $count = ...;' before mutating"
  }
}
```

### 7.3 AI-Actionable Diagnostic Output Format

Human-readable and serial console error reports emit structured, actionable diagnostic sections:

```text
==================== [PULSELANG COMPILE ERROR DIAGNOSTIC (AI-ACTIONABLE)] ====================
[ERROR_CODE]: ERR_MUTABILITY_VIOLATION
[MESSAGE]: Variable is immutable, declare with 'let mut'
[FILE]: script.pul
[LOCATION]: Line 4, Column 5 (ByteOffset: 62)
[TOKEN_FOUND]: Kind: VarIdent, Value: "$count"
[EXPECTED]: Mutable variable declaration
[PARSER_STAGE]: Statement -> Assignment
[SOURCE_CONTEXT]:
   3 | let $count = 0;
>  4 | $count += 1;
     | ^^^^
   5 | @println($count);
[HEX_DUMP (offset 0x0030..0x0050)]:
  00000030: 20 20 6c 65 74 20 24 63 6f 75 6e 74 20 3d 20 30 |  let $count = 0|
  00000040: 3b 0a 20 20 24 63 6f 75 6e 74 20 2b 3d 20 31 3b |;.  $count += 1;|
[AI_REPAIR_HINT]: Declare variable with 'let mut $count = ...;' before mutating
=============================================================================================
```

### 7.4 Exhaustive Compiler & Runtime Error Catalog

| Error Code | Category | Root Cause | AI Repair Hint |
|---|---|---|---|
| `ERR_MUTABILITY_VIOLATION` | Compile | Reassigned variable declared with `let` | Declare variable with `let mut $var = ...;` |
| `ERR_UNBOUNDED_LOOP` | Compile | While loop lacks monotonic termination progress | Add monotonic increment/decrement (e.g. `$i += 1;`) |
| `ERR_LINEAR_UNCONSUMED_HANDLE` | Compile | Descriptor `#f` captured but not consumed via `@send()` | Add `@send(#f);` before scope exit |
| `ERR_LINEAR_DOUBLE_SEND` | Compile | Descriptor `#f` transmitted multiple times | Consume `#handle` strictly once |
| `ERR_LINEAR_OVERWRITE` | Compile | Overwrote unconsumed `#handle` variable | Transmit prior `#handle` before reassigning |
| `ERR_MAX_ARRAYS_EXCEEDED` | Compile | Exceeded maximum 8 distinct arrays | Use fewer array declarations |
| `ERR_ARRAY_CAPACITY_EXCEEDED` | Compile | Exceeded 256 total array elements | Reduce array sizes |
| `ERR_MAX_STRUCTS_EXCEEDED` | Compile | Exceeded 8 distinct struct definitions | Define fewer struct types |
| `ERR_MAX_STRUCT_INSTS_EXCEEDED` | Compile | Exceeded 8 active struct instances | Use fewer struct variables |
| `ERR_UNKNOWN_STRUCT_FIELD` | Compile | Field does not exist on struct definition | Check struct field spelling |
| `ERR_PX64_TIMEOUT_EXCEEDED` | Runtime | Wall-clock execution exceeded 5.0 ms | Bound while loops or insert `@within` guards |
| `ERR_PX64_WCET_EXCEEDED` | Runtime | Instruction step limit exceeded (10,000 steps) | Ensure loop decrements to termination condition |
| `ERR_PX64_ASSERTION_FAILED` | Runtime | `@assert(cond)` evaluated to false (`0`) | Check computational pipeline and invariants |
| `ERR_PX64_ARRAY_OUT_OF_BOUNDS` | Runtime | Array index evaluated outside `0..N-1` | Bound indexing with `for $i in 0..N` |
| `ERR_PX64_STRUCT_OUT_OF_BOUNDS`| Runtime | Struct field offset is out of bounds | Verify struct instance and field offsets |
| `ERR_PX64_TABLE_OUT_OF_BOUNDS` | Runtime | Const table lookup index outside `0..N-1`| Guard index expression before table lookup |
| `ERR_PX64_STACK_OVERFLOW` | Runtime | Call stack exceeded 8 frames | Flatten recursive functions |
| `ERR_PX64_UNWRAP_FAILED` | Runtime | Called `@unwrap()` on an `Err` tagged result | Guard unwrap with `if (@is_ok($res))` |
| `ERR_BINARY_VERSION_MISMATCH` | Runtime | Binary compiled with outdated version | Recompile with `pulc compile <file.pul>` |
| `ERR_EXPECTED_LBRACE` | Compile | Missing opening brace `{` after `else`, `if`, `while`, `for`, `match`, or `@within` | Add `{` immediately after keyword (e.g. `else { if (...) { ... } }` instead of `else if`) |

### 7.5 PulseEditor In-Kernel Editor & Shortcut Bar

PulseEditor is the built-in full-screen ANSI text editor inside the LatencyOS kernel (`edit <file.pul>`). The bottom status line features a fixed nano-style shortcut bar:

```text
+----------------------------------------------------------------------------------------------------+
| [^S / F2 Save]  [^R / F5 Run]  [^Q / F10 Quit]  [^X Save&Quit]  [Esc C Clear]                     |
+----------------------------------------------------------------------------------------------------+
```

- **`^S` / `F2`**: Save buffer directly to kernel VFS (`fs_write`).
- **`^R` / `F5`**: Compile and execute active script immediately in `px64` VM.
- **`^Q` / `F10`**: Quit editor and return to PulseShell without saving.
- **`^X`**: Save buffer and exit to PulseShell.
- **`Esc C`**: Clear editor buffer.

---

## 8. Standard Production Script Templates (`.pul`)

### 8.1 `stream.pul`: Zero-Copy GPU-to-NIC Pipeline

```pulse
// stream.pul - Zero-Copy GPU-to-NIC Ultra-Low-Latency Pipeline
@pipeline: UltraStream @budget(8000us);

@on_vblank: {
    #f := @capture();
    @within(500us) {
        let $rtt = @rtt();
        if ($rtt > 200us) {
            @rate(80);
        } else {
            @rate(100);
        }
        @send(#f);
    } !drop;
};
```

### 8.2 `bench.pul`: Latency & Realtime Math Benchmark

```pulse
// bench.pul - Realtime Math & Latency Benchmark
@contract: @wcet(5us) @budget(50us);

let $t0 = @tsc();
let mut $sum = 0;

for $i in 0..100 {
    $sum += $i * 2;
}

let $dt = @tsc() - $t0;
@println("[BENCH] Iterations: 100");
@print("[RESULT] Sum: ");
@println($sum);
@print("[LATENCY] Cycles: ");
@println($dt);
```

### 8.3 `filter.pul`: Adaptive Congestion Guard

```pulse
// filter.pul - Adaptive Congestion Guard & Rate Controller
@contract: @wcet(2us) @budget(100us);

let $rtt = @rtt();
@print("[FILTER] Measured RTT (ns): ");
@println($rtt);

if ($rtt > 300us) {
    @println("[ACTION] Congestion detected -> Rate: 60%");
    @rate(60);
} else {
    @println("[ACTION] Optimal latency -> Rate: 100%");
    @rate(100);
}
```

### 8.4 `echo.pul`: CLI Argument Echo & String Formatter

```pulse
// echo.pul - CLI Argument Echo & Normalizer
@contract: @wcet(2us) @budget(20us);

let $argc = @argc();
if ($argc > 0) {
    let mut $i = 0;
    while ($i < $argc) {
        @print(@arg($i));
        $i += 1;
        if ($i < $argc) {
            @print(" ");
        }
    }
    @println("");
} else {
    @println("LatencyOS PulseLang Real-Time Script Engine Active");
}
```

### 8.5 `math_demo.pul`: Hardware Bit & Math Demonstration

```pulse
// math_demo.pul - Hardware Math & Bitwise Intrinsics Demonstration
@contract: @wcet(3us) @budget(30us);

let $val = -125;
let $abs_v = @abs($val);
let $clamped = @clamp(180, 0, 100);
let $pop = @popcnt(0b11110000);
let $lz = @lzcnt(0x00FF0000);
let $crc = @crc32(0, 0xDEADBEEF);

@print("[MATH] Absolute: ");
@println($abs_v);
@print("[MATH] Clamped: ");
@println($clamped);
@print("[BITS] Popcount: ");
@println($pop);
@print("[BITS] Leading Zeros: ");
@println($lz);
@print("[HASH] CRC32: ");
@println($crc);
```

### 8.6 `telemetry_ext.pul`: Extended Multi-Core Telemetry

```pulse
// telemetry_ext.pul - Extended Multi-Core Telemetry & Clock Monitor
@contract: @wcet(2us) @budget(25us);

let $core = @core_id();
let $freq = @tsc_freq();
let $uptime = @uptime_ns();
let $q_depth = @ring_depth(0);

@println("=== LatencyOS Hardware Telemetry ===");
@print("[CPU] LAPIC Core ID: ");
@println($core);
@print("[CPU] Invariant TSC Frequency (MHz): ");
@println($freq);
@print("[SYS] Boot Uptime (ns): ");
@println($uptime);
@print("[DMA] Capture Ring Depth: ");
@println($q_depth);
```

### 8.7 `vram_test.pul`: Direct Zero-Copy VRAM Framebuffer Test

```pulse
// vram_test.pul - Direct Zero-Copy VRAM Framebuffer Read/Write
@contract: @wcet(5us) @budget(50us);

let $slot = 0;
let $offset = 512;
let $signature = 0x50583634_4C415445; // "PX64LATE"

// Write signature word to VRAM slot buffer
@vram_write($slot, $offset, $signature);

// Read back and assert memory coherency
let $readback = @vram_read($slot, $offset);
@assert($readback == $signature);

@println("[VRAM] Framebuffer memory coherency verified.");
```

### 8.8 `fn_test.pul`: Static Function Calling & Contract Validation

```pulse
// fn_test.pul - Static Function Calling & Contract Validation
@contract: @wcet(4us) @budget(40us);

fn compute_metric($base, $scale) @requires($scale > 0) {
    let $result = ($base * $scale) + 10;
    return $result;
}

let $val1 = compute_metric(20, 3);
let $val2 = compute_metric(50, 2);

@assert($val1 == 70);
@assert($val2 == 110);
@println("[FN_TEST] Static function evaluation passed.");
```

### 8.9 `struct_test.pul`: Static Struct Manipulation

```pulse
// struct_test.pul - Static Struct Definition, Instantiation & Mutation
@contract: @wcet(3us) @budget(30us);

struct FrameMetadata {
    slot_id: i64,
    timestamp: i64,
    crc: i64,
}

let mut $meta = FrameMetadata {
    slot_id: 1,
    timestamp: 1000000,
    crc: 0x12345678,
};

$meta.slot_id := 2;
$meta.timestamp += 50000;

@print("[STRUCT] Frame Slot: ");
@println($meta.slot_id);
@print("[STRUCT] Frame Timestamp: ");
@println($meta.timestamp);
@println("[STRUCT] Struct validation complete.");
```

### 8.10 `match_test.pul`: Tagged Result Pattern Matching

```pulse
// match_test.pul - Tagged Result Pattern Matching & Error Propagation
@contract: @wcet(3us) @budget(30us);

fn safe_divide($a, $b) {
    if ($b == 0) {
        return @err(400); // Bad request / Zero division
    }
    return @ok($a / $b);
}

let $res1 = safe_divide(100, 4);
let $res2 = safe_divide(50, 0);

match $res1 {
    Ok($val) => {
        @print("[MATCH] Division result: ");
        @println($val);
    },
    Err($err_code) => {
        @print("[MATCH] Error encountered: ");
        @println($err_code);
    }
};

match $res2 {
    Ok($val) => {
        @println("[MATCH] Unexpected success");
    },
    Err($err_code) => {
        @print("[MATCH] Expected error code caught: ");
        @println($err_code);
    }
};
```

### 8.11 `fizzbuzz.pul`: Multi-Branch Conditionals & Nested If-Else

```pulse
// fizzbuzz.pul - Real-Time Multi-Branch Decision Tree & Arithmetic
@contract: @wcet(10us) @budget(100us);

for $i in 1..16 {
    if (($i % 15) == 0) {
        @println("FizzBuzz");
    } else {
        if (($i % 3) == 0) {
            @println("Fizz");
        } else {
            if (($i % 5) == 0) {
                @println("Buzz");
            } else {
                @println($i);
            }
        }
    }
}
```

