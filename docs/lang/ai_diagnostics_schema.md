# PulseLang AI-Actionable Diagnostic Schema (v1)

This specification defines the machine-readable diagnostic schema produced by `pulc --json` and consumed by AI coding agents and the Language Server Protocol (`pulc-lsp`).

---

## 1. Schema Overview

- **Schema URI**: `https://latencyos.org/schema/pulselang-diagnostic-v1.json`
- **Schema Version**: `1.0`

Every compile, check, or test failure emitted with `--json` adheres to this format on `stderr` (or in LSP `publishDiagnostics` payload).

---

## 2. JSON Structure

```json
{
  "$schema": "https://latencyos.org/schema/pulselang-diagnostic-v1.json",
  "version": "1.0",
  "success": false,
  "diagnostics": [
    {
      "code": "ERR_SYNTAX_UNEXPECTED_TOKEN",
      "category": "syntax",
      "severity": "error",
      "message": "Unexpected token",
      "location": {
        "file": "script.pul",
        "line": 3,
        "column": 7,
        "byte_offset": 45,
        "length": 2
      },
      "stage": "Expression -> Primary",
      "expected": "Literal value, variable ($var), hardware handle (#h), or intrinsic call (@fn)",
      "cause": "Unexpected token encountered during primary expression parsing",
      "repairability": "safe_patch",
      "ai_repair_hint": "Replace invalid token with a valid variable name, number, or expression",
      "repairs": [
        {
          "id": "ERR_SYNTAX_UNEXPECTED_TOKEN",
          "description": "Replace invalid token with a valid variable name, number, or expression",
          "confidence": 0.95,
          "edits": []
        }
      ]
    }
  ]
}
```

---

## 3. Diagnostic Categories

| Category | Description | Examples |
|---|---|---|
| `syntax` | Lexical, token delimiter, or BNF grammar parsing error. | `ERR_UNEXPECTED_TOKEN`, `ERR_UNCLOSED_STRING` |
| `type` | Type mismatch, immutable variable reassignment. | `ERR_TYPE_MISMATCH`, `ERR_MUTABILITY_VIOLATION` |
| `linear_ownership` | Unconsumed linear handle, double-send, or overwrite. | `ERR_LINEAR_UNCONSUMED_HANDLE`, `ERR_LINEAR_DOUBLE_SEND` |
| `wcet` | Static WCET bound exceeded or missing contract budget. | `ERR_WCET_EXCEEDED`, `ERR_UNBOUNDED_LOOP` |
| `runtime` | VM instruction fault, assertion failure, watchdog violation. | `ERR_PX64_ASSERTION_FAILED`, `ERR_PX64_TIMEOUT_EXCEEDED` |
| `semantic` | General semantic validation or module resolution error. | `ERR_RUNTIME_NOT_IMPORTED` |

---

## 4. Repairability Levels

- `safe_patch`: Unambiguous fix with high confidence (e.g., auto-inserting missing `@contract` or `@import "sys";`).
- `suggestion`: Algorithmic or architectural recommendation requiring AI synthesis.
- `requires_decision`: Multiple valid architectural choices exist (e.g., choice between sending or dropping an unused handle).
- `none`: Unrecoverable runtime or binary payload corruption.
