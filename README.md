# LatencyOS

> **A Hard Real-Time, Deterministic Low-Latency Operating System in Rust (`no_std`).**  
> **Release Version**: `v0.0.40` | **Language Specification**: `PulseLang v3.2 (px64 v3)`

---

## 1. Overview & Core Philosophy

LatencyOS is a dedicated appliance operating system built exclusively to minimize **end-to-end (glass-to-glass) latency and eliminate timing jitter**.

Traditional operating systems (Linux, Windows) optimize for **average throughput and general-purpose fairness**. In contrast, LatencyOS optimizes strictly for **Worst-Case Execution Time (WCET)**:

- **Zero Dynamic Allocation**: 100% pre-allocated static memory pools. No runtime heap allocations (`malloc`, `Box`, or `alloc` crate) in any execution path.
- **Lock-Free Architecture**: No mutexes, semaphores, or condition variables. Inter-core communication is performed via single-producer single-consumer (SPSC) cacheline-aligned ring buffers.
- **Static Core Pinning & Zero Preemption**: Each CPU core has a fixed, dedicated hardware role. Schedulers are completely omitted on streaming cores.
- **Zero-Jitter C-State Locking**: CPU power-saving C-state transitions are hardware-disabled via MSR `0x1A0` and `0x1B0` to eliminate clock-gating wake-up latencies.
- **Kernel-Bypass Hardware Drivers**: Direct poll-mode drivers (PMD) for Intel e1000 NIC and zero-copy GPU frame buffer capture.

---

## 2. 4-Core Static Pipeline Architecture

```
+-----------------------------------------------------------------------------------+
|                                  LatencyOS Pipeline                               |
+-----------------------------------------------------------------------------------+
| Core 0: Control / Pulse Shell  (ISR -> Command Execution: budget 0.15ms)          |
| Core 1: Capture Engine         (VBLANK Sync -> Zero-Copy GPU DMA: budget 2.00ms)   |
| Core 2: Encode Engine          (Hardware Video Encode: budget 4.50ms)             |
| Core 3: Network Engine         (Kernel-Bypass PMD -> SRTP / AES-GCM: budget 5.00ms)|
+-----------------------------------------------------------------------------------+
```

---

## 3. Key Components

### 3.1 Pulse Shell (Time-Native Unix Minimalist Shell)
- **Time-First Prompt**: `[c0|18ns] % ` displays the active CPU core and hardware TSC execution latency ($\Delta t$) of the preceding command.
- **Deadline Guard (`within <time> <cmd>`)**: Evaluates real-time execution against strict hardware budgets (e.g. `within 500us run filter.pul`).
- **Hierarchical Path Resolution**: Full support for absolute paths (`/pulselang/echo.pul`), CWD relative paths (`echo.pul`), `cd`, and `pwd`.
- **Command-Line Arguments**: Transparent argument passing to scripts and binaries (`run /bin/echo.bin "hello world"`).
- **Disassembler (`disasm <file.bin>`)**: Decodes `px64` binaries into 32-bit fixed instructions with x64 register names (`$rax`..`$r15`).
- **Hardware Telemetry Commands**:
  - `timeline`: Monospace microsecond breakdown of the 6 pipeline stages.
  - `ring`: Real-time inspection of SPSC lock-free queues (occupancy, head/tail pointers).
  - `cores`: APIC ID, core role, C0 state locks, and iteration metrics.
  - `tsc`: Raw hardware TSC timestamp and cycle resolution.
  - `ls -t`: File listing with worst-case execution time (WCET) budgets.
  - `doc pulse`: In-kernel formal specification of PulseLang v2.
  - `exit` / `poweroff`: ACPI hardware shutdown.

### 3.2 PulseLang (Language Spec: v3.2 / Architecture: px64 v3)
- **`px64` 64-bit Virtual Register Architecture**: 20-register model (16 GPRs `$rax`..`$r15` + 4 HW DMA slots `#f0`..`#f3`) with 32-bit fixed-length instructions.
- **Enums & Exhaustive Pattern Matching**: Support for sum types (`enum`), scoped variant resolution (`EnumName::Variant`), and compile-time exhaustiveness checking in `match` statements (with error reporting for missing/duplicate/invalid patterns).
- **64-bit Constant Pool & Immediate ALU**: 16-bit index constant pool loading (`0x17 LDC Rd, const[idx]`) and 8-bit immediate operations (`0x18 ADDI`, `0x19 SUBI`).
- **Safety Guards & Bounds Checking**: Out-of-bounds constant pool protection (`ERR_PX64_CONST_OUT_OF_BOUNDS`) and invalid opcode trapping (`ERR_PX64_INVALID_OPCODE`).
- **Dual Runtime Safety Watchdog**: 10,000 instruction steps limit + 50.0ms TSC wall-clock timeout guard (worst-case execution bound: 5.48ms).
- **Disassembler (`disasm <file.bin>`)**: Decodes bytecode with explicit virtual register clarification.
- **In-Kernel Instruction Microbenchmarking (`pulse-bench` / `benchmark`)**: TSC-serialized nanosecond benchmarking for each VM opcode.
- **AI-Actionable Error Diagnostics**: Machine-readable structured diagnostic logs with error codes, byte offsets, ASCII/Hex dumps, and automatic repair hints (syntax vs runtime separation).
- **Extensive Real-Time Intrinsics**: Core ID, TSC frequency, uptime, busy-wait, ring depth, branchless math, bits, hash, zero-copy VRAM DMA, etc.
- **Standard Scripts**: `stream.pul`, `contracts_and_enums.pul`, `fizzbuzz.pul`, etc.
- **Formal Contracts & Testing**: PulseLang v3.2 natively supports Design-by-Contract preconditions (`@requires`) and postconditions (`@ensures($result > 0)`) verified against the return register `$rax`, along with embedded native unit testing (`@test "name" @budget(...) { ... }`).
### 3.3 PulseEditor (In-Kernel ANSI Text Editor)
- Full-screen terminal text editor running inside the kernel on Core 0.
- **Nano-Style Shortcut Bar**: Persistent bottom bar (`[^S / F2 Save]  [^R / F5 Run]  [^Q / F10 Quit]  [^X Save&Quit]  [Esc C Clear]`).
- **High-Speed Paste**: Instant UART batch drain preventing character drops on large code pastes.
- Real-time ANSI syntax highlighting for PulseLang tokens, directives, and numbers.

### 3.4 LatencyFS & LatencyVFS (Static Real-Time Filesystems)
- **LatencyFS**: In-memory static filesystem with zero fragmentation and fixed memory layout. Precompiled `px64` binaries in `/bin/`.
- **LatencyVFS**: GPU DMA Framebuffer VRAM disk mapped directly at `/vram/` (`/vram/slot0..7`, `/vram/scratch`, `/vram/stats`).

### 3.5 Export Disk (Windows 11 FAT16 Interop & Auto-Sync)
- **Secondary ATA FAT16 Drive**: Dedicated interop disk (`export.img`) for seamless data exchange between Windows host and LatencyOS.
- **Boot Auto-Import**: Scans the FAT16 root directory on boot and automatically loads files and PulseLang scripts into LatencyFS.
- **Continuous Write-Through Auto-Sync**: File CRUD operations (`edit`, `touch`, `cp`, `mv`, `rm`, `compile`) immediately persist to the FAT16 disk, enabling persistent scripts across full cold OS reboots.
- **Shell Management Commands**: `export-ls` (FAT16 directory list), `import <file> [dst]`, `export <src> [dst]`.

---

## 4. Building and Running (Windows 11 Native)

LatencyOS is developed and verified natively on **Windows 11** using the `cargo xtask` build pattern.

### 4.1 Prerequisites
- **Rust Toolchain**: `rustup` + `x86_64-unknown-none` target
- **C++ Compiler**: LLVM/Clang for Windows (`clang++.exe`)
- **Assembler**: NASM for Windows (`nasm.exe`)
- **Emulator**: QEMU for Windows (`qemu-system-x86_64.exe`)

### 4.2 Setup Toolchain Paths
```powershell
$env:PATH = 'C:\Users\User\scoop\apps\llvm\current\bin;C:\Users\User\scoop\apps\rustup\current\.cargo\bin;C:\Users\User\scoop\apps\QEMU\current;C:\Users\User\scoop\shims;' + $env:PATH
```

### 4.3 Check Toolchain and Target
```powershell
cargo run --package xtask -- check
```

### 4.4 Build and Run in Interactive Mode
```powershell
cargo run --package xtask -- interactive --release
```

### 4.5 Package Standalone Executable
```powershell
cargo run --package xtask -- dist
# Produces self-contained executable: dist/LatencyOS.exe
```

---

## 5. Directory Structure

```
LatencyOS/
├── kernel/                 # Kernel source (Rust no_std)
│   ├── src/
│   │   ├── main.rs         # Boot sequence & multi-core entry points
│   │   ├── shell.rs        # Pulse Shell with time-native prompt & ANSI parser
│   │   ├── editor.rs       # PulseEditor full-screen in-kernel text editor
│   │   ├── lang.rs         # PulseLang compiler & px64 v3 bytecode VM
│   │   ├── fs.rs           # LatencyFS static zero-allocation filesystem
│   │   ├── vfs.rs          # LatencyVFS GPU DMA Framebuffer VRAM disk (/vram)
│   │   ├── smp.rs          # APIC & multi-core initialization (Cores 0-3)
│   │   ├── ring_buffer.rs  # Lock-free SPSC cache-line aligned ring buffer
│   │   ├── e1000.rs        # Intel 82540EM poll-mode network driver (PMD)
│   │   ├── gpu.rs          # Zero-copy GPU frame capture & CRC32
│   │   ├── latency.rs      # Microsecond & nanosecond telemetry profiler
│   │   ├── tsc.rs          # Serialized TSC cycle timer & calibration
│   │   ├── serial.rs       # UART 16550 serial driver
│   │   ├── gdt.rs          # Global Descriptor Table setup
│   │   ├── idt.rs          # Interrupt Descriptor Table & exception handlers
│   │   └── pic.rs          # 8259 PIC disablement for APIC mode
├── runner/                 # Self-contained standalone executable loader
│   └── src/main.rs         # Embedded ZIP extractor & QEMU runner
├── dist/                   # Standalone distribution artifacts
│   └── LatencyOS.exe       # 100% portable Windows 11 standalone executable
├── xtask/                  # Native Rust build & test orchestration
│   └── src/main.rs         # Toolchain checks, QEMU runner, test harness
├── docs/                   # Documentation & formal specifications
│   ├── lang/               # PulseLang language specifications & ISA manuals
│   ├── ja/                 # Japanese architecture and documentation
│   └── superpowers/specs/  # Architectural design specifications
├── STATUS.md               # Project Phase 0-9 status & inventory report
└── architecture.md         # Detailed hardware & latency budget specification
```

---

## 6. プロジェクト状態一覧 (Project Status Inventory)

| フェーズ名 | 状態 | 備考 |
|---|---|---|
| **Phase 0: QEMU Core 0 起動 & シリアル出力** | **完了・実測検証済み** | QEMU COM1 UART（115200 baud）出力およびマイクロカーネル初期ブートシーケンス確認済み。 |
| **Phase 1: 静的コア割当 & TSC 精密タイマー** | **完了・実測検証済み** | APIC SMP 4コア静的アフィニティ起動、MSR C0ステートロック、シリアライズTSC校正確認済み。 |
| **Phase 2: GPU Capture ドメイン (Zero-Copy Ring)** | **完了・未検証** | GPU DMAフレームリング・CRC32整合性実装完了。ただしGPUパススルー非搭載のQEMU環境のため、実機GPU測定値ではなくエミュレータ上の値である点に留意。 |
| **Phase 3: ネットワークドメイン (e1000 PMD + SRTP)** | **完了・実測検証済み** | Intel 82540EM poll-modeドライバー、AES-NI / PCLMULQDQ ハードウェア暗号化パケット送出確認済み。 |
| **Phase 4: エンドツーエンド計測・統計レポート** | **完了・実測検証済み** | **【フェーズAにて是正完了】** 生ログ（9〜12ms）と1000サンプル集計統計表（p50 235us, p95 12.2ms, Max 18.2ms）の乖離原因を特定・解消し、QEMU制約下での真実の計測値を再提示。 |
| **Phase 5: 静的ファイルシステム (LatencyFS) & Pulse Shell** | **完了・実測検証済み** | 階層ディレクトリ操作（cd, ls, mkdir, cp, mv）、時間駆動型プロンプト（`[c0\|18ns] %`）、時限ガード（`within`）、VFS VRAMマウント（`/vram`）確認済み。 |
| **Phase 6: AIネイティブDSL (PulseLang) & 64-bit VM (`px64`)** | **完了・実測検証済み** | 20レジスタマシン、32-bit固定長命令、引数付きスクリプト実行（`run`）、逆アセンブラ（`disasm`）確認済み。 |
| **Phase 7: カーネル内蔵エディタ (PulseEditor) & AI診断** | **完了・実測検証済み** | ANSIフルスクリーンエディタ、Nano風ショートカットバー、UART高速ペースト、構造化エラー診断確認済み。 |
| **Phase 8: 実効時間ガード (Wall-Clock Watchdog) & 診断分離** | **完了・実測検証済み** | TSC 5.0ms壁時計タイムアウトガード（最悪保証5.48ms）、構文エラーと実行時エラーの診断テンプレート完全分離確認済み。 |
| **Phase 9: px64 ISA リファクタリング (定数プール & 即値演算)** | **完了・実測検証済み** | PX64 v3バイナリ、64-bit定数プール（`LDC`）、即値加減算（`ADDI`/`SUBI`）、定数プール境界防御、未登録オペコード検知、disasm仮想レジスタ注記、命令マイクロベンチマーク確認済み。 |
| **Phase 10: PulseLang v3 言語機能拡張 & 形式検証** | **完了・実測検証済み** | 10-1（静的forループ & WCET解析）、10-2（固定長配列・ビット演算・アサーション）、10-3（静的関数 & Result型）、10-4（構造体）、10-5（ROM定数表）、10-6（文字列比較）、10-7（定数畳み込み）、不変性デフォルト（`let mut`）、契約プログラミング（`@requires`）、網羅的パターンマッチング（`match`）確認済み。 |
| **Phase S: Export Disk (FAT16 輸出入専用ディスク & 常時自動同期・透過保存)** | **完了・実測検証済み** | ATA PIO LBA28ドライバ、FAT16 BPBパーサー、起動時自動インポート、Write-Through常時自動同期（CRUD連動）、Windows 11ホスト直接連携、コールドリブート永続化確認済み。 |

### 現時点で判明している制約・留意事項
1. **GPU / NVENC のエミュレーション制約**: GPUパススルー非搭載のQEMU環境で動作しているため、GPUキャプチャおよびNVENCエンコード区間は実機ハードウェアの実測ではなくソフトウェアエミュレーション/スタブ値である点。
2. **MSR / CPU温度データの読み取り制約**: QEMU環境においてMSRレジスタ読み取りが `0x0` となる箇所があり、実ハードウェアセンサー値ではない点。


