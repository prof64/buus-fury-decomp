@ Generic call stub, placed at 0x082ED300 (free ROM, patched in by the harness).
@ A BL in the main loop (0x08007052 -> 0x0800965c) is redirected here; we call
@ func(args...) and then tail-jump to the original target with original regs.
@ Args block at 0x082ED400: r0,r1,r2,r3, stack0..3, func, orig_target|1
    .syntax unified
    .thumb
    .global stub
stub:
    push {r0-r3}
    push {r4, r5, lr}
    sub sp, #16
    ldr r4, =0x082ED400
    ldr r0, [r4, #16]
    str r0, [sp]
    ldr r0, [r4, #20]
    str r0, [sp, #4]
    ldr r0, [r4, #24]
    str r0, [sp, #8]
    ldr r0, [r4, #28]
    str r0, [sp, #12]
    ldr r5, [r4, #32]
    ldr r0, [r4]
    ldr r1, [r4, #4]
    ldr r2, [r4, #8]
    ldr r3, [r4, #12]
    bl veneer
    ldr r1, =0x0203FFF0     @ result mailbox in EWRAM (top, assumed unused)
    str r0, [r1]
    add sp, #16
    pop {r4, r5}
    pop {r0}
    mov lr, r0
    ldr r0, =0x082ED400
    ldr r0, [r0, #36]
    mov r12, r0
    pop {r0-r3}
    bx r12
veneer:
    bx r5
    .ltorg
