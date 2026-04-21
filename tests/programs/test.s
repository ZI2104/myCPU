# Minimal deterministic RV32I workload for DiffTest demo
# This program does not rely on syscalls. Pair with --count for bounded runs.

.section .text
.globl _start
.globl main

_start:
    j       main

main:
    li      t0, 0x1234
    li      t1, 0x5678
    li      t2, 0x0080

mix_loop:
    add     t0, t0, t1
    xor     t1, t1, t0
    slli    t1, t1, 1
    addi    t2, t2, -1
    bnez    t2, mix_loop

spin:
    addi    t3, t3, 1
    xor     t3, t3, t0
    j       spin
