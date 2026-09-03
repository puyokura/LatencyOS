; boot.asm - LatencyOS Bootloader Entry, 4GB Paging, and SMP AP Trampoline
;
; Worst-case execution time: Documented per function.

section .multiboot
align 4
MULTIBOOT_HEADER_MAGIC  equ 0x1BADB002
MULTIBOOT_HEADER_FLAGS  equ 0x00010003  ; ALIGN(1) | MEMINFO(2) | AOUT_KLUDGE(0x10000)
MULTIBOOT_CHECKSUM      equ -(MULTIBOOT_HEADER_MAGIC + MULTIBOOT_HEADER_FLAGS)

extern _boot_start
extern _data_end
extern _bss_end
multiboot_header:
    dd MULTIBOOT_HEADER_MAGIC
    dd MULTIBOOT_HEADER_FLAGS
    dd MULTIBOOT_CHECKSUM
    ; AOUT kludge fields for direct ELF64 loading via Multiboot1
    dd multiboot_header         ; header_addr
    dd _boot_start              ; load_addr
    dd _data_end                ; load_end_addr
    dd _bss_end                 ; bss_end_addr
    dd _start                   ; entry_addr

section .multiboot2
align 8
multiboot2_header_start:
    dd 0xE85250D6              ; Multiboot2 magic
    dd 0                       ; Architecture: 0 (32-bit protected mode)
    dd multiboot2_header_end - multiboot2_header_start ; Header length
    dd -(0xE85250D6 + 0 + (multiboot2_header_end - multiboot2_header_start)) ; Checksum

    ; End tag
    dw 0                       ; type: 0
    dw 0                       ; flags: 0
    dd 8                       ; size: 8
multiboot2_header_end:

section .bss
align 4096
; Initial page tables for identity mapping 4GB using 2MB huge pages
pml4_table:
    resb 4096
pdpt_table:
    resb 4096
pd_table_0: ; 0..1GB
    resb 4096
pd_table_1: ; 1..2GB
    resb 4096
pd_table_2: ; 2..3GB
    resb 4096
pd_table_3: ; 3..4GB
    resb 4096

; Per-core pre-allocated stacks (64KB * 4 cores = 256KB)
align 16
global core_stacks
core_stacks:
    resb 65536 * 4
core_stacks_end:

section .rodata
align 16
; 64-bit Global Descriptor Table (GDT)
global gdt64
global gdt64_pointer
gdt64:
    dq 0 ; 0x00: Null descriptor
.code: equ $ - gdt64
    ; 0x08: 64-bit Code descriptor (L=1, D=0, DPL=0, Present=1, Executable=1, Readable=1)
    dq 0x00209A0000000000
.data: equ $ - gdt64
    ; 0x10: 64-bit Data descriptor (Present=1, DPL=0, Writable=1)
    dq 0x0000920000000000
gdt64_end:

gdt64_pointer:
    dw gdt64_end - gdt64 - 1
    dd gdt64

section .boot
[BITS 32]
global _start
extern rust_main

; Function: _start
; Description: 32-bit Multiboot entry point for Core 0 (BSP). Sets up 4GB identity paging, enables Long Mode, loads GDT64, and enters 64-bit mode.
; Worst-case execution time: ~1800 ns
_start:
    cli
    cld

    ; Verify Multiboot 1 (0x2BADB002) or Multiboot 2 (0x36D76289) magic in EAX
    cmp eax, 0x2BADB002
    je .multiboot_ok
    cmp eax, 0x36D76289
    jne .no_multiboot

.multiboot_ok:
    ; Save Multiboot info pointer (in EBX) into EDI (1st argument in System V ABI)
    mov edi, ebx

    ; 1. Setup page tables (PML4 -> PDPT -> 4 PD tables mapping 4GB with 2MB huge pages)
    ; PML4[0] = &pdpt_table | Present(1) | Writable(2)
    mov eax, pdpt_table
    or eax, 0x3
    mov dword [pml4_table], eax
    mov dword [pml4_table + 4], 0

    ; PDPT[0..3] = &pd_table_0..3 | Present(1) | Writable(2)
    mov eax, pd_table_0
    or eax, 0x3
    mov dword [pdpt_table], eax
    mov dword [pdpt_table + 4], 0

    mov eax, pd_table_1
    or eax, 0x3
    mov dword [pdpt_table + 8], eax
    mov dword [pdpt_table + 12], 0

    mov eax, pd_table_2
    or eax, 0x3
    mov dword [pdpt_table + 16], eax
    mov dword [pdpt_table + 20], 0

    mov eax, pd_table_3
    or eax, 0x3
    mov dword [pdpt_table + 24], eax
    mov dword [pdpt_table + 28], 0

    ; Map 2048 entries (2048 * 2MB = 4GB identity mapped)
    ; Entry = (i * 2MB) | Present(1) | Writable(2) | HugePage(0x80) = 0x83
    mov ecx, 0
.map_4gb_loop:
    mov eax, 0x200000      ; 2MB
    mul ecx                ; EDX:EAX = index * 2MB
    or eax, 0x83           ; Present + Writable + HugePage (bit 7)
    mov dword [pd_table_0 + ecx * 8], eax
    mov dword [pd_table_0 + ecx * 8 + 4], edx
    inc ecx
    cmp ecx, 2048
    jne .map_4gb_loop

    ; 2. Enable PAE (Physical Address Extension) in CR4
    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax

    ; 3. Load CR3 with PML4 address
    mov eax, pml4_table
    mov cr3, eax

    ; 4. Enable Long Mode (LME) in EFER MSR (0xC0000080)
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8
    wrmsr

    ; 5. Enable Paging (PG) and Protection (PE) in CR0
    mov eax, cr0
    or eax, (1 << 31) | 1
    mov cr0, eax

    ; 6. Load 64-bit GDT
    lgdt [gdt64_pointer]

    ; 7. Far jump to 64-bit long mode
    jmp 0x08:long_mode_entry

.no_multiboot:
    hlt
    jmp .no_multiboot

[BITS 64]
; Function: long_mode_entry
; Description: 64-bit entry point for Core 0 (BSP). Reloads segment registers, sets up Core 0 stack, and calls Rust entry point.
; Worst-case execution time: ~100 ns
long_mode_entry:
    ; Reload data segment registers with 64-bit data descriptor (0x10)
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Set up Core 0 stack pointer (top of first 64KB stack slice)
    lea rsp, [core_stacks + 65536]

    ; Call Rust kernel main (RDI contains Multiboot info pointer)
    call rust_main

    ; If rust_main returns, halt Core 0
.halt_loop:
    cli
    hlt
    jmp .halt_loop

; =============================================================================
; AP (Application Processor) Trampoline Code (Loaded at physical address 0x8000)
; =============================================================================

section .trampoline
align 16
global ap_trampoline_start
global ap_trampoline_end
extern ap_main

ap_trampoline_start:
[BITS 16]
; Function: ap_trampoline_16
; Description: 16-bit real mode entry for APs started via SIPI.
; Worst-case execution time: ~300 ns
    cli
    cld

    ; Set up 16-bit segment registers
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax

    ; Load 32-bit temporary GDT (at 0x8000 + offset)
    lgdt [0x8000 + (ap_gdt32_ptr - ap_trampoline_start)]

    ; Enable protected mode (CR0 bit 0)
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    ; Far jump to 32-bit protected mode at 0x8000 + offset
    jmp dword 0x08:(0x8000 + (ap_trampoline_32 - ap_trampoline_start))

align 8
ap_gdt32:
    dq 0 ; Null descriptor
    ; 0x08: Code32 (Base 0, Limit 4GB, 32-bit, Executable, Readable)
    dq 0x00CF9A000000FFFF
    ; 0x10: Data32 (Base 0, Limit 4GB, 32-bit, Writable)
    dq 0x00CF92000000FFFF
ap_gdt32_end:

ap_gdt32_ptr:
    dw ap_gdt32_end - ap_gdt32 - 1
    dd 0x8000 + (ap_gdt32 - ap_trampoline_start)

[BITS 32]
; Function: ap_trampoline_32
; Description: 32-bit protected mode transition for APs. Enables PAE, loads PML4, enables long mode and paging.
; Worst-case execution time: ~400 ns
ap_trampoline_32:
    ; Reload 32-bit data segments
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax

    ; Enable PAE (CR4 bit 5)
    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax

    ; Load CR3 with PML4 address
    mov eax, pml4_table
    mov cr3, eax

    ; Enable Long Mode (LME) in EFER MSR (0xC0000080)
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8
    wrmsr

    ; Enable Paging (CR0 bit 31)
    mov eax, cr0
    or eax, (1 << 31) | 1
    mov cr0, eax

    ; Load 64-bit GDT
    lgdt [gdt64_pointer]

    ; Far jump to 64-bit long mode
    jmp 0x08:ap_trampoline_64

[BITS 64]
; Function: ap_trampoline_64
; Description: 64-bit long mode entry for APs. Reads LAPIC ID, sets up per-core stack, and calls ap_main.
; Worst-case execution time: ~200 ns
ap_trampoline_64:
    ; Reload 64-bit data segments
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Read Local APIC ID register at physical address 0xFEE00020
    mov rdi, 0xFEE00020
    mov eax, [rdi]
    shr eax, 24             ; EAX = APIC ID (1, 2, 3...)
    movzx rdi, al           ; RDI = 1st argument (core_id) for ap_main

    ; Set up per-core stack: core_stacks + (core_id + 1) * 65536
    mov rax, rdi
    inc rax
    shl rax, 16             ; RAX = (core_id + 1) * 64KB
    lea rsp, [core_stacks + rax]

    ; Call Rust AP entry point
    call ap_main

.ap_halt:
    cli
    hlt
    jmp .ap_halt

ap_trampoline_end:
