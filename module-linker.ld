OUTPUT_FORMAT(elf64-aarch64)

SECTIONS {
    . = SIZEOF_HEADERS;

    .text ALIGN(4K) :
    {
        *(.text .text.*)
        *(.plt)
    }

    .got ALIGN(4K) :
    {
        *(.got.*)
    }

    .rodata ALIGN(4K) :
    {
        *(.rodata .rodata.*)
    }

    .dynsym : { *(.dynsym) }
    .dynstr : { *(.dynstr) }

    .data ALIGN(4K) :
    {
        *(.data .data.*)
    }

    .bss ALIGN(4K) :
    {
        *(.bss .bss.*)
    }

    /DISCARD/ : { *(.comment) }
    /DISCARD/ : { *(.note.GNU-STACK) }
    /DISCARD/ : { *(.note.gnu-stack) }
    /DISCARD/ : { *(.hash) }
    /DISCARD/ : { *(.gnu.hash) }
    /DISCARD/ : { *(.dynamic) }
}
