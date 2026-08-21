# px64 (Pulse Extended 64-bit Real-Time Architecture) ISA & VM Specification

---

## 1. Architectural Overview & Design Invariants

`px64` is a deterministic, 64-bit register-machine Instruction Set Architecture (ISA) and virtual machine engineered directly into the LatencyOS kernel. It is specifically designed to execute hard real-time streaming scripts with strictly bounded Worst-Case Execution Time (WCET) and zero runtime heap allocations.

### Architectural Invariants:
1. **Fixed 32-bit (4-Byte) Instruction Format**: Every instruction is exactly 4 bytes long, enabling $O(1)$ constant-time instruction fetch and decoding without variable-length instruction hazards.
2. **20-Register Architecture**: 16 General Purpose Registers (`$rax`..`$r15`) + 4 Hardware DMA Slot Registers (`#f0`..`#f3`).
3. **Tagged Pointer String & Argument Encoding**: Strings and CLI arguments are passed via high-bit tagged 64-bit integers (`STR_TAG: 0x4000_...`, `ARG_TAG: 0x2000_...`) with zero heap allocation.
4. **Hardware-Integrated Temporal Guards**: First-class hardware instructions for deadline tracking (`WITHIN_START`, `WITHIN_END`, `DROP`) backed by CPU TSC serialized registers.
5. **Deterministic Step Budget**: Maximum 10,000 instruction steps per execution to strictly prevent infinite loops and WCET overruns.

---

## 2. Register File Map (20 Registers)

| Reg ID | Canonical Name | x64 Alias | Primary Architectural Purpose |
|---|---|---|---|
| `0` | `$rax` | `$r0` | Accumulator, Primary Expression Result, Return Value |
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
| `16` | `#f0` | `#frame`, `#slot0` | Hardware Zero-Copy Frame Slot 0 Descriptor |
| `17` | `#f1` | `#slot1` | Hardware Zero-Copy Frame Slot 1 Descriptor |
| `18` | `#f2` | `#slot2` | Hardware Zero-Copy Frame Slot 2 Descriptor |
| `19` | `#f3` | `#slot3` | Hardware Zero-Copy Frame Slot 3 Descriptor |

---

## 3. 32-bit Fixed Instruction Format

All `px64` instructions are 4 bytes aligned:

```text
+----------------+----------------+----------------+----------------+
| Byte 0 (Opcode)| Byte 1 (Rd)    | Byte 2 (Rs1)   | Byte 3 (Rs2)   |
| [7:0]          | [7:0]          | [7:0] / Imm_hi | [7:0] / Imm_lo |
+----------------+----------------+----------------+----------------+
```

- **Byte 0 (`Opcode`)**: `PX64_OP_*` opcode identifier (0..22).
- **Byte 1 (`Rd`)**: Destination Register ID (0..19).
- **Byte 2 (`Rs1`)**: First Source Register ID (0..19) OR High Byte of 16-bit Immediate (`Imm[15:8]`).
- **Byte 3 (`Rs2`)**: Second Source Register ID (0..19) OR Low Byte of 16-bit Immediate (`Imm[7:0]`).

---

## 4. Complete Instruction Set

| Opcode | Mnemonic | Operands | Encoding | Semantics & Operation | WCET |
|---|---|---|---|---|---|
| `0x00` | `NOP` | None | `00 00 00 00` | No Operation | ~1 ns |
| `0x01` | `MOV` | `Rd, Imm16` | `01 Rd Ih Il` | `Rd = (Ih << 8) \| Il` | ~2 ns |
| `0x02` | `MOV` | `Rd, Rs1` | `02 Rd Rs 00` | `Rd = Rs1` | ~2 ns |
| `0x03` | `MOVS` | `Rd, Offset, Len` | `03 Rd Of Ln` | `Rd = STR_TAG \| (Of << 32) \| Ln` | ~3 ns |
| `0x04` | `ADD` | `Rd, Rs1, Rs2` | `04 Rd S1 S2` | `Rd = Rs1.wrapping_add(Rs2)` | ~2 ns |
| `0x05` | `SUB` | `Rd, Rs1, Rs2` | `05 Rd S1 S2` | `Rd = Rs1.wrapping_sub(Rs2)` | ~2 ns |
| `0x06` | `MUL` | `Rd, Rs1, Rs2` | `06 Rd S1 S2` | `Rd = Rs1.wrapping_mul(Rs2)` | ~3 ns |
| `0x07` | `DIV` | `Rd, Rs1, Rs2` | `07 Rd S1 S2` | `Rd = (Rs2 != 0) ? Rs1 / Rs2 : 0` | ~12 ns |
| `0x08` | `MOD` | `Rd, Rs1, Rs2` | `08 Rd S1 S2` | `Rd = (Rs2 != 0) ? Rs1 % Rs2 : 0` | ~12 ns |
| `0x09` | `CMPEQ` | `Rd, Rs1, Rs2` | `09 Rd S1 S2` | `Rd = (Rs1 == Rs2) ? 1 : 0` | ~2 ns |
| `0x0A` | `CMPNE` | `Rd, Rs1, Rs2` | `0a Rd S1 S2` | `Rd = (Rs1 != Rs2) ? 1 : 0` | ~2 ns |
| `0x0B` | `CMPLT` | `Rd, Rs1, Rs2` | `0b Rd S1 S2` | `Rd = (Rs1 < Rs2) ? 1 : 0` | ~2 ns |
| `0x0C` | `CMPLE` | `Rd, Rs1, Rs2` | `0c Rd S1 S2` | `Rd = (Rs1 <= Rs2) ? 1 : 0` | ~2 ns |
| `0x0D` | `CMPGT` | `Rd, Rs1, Rs2` | `0d Rd S1 S2` | `Rd = (Rs1 > Rs2) ? 1 : 0` | ~2 ns |
| `0x0E` | `CMPGE` | `Rd, Rs1, Rs2` | `0e Rd S1 S2` | `Rd = (Rs1 >= Rs2) ? 1 : 0` | ~2 ns |
| `0x0F` | `JMP` | `Target16` | `0f 00 Th Tl` | `IP = (Th << 8) \| Tl` | ~2 ns |
| `0x10` | `JZ` | `Rs1, Target16` | `10 Rs Th Tl` | `if Rs1 == 0 { IP = Target }` | ~3 ns |
| `0x11` | `JNZ` | `Rs1, Target16` | `11 Rs Th Tl` | `if Rs1 != 0 { IP = Target }` | ~3 ns |
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
| `0x1E` | `SHR` | `Rd, Rs1, Rs2` | `1e Rd S1 S2` | `Rd = (Rs1 as u64 >> (Rs2 & 63)) as i64` (logical shift right) | ~2 ns |
| `0x1F` | `ARR_DEF` | `ArrId, Len16` | `1f Ar Lh Ll` | `array_lens[Ar] = (Lh << 8) \| Ll` | ~3 ns |
| `0x20` | `ARR_LOAD` | `Rd, ArrId, Rs_idx` | `20 Rd Ar Rs` | `Rd = array_slots[base + Rs_idx]` (bounds checked: `[0..N-1]`) | ~4 ns |
| `0x21` | `ARR_STORE` | `ArrId, Rs_idx, Rs_val` | `21 Ar R1 R2` | `array_slots[base + R1] = R2` (bounds checked: `[0..N-1]`) | ~4 ns |
| `0x22` | `ASSERT` | `Rs1` | `22 Rs 00 00` | If `Rs1 == 0`, halt with `ERR_PX64_ASSERTION_FAILED` | ~2 ns |
| `0x23` | `CALL` | `Target16` | `23 00 Th Tl` | Push return IP + frame, `IP = (Th << 8) \| Tl` (depth <= 8) | ~4 ns |
| `0x24` | `RET` | None | `24 00 00 00` | Pop return IP + restore frame, return to caller with `$rax` | ~4 ns |
| `0x25` | `STRUCT_DEF` | `InstId, FieldCount` | `25 In Fc 00` | `struct_field_counts[In] = Fc` | ~2 ns |
| `0x26` | `STRUCT_LOAD` | `Rd, InstId, FieldOffset` | `26 Rd In Of` | `Rd = struct_slots[base + Of]` (bounds checked: `[0..F-1]`) | ~3 ns |
| `0x27` | `STRUCT_STORE` | `InstId, FieldOffset, Rs_val` | `27 In Of Rs` | `struct_slots[base + Of] = Rs` (bounds checked: `[0..F-1]`) | ~3 ns |
| `0x28` | `TBL_DEF` | `TblId, Base8, Len8` | `28 Tb Ba Le` | `table_bases[Tb] = Ba, table_lens[Tb] = Le` | ~2 ns |
| `0x29` | `TBL_LOAD` | `Rd, TblId, Rs_idx` | `29 Rd Tb Rs` | `Rd = const_pool[table_base + Rs_idx]` (bounds checked: `[0..N-1]`) | ~3 ns |
| `0x2A` | `STREQ` | `Rd, Rs1, Rs2` | `2a Rd S1 S2` | `Rd = (str_equal(Rs1, Rs2)) ? 1 : 0` (bounded O(1) comparison) | ~5 ns |

---

## 5. Native Intrinsics (`CALL_NAT`) Reference

| Func ID | Intrinsic Name | Signature | Description |
|---|---|---|---|
| `1` | `@print` | `(any) -> 0` | Print string literal, tagged CLI argument, or integer to serial console without heap allocation. |
| `2` | `@println` | `(any) -> 0` | Print value followed by CRLF to serial console. |
| `3` | `@tsc` | `() -> i64` | Read hardware serialized Time Stamp Counter (`lfence; rdtsc`). |
| `4` | `@rtt` | `() -> i64` | Read active network minimum round-trip time in nanoseconds. |
| `5` | `@rate` | `(pct: i64) -> 0` | Adjust network congestion throttle percentage (10%..100%). |
| `6` | `@capture` | `() -> i64` | Zero-copy GPU frame capture synchronized with VBLANK edge. Returns slot ID (16..19). |
| `7` | `@send` | `(slot: i64) -> 1` | Transmit frame via kernel-bypass Intel e1000 PMD driver with SRTP/AES-GCM encryption. |
| `8` | `@argc` | `() -> i64` | Return number of CLI arguments passed to script (0..8). |
| `9` | `@arg` | `(idx: i64) -> Tagged` | Return tagged pointer reference to CLI argument at index `idx`. |
| `10` | `@ok` | `(val: i64) -> Tagged` | Construct and return a tagged OK Result with value `val`. |
| `11` | `@err` | `(code: i64) -> Tagged` | Construct and return a tagged Err Result with error code `code`. |
| `12` | `@is_ok` | `(res: Tagged) -> i64`| Return `1` if result is OK, `0` if Err. |
| `13` | `@is_err` | `(res: Tagged) -> i64`| Return `1` if result is Err, `0` if OK. |
| `14` | `@unwrap` | `(res: Tagged) -> i64`| Extract payload from OK Result, or fail with runtime fault if Err. |
| `15` | `@streq` | `(s1: str, s2: str) -> i64` | Compare two strings or CLI arguments for byte equality with bounded execution time. |

---

## 6. Binary Container Format (`PX64`)

Compiled `px64` executable binaries contain a 16-byte fixed header followed by 4-byte aligned bytecode instructions and a static string pool.

```text
+-------------------------------------------------------------------------------+
| Bytes 0..3   : Magic Bytes ("PX64" -> 0x50, 0x58, 0x36, 0x34)                 |
| Bytes 4..5   : Version (0x0003)                                               |
| Bytes 6..7   : Bytecode Section Length in Bytes (CodeLen: u16 big-endian)     |
| Bytes 8..9   : String Pool Section Length in Bytes (StrLen: u16 big-endian)   |
| Bytes 10..11 : Constant Pool Entries Count (u16 big-endian)                   |
| Bytes 12..13 : Register Count (0x0014 = 20 Registers)                         |
| Bytes 14..15 : Reserved (0x0000)                                              |
+-------------------------------------------------------------------------------+
| Bytecode Payload (CodeLen bytes, 4-byte aligned px64 instructions)            |
+-------------------------------------------------------------------------------+
| String Pool Payload (StrLen bytes of raw UTF-8 string data)                   |
+-------------------------------------------------------------------------------+
| Constant Pool Payload (ConstCount * 8 bytes of 64-bit big-endian constants)   |
+-------------------------------------------------------------------------------+
```

---

## 7. Disassembly Output Format

The in-kernel `disasm <file.bin>` command inspects and formats `PX64` binaries:

```text
=== [px64 Virtual Register Machine Disassembly] /bin/echo.bin ===
Magic: PX64 | Version: 3 | Code: 124 B | Registers: 20 GPRs+HW | StringPool: 51 B | ConstPool: 2 entries
OFFSET  HEX          INSTRUCTION  OPERANDS
---------------------------------------------------------------
0000:   12 01 08 00  CALL_NAT     $rcx = @argc($rax)
0004:   02 00 01 00  MOV          $rax, $rcx
0008:   01 0f 00 00  MOV          $r15, 0
000c:   0d 00 00 0f  CMPGT        $rax, $rax, $r15
0010:   10 00 00 70  JZ           $rax, 0x0070
0014:   01 02 00 00  MOV          $rdx, 0
0018:   02 00 02 00  MOV          $rax, $rdx
001c:   02 0f 01 00  MOV          $r15, $rcx
0020:   0b 00 00 0f  CMPLT        $rax, $rax, $r15
0024:   10 00 00 64  JZ           $rax, 0x0064
...
0078:   16 00 00 00  HALT
```

