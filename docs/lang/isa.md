# PulseLang v2 Bytecode ISA & Virtual Machine Reference

---

## 1. Virtual Machine Architectural Limits

- **Stack Size**: 64 entries (64-bit signed integers).
- **Static Variable Slots**: 32 entries (`$0` to `$31`, `i64`).
- **Static String Pool**: 512 bytes with tagged pointer representation (`0x7FFF_...`).
- **Step Limit**: 10,000 instructions max (hard infinite loop & WCET breach prevention).
- **Deadline Stack**: 8-level nested temporal deadline stack with hardware TSC comparison.

---

## 2. Bytecode Instruction Set

| Opcode | Mnemonic | Operands | Stack Effect | Description |
|---|---|---|---|---|
| `0x00` | `OP_NOP` | None | `[] -> []` | No operation |
| `0x01` | `OP_PUSH_CONST` | `i64` (8 bytes) | `[] -> [val]` | Push immediate 64-bit integer |
| `0x02` | `OP_LOAD_VAR` | `u8` (1 byte) | `[] -> [var[idx]]` | Load value from register slot |
| `0x03` | `OP_STORE_VAR` | `u8` (1 byte) | `[val] -> []` | Store value to register slot |
| `0x04` | `OP_ADD` | None | `[a, b] -> [a + b]` | Integer addition |
| `0x05` | `OP_SUB` | None | `[a, b] -> [a - b]` | Integer subtraction |
| `0x06` | `OP_MUL` | None | `[a, b] -> [a * b]` | Integer multiplication |
| `0x07` | `OP_DIV` | None | `[a, b] -> [a / b]` | Integer division (div-by-zero protected) |
| `0x08` | `OP_MOD` | None | `[a, b] -> [a % b]` | Integer modulo |
| `0x09` | `OP_CMP_EQ` | None | `[a, b] -> [a == b]` | Equality test (1 or 0) |
| `0x0A` | `OP_CMP_NE` | None | `[a, b] -> [a != b]` | Inequality test |
| `0x0B` | `OP_CMP_LT` | None | `[a, b] -> [a < b]` | Less than |
| `0x0C` | `OP_CMP_LE` | None | `[a, b] -> [a <= b]` | Less than or equal |
| `0x0D` | `OP_CMP_GT` | None | `[a, b] -> [a > b]` | Greater than |
| `0x0E` | `OP_CMP_GE` | None | `[a, b] -> [a >= b]` | Greater than or equal |
| `0x0F` | `OP_JUMP` | `u16` (2 bytes) | `[] -> []` | Unconditional jump |
| `0x10` | `OP_JUMP_IF_FALSE`| `u16` (2 bytes)| `[cond] -> []` | Jump if top of stack is 0 |
| `0x11` | `OP_CALL_NATIVE` | `u8, u8` (2 bytes)| `[args...] -> [res]` | Call hardware intrinsic (func_id, argc) |
| `0x12` | `OP_WITHIN_START`| `i64` (8 bytes) | `[] -> []` | Push temporal deadline (ns) |
| `0x13` | `OP_WITHIN_END` | None | `[] -> []` | Pop and evaluate temporal deadline |
| `0x14` | `OP_DROP` | None | `[] -> []` | Drop overdue frame & free descriptors |
| `0x15` | `OP_PUSH_STR` | `u16, u16` (4B) | `[] -> [ptr]` | Push static string pool reference |
| `0x16` | `OP_HALT` | None | `[] -> []` | Terminate script execution |

---

## 3. Native Function Identifiers (`NATIVE_*`)

- `1`: `NATIVE_PRINT` (`@print`)
- `2`: `NATIVE_PRINTLN` (`@println`)
- `3`: `NATIVE_SYS_TSC` (`@tsc`)
- `4`: `NATIVE_NET_RTT` (`@rtt`)
- `5`: `NATIVE_NET_SET_RATE` (`@rate`)
- `6`: `NATIVE_GPU_CAPTURE` (`@capture`)
- `7`: `NATIVE_NET_SEND` (`@send`)
