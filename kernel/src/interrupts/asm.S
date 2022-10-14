.text
.global vector_table
.balign 2048
vector_table:
mov x0, #1
b interrupt_print
.balign 0x80
mov x0, #2
b interrupt_print
.balign 0x80
mov x0, #3
b interrupt_print
.balign 0x80
mov x0, #4
b interrupt_print
.balign 0x80 // exception
b handler
.balign 0x80 // IRQ
mov x0, #6
b interrupt_print
.balign 0x80 // FIQ
mov x0, #7
b interrupt_print
.balign 0x80 // SError
mov x0, #8
b interrupt_print
.balign 0x80
mov x0, #9
b interrupt_print
.balign 0x80
mov x0, #10
b interrupt_print
.balign 0x80
mov x0, #11
b interrupt_print
.balign 0x80
mov x0, #12
b interrupt_print
.balign 0x80
mov x0, #13
b interrupt_print
.balign 0x80
mov x0, #14
b interrupt_print
.balign 0x80
mov x0, #15
b interrupt_print
.balign 0x80
mov x0, #16
b interrupt_print

handler:
sub sp, sp, #192
stp x0, x1, [sp, #0]
stp x2, x3, [sp, #16]
stp x4, x5, [sp, #32]
stp x6, x7, [sp, #48]
stp x8, x9, [sp, #64]
stp x10, x11, [sp, #80]
stp x12, x13, [sp, #96]
stp x14, x15, [sp, #112]
stp x16, x17, [sp, #128]
stp x18, x29, [sp, #144]
stp x30, xzr, [sp, #160]
mrs x0, ESR_EL1
mrs x1, FAR_EL1
stp x0, x1, [sp, #176]

mov x0, sp
bl exception_handler

b . // don't return