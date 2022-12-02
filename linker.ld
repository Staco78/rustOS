ENTRY(start)
OUTPUT_FORMAT(elf64-aarch64)

KERNEL_OFFSET = 0xFFFFFFFF80000000; # -2GB

SECTIONS {
    . = KERNEL_OFFSET;

    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_OFFSET)
    {
        *(.text .text.*)
    }

    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_OFFSET)
    {
        *(.rodata .rodata.*)
    }

    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_OFFSET)
    {
        *(.data .data.*)
    }

    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_OFFSET)
    {
        *(.bss .bss.*)
    }

    .dynsym ALIGN(4K) (INFO) : { *(.dynsym) }
    .dynstr (INFO) : { *(.dynstr) }
    
    .defs_exports (INFO) : 
    {
         *(.defs_exports)
         KEEP(*(.defs_exports))
    }

     .sym_exports (INFO) : 
    {
         *(.sym_exports)
         KEEP(*(.sym_exports))
    }

    /DISCARD/ : { *(.comment) }
    /DISCARD/ : { *(.note.GNU-STACK) }
    /DISCARD/ : { *(.note.gnu-stack) }
    /DISCARD/ : { *(.hash) }
    /DISCARD/ : { *(.gnu.hash) }
    /DISCARD/ : { *(.dynamic) }
}
