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
    }
    .bss :
    {
        *(.common)
        *(.bss)
    }
}
