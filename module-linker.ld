OUTPUT_FORMAT(elf64-aarch64)

SECTIONS {
    . = SIZEOF_HEADERS;

    .text ALIGN(4K) :
    {
        *(.text .text.*)
        *(.plt)
    }

    .rodata ALIGN(4K) :
    {
        *(.rodata .rodata.*)
    }

    .got ALIGN(4K) :
    {
        *(.got.*)
    }

    .data ALIGN(4K) :
    {
        *(.data .data.*)
    }

    .bss ALIGN(4K) :
    {
        *(.bss .bss.*)
    }

    .dynsym (INFO) : { *(.dynsym) }
    .dynstr (INFO) : { *(.dynstr) }
    .rela (INFO) : { *(.rela.*) }

    /DISCARD/ : { *(.comment) }
    /DISCARD/ : { *(.note.GNU-STACK) }
    /DISCARD/ : { *(.note.gnu-stack) }
    /DISCARD/ : { *(.hash) }
    /DISCARD/ : { *(.gnu.hash) }
    /DISCARD/ : { *(.dynamic) }
}
