ENTRY(kernel_entry)
SECTIONS {
    . = 0x10000;
    .text : { *(.text.entry) *(.text*) }
    .rodata : { *(.rodata*) }
    .data : { *(.data*) }
    .bss : { __bss_start = .; *(.bss) *(.bss.*) *(COMMON) *(.sbss) *(.sbss.*) __bss_end = .; }
    /DISCARD/ : { *(.eh_frame*) *(.comment) }
}
