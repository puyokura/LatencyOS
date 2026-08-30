# PulseLang v2 Documentation Hub

PulseLang v2 is the native, AI-optimized, temporal reactive Domain-Specific Language (DSL) built directly into the LatencyOS kernel.

---

## Documentation Structure

| Document | Description | Target Audience |
|---|---|---|
| [**Formal Specification (`spec.md`)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/spec.md) | Full language specification, formal EBNF grammar, type system, and contracts | Developers, Systems Architects |
| [**AI Specification (`ai_spec.md`)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ai_spec.md) | Machine-readable grammar, invariant rules, and code generation templates | AI Assistants, LLMs, Static Analyzers |
| [**Bytecode ISA Reference (`isa.md`)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/isa.md) | Virtual Machine architecture, stack machine model, opcodes, and ABI | Compiler Developers, VM Engineers |
| [**Scripts Cookbook (`cookbook.md`)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/cookbook.md) | Standard `.pul` scripts, line-by-line breakdowns, and real-time recipes | Application Developers |

---

## Japanese Documentation (日本語ドキュメント)

- [**PulseLang 言語ポータル (日本語)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ja/README.md)
- [**形式言語仕様書 (日本語)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ja/spec.md)
- [**AI向け形式仕様書 (日本語)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ja/ai_spec.md)
- [**バイトコード ISA 仕様書 (日本語)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ja/isa.md)
- [**スクリプトクックブック (日本語)**](file:///C:/Users/User/Desktop/LatencyOS/docs/lang/ja/cookbook.md)

---

## Key Language Highlights

1. **First-Class Time Literals**: `50ns`, `200us`, `5ms`, `1s` with immediate compile-time nanosecond folding.
2. **Deterministic Linear Types**: Hardware DMA buffer handles (`#f`) with single-consumer ownership verification.
3. **Compiler Contracts**: `@contract: @wcet(5us) @budget(50us);` for static and runtime latency bounds.
4. **Zero Heap Allocation**: Single-pass $O(N)$ AST-less compilation and fixed-size register slots (`$0`..`$31`).
