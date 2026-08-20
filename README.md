# LatencyOS

> **A Hard Real-Time, Deterministic Low-Latency Operating System Engineered from Scratch in Rust (`no_std`) for Ultra-Low-Latency Streaming Pipelines.**

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
- **Deadline Guard (`within <time> <cmd>`)**: Evaluates real-time execution against strict hardware budgets (e.g. `within 500us run filter.pl`).
- **Hardware Telemetry Commands**:
  - `timeline`: Monospace microsecond breakdown of the 6 pipeline stages.
  - `ring`: Real-time inspection of SPSC lock-free queues (occupancy, head/tail pointers).
  - `cores`: APIC ID, core role, C0 state locks, and iteration metrics.
  - `tsc`: Raw hardware TSC timestamp and cycle resolution.
  - `ls -t`: File listing with worst-case execution time (WCET) budgets.
  - `doc pulse`: In-kernel formal specification of PulseLang v2.
  - `exit` / `poweroff`: ACPI hardware shutdown.

### 3.2 PulseLang v2 (AI-Native Temporal Reactive DSL)
- Mathematically dense, AI-optimized grammar with first-class time literals (`50ns`, `200us`, `5ms`, `1s`).
- Direct register bindings (`$rtt`, `$sum`), hardware handles (`#f`), and compiler contracts (`@contract: @wcet(5us) @budget(50us);`).
- Zero-copy stream piping (`|>`) and deadline assertions (`@within(500us) { ... } !drop;`).
- Standard scripts: `stream.pl`, `bench.pl`, `filter.pl`.

### 3.3 PulseEditor (In-Kernel ANSI Text Editor)
- Full-screen terminal text editor running inside the kernel on Core 0.
- Real-time ANSI syntax highlighting for PulseLang tokens, directives, and numbers.
- Smart word jumping (`Ctrl+Left` / `Ctrl+Right`), full cursor tracking, and one-key in-kernel compilation (`Ctrl+R`).

### 3.4 LatencyFS (Static Real-Time Filesystem)
- In-memory static filesystem with zero fragmentation and fixed memory layout.
- Files stored as fixed-size blocks with instant $O(1)$ lookup.

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

---

## 5. Directory Structure

```
LatencyOS/
├── kernel/                 # Kernel source (Rust no_std)
│   ├── src/
│   │   ├── main.rs         # Boot sequence & multi-core entry points
│   │   ├── shell.rs        # Pulse Shell with time-native prompt & ANSI parser
│   │   ├── editor.rs       # PulseEditor full-screen in-kernel text editor
│   │   ├── lang.rs         # PulseLang v2 compiler & bytecode VM
│   │   ├── fs.rs           # LatencyFS static zero-allocation filesystem
│   │   ├── smp.rs          # APIC & multi-core initialization (Cores 0-3)
│   │   ├── ring_buffer.rs  # Lock-free SPSC cache-line aligned ring buffer
│   │   ├── e1000.rs        # Intel 82540EM poll-mode network driver (PMD)
│   │   ├── gpu.rs          # Zero-copy GPU frame capture & CRC32
│   │   ├── latency.rs      # Microsecond & nanosecond telemetry profiler
│   │   ├── tsc.rs          # Serialized TSC cycle timer & calibration
│   │   └── serial.rs       # UART 16550 serial driver
├── xtask/                  # Native Rust build & test orchestration
│   └── src/main.rs         # Toolchain checks, QEMU runner, Win32 console handler
├── docs/                   # Documentation & formal specifications
│   ├── pulselang.md        # PulseLang v2 language manual & grammar
│   └── superpowers/specs/  # Architectural design specifications
└── architecture.md         # Detailed hardware & latency budget specification
```
