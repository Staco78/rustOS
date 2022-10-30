.section .text.exceptions

.macro HANDLER, name, stack_el
sub sp, sp, #272

stp x0, x1, [sp, #16 * 0]
stp x2, x3, [sp, #16 * 1]
stp x4, x5, [sp, #16 * 2]
stp x6, x7, [sp, #16 * 3]
stp x8, x9, [sp, #16 * 4]
stp x10, x11, [sp, #16 * 5]
stp x12, x13, [sp, #16 * 6]
stp x14, x15, [sp, #16 * 7]
stp x16, x17, [sp, #16 * 8]
stp x18, x19, [sp, #16 * 9]
stp x20, x21, [sp, #16 * 10]
stp x22, x23, [sp, #16 * 11]
stp x24, x25, [sp, #16 * 12]
stp x26, x27, [sp, #16 * 13]
stp x28, x29, [sp, #16 * 14]
mrs x0, SP_EL\stack_el
stp x30, x0, [sp, #16 * 15]
mrs x0, ELR_EL1
mrs x1, SPSR_EL1
stp x0, x1, [sp, #16 * 16]

mov x0, sp
.global \name
// the high-level handler return a *mut InterruptFrame that we use to eret 
bl \name

b exception_exit
.endm

.global vector_table
.balign 2048
vector_table:

.org 0x0000
HANDLER exception_handler, 0

.org 0x0080
HANDLER interrupt_handler, 0

.org 0x0100
mov x0, #3
b interrupt_print

.org 0x0180
mov x0, #4
b interrupt_print


.org 0x0200 // exception
HANDLER exception_handler, 1

.org 0x0280 // IRQ
mov x0, #6
b interrupt_print

.org 0x0300 // FIQ
mov x0, #7
b interrupt_print

.org 0x0380 // SError
mov x0, #8
b interrupt_print


.org 0x0400
mov x0, #9
b interrupt_print

.org 0x0480
mov x0, #10
b interrupt_print

.org 0x0500
mov x0, #11
b interrupt_print

.org 0x0580
mov x0, #12
b interrupt_print


.org 0x0600
mov x0, #13
b interrupt_print

.org 0x0680
mov x0, #14
b interrupt_print

.org 0x0700
mov x0, #15
b interrupt_print

.org 0x0780
mov x0, #16
b interrupt_print


// x0: *mut InterruptFrame: restore context
.global exception_exit
exception_exit:
mov sp, x0

ldp x0, x1, [sp, #16 * 16]
msr ELR_EL1, x0
msr SPSR_EL1, x1
ldp x30, x0, [sp, #16 * 15]
// TODO: use SP_EL1 when needed (check pstate)
msr SP_EL0, x0
ldp x28, x29, [sp, #16 * 14]
ldp x26, x27, [sp, #16 * 13]
ldp x24, x25, [sp, #16 * 12]
ldp x22, x23, [sp, #16 * 11]
ldp x20, x21, [sp, #16 * 10]
ldp x18, x19, [sp, #16 * 9]
ldp x16, x17, [sp, #16 * 8]
ldp x14, x15, [sp, #16 * 7]
ldp x12, x13, [sp, #16 * 6]
ldp x10, x11, [sp, #16 * 5]
ldp x8, x9, [sp, #16 * 4]
ldp x6, x7, [sp, #16 * 3]
ldp x4, x5, [sp, #16 * 2]
ldp x2, x3, [sp, #16 * 1]
ldp x0, x1, [sp, #16 * 0]

add sp, sp, #272

eret