// x0 is a ptr to a StartInfos struct:
ap_start:
ldp x1, x2, [x0, #8]
msr TTBR0_EL1, x1
msr TTBR1_EL1, x2

ldp x1, x2, [x0, #16 * 1 + 8] 
mov sp, x1 // set the stack ptr
msr VBAR_EL1, x2

ldp x1, x2, [x0, #16 * 2 + 8]
msr MAIR_EL1, x1
msr TCR_EL1, x2

ldr x1, [x0, #16 * 3 + 8]
msr SCTLR_EL1, x1

ldr w4, [x0] // store our id in w4

mov w5, #1
str w5, [x0, #4] // informs the main core that we have started: set has_started to true
// the ptr in x0 is invalid after here bc could have dropped the struct

ldr x1, [x0, #0x40]

mov w0, w4
br x1