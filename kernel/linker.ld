ENTRY(_start)

PHDRS {
    kernel PT_LOAD FLAGS(7); /* Read(4) | Write(2) | Execute(1) */
}

SECTIONS {
    . = 1M;
    _boot_start = .;

    .boot : ALIGN(4K) {
        KEEP(*(.multiboot))
        KEEP(*(.multiboot2))
        *(.boot)
        *(.trampoline)
    } :kernel

    .text : ALIGN(4K) {
        *(.text .text.*)
    } :kernel

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    } :kernel

    .data : ALIGN(4K) {
        *(.data .data.*)
        *(.got)
        *(.got.pult)
        *(.sdata .sdata.*)
    } :kernel
    _data_end = .;

    .bss : ALIGN(4K) {
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
    } :kernel
    _bss_end = .;

    /DISCARD/ : {
        *(.comment)
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.note.*)
    }
}