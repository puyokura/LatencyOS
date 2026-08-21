# PulseLang v2 Formal Language Specification

---

## 1. Executive Summary

PulseLang v2 is a temporal reactive DSL designed for ultra-low-latency, zero-copy hardware stream orchestration on LatencyOS. It combines mathematically dense syntax with explicit hardware contracts and zero dynamic memory allocation.

---

## 2. Lexical Structure

### 2.1 Identifiers and Prefixes
- **Variables**: Prefixed with `$` (e.g. `$rtt`, `$sum`, `$i`, `$t0`). Bound to static 64-bit integer register slots.
- **Hardware Handles**: Prefixed with `#` (e.g. `#f`, `#packet`). Represent non-copyable linear DMA descriptors.
- **Directives & Intrinsics**: Prefixed with `@` (e.g. `@contract`, `@pipeline`, `@on_vblank`, `@within`, `@tsc()`).

### 2.2 Operators
- **Walrus Assignment**: `:=` (e.g. `$a := 10;`)
- **Compound Assignment**: `+=`, `-=`
- **Ternary Operator**: `cond ? expr1 : expr2` or `cond ? { block1 } : { block2 };`
- **Stream Pipe**: `|>`
- **Relational**: `==`, `!=`, `<`, `<=`, `>`, `>=`
- **Arithmetic**: `+`, `-`, `*`, `/`, `%`
- **Deadline Guard**: `@within(time) { ... } !drop;`

### 2.3 Time Units
- `ns`: Nanoseconds
- `us`: Microseconds ($10^3$ ns)
- `ms`: Milliseconds ($10^6$ ns)
- `s`: Seconds ($10^9$ ns)

---

## 3. Formal EBNF Grammar

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

IntrinsicCall   ::= ( "@tsc" | "@rtt" | "@rate" | "@capture" | "@send" | "@argc" | "@arg" | "@print" | "@println" ) "(" ArgList? ")"
ArgList         ::= Expression ( "," Expression )*

IntegerLiteral  ::= [0-9]+
TimeLiteral     ::= [0-9]+ ("ns" | "us" | "ms" | "s")
StringLiteral   ::= '"' [^"]* '"'
VarIdent        ::= "$" [a-zA-Z0-9_]+
HardwareIdent   ::= "#" [a-zA-Z0-9_]+
Identifier      ::= [a-zA-Z_] [a-zA-Z0-9_]*
```

---

## 4. Hardware Intrinsics Catalog

| Intrinsic | Signature | WCET | Description |
|---|---|---|---|
| `@tsc()` | `() -> i64` | **~15 ns** | Reads serialized hardware Time-Stamp Counter (`lfence; rdtsc`). |
| `@rtt()` | `() -> i64` | **~20 ns** | Queries active network round-trip time in nanoseconds from NIC driver. |
| `@rate(pct)` | `(i64) -> ()` | **~10 ns** | Sets congestion throttle percentage (range: `10` to `100`). |
| `@capture()` | `() -> #handle`| **~100 ns**| Claims zero-copy GPU frame buffer descriptor slot. |
| `@send(#h)` | `(#handle) -> ()` | **~200 ns**| Enqueues frame buffer to Intel e1000 NIC TX ring and moves ownership. |
| `@argc()` | `() -> i64` | **~5 ns** | Returns number of CLI arguments passed to the script (0..8). |
| `@arg(idx)` | `(i64) -> Tagged` | **~10 ns** | Accesses command-line argument at index `idx` via zero-allocation tagged pointer. |
| `@print(v)` | `(Any) -> ()` | **~500 ns**| Emits text, argument, or number to UART COM1 port (no newline). |
| `@println(v)`| `(Any) -> ()` | **~500 ns**| Emits text, argument, or number to UART COM1 port with automatic CRLF normalization. |

---

## 5. Command-Line Arguments & Zero-Copy Passing

PulseLang scripts executed via `run <file> [args...]` receive CLI arguments directly into static memory buffers:

```pulse
// echo.pl - Accessing Command-Line Arguments
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

