SECTIONS {
    . = 0xFFFFFF8000000000;
    .text :
    {
        *(.text .text.*)
    }
    .data :
    {
        *(.data .data.*)
        *(.rodata .rodata.*)
        *(.trampoline)
    }
    .bss :
    {
        *(.common)
        *(.bss)
    }
}
