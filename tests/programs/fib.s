# Minimal RV32I Fibonacci demo for GDB/perf showcase
# - defines `main` symbol for convenient breakpointing
# - prints "FIB\n" to UART
# - exits via Linux-style ECALL (a7=93), intended to pair with --ecall-exit

.equ UART_BASE, 0x10000000

.section .text
.globl _start
.globl main

_start:
    call    main

    # exit(0)
    li      a0, 0
    li      a7, 93
    ecall

main:
    # Fibonacci iteration count
    li      t2, 24

    # a=0, b=1
    li      t0, 0
    li      t1, 1

fib_loop:
    add     t3, t0, t1
    mv      t0, t1
    mv      t1, t3
    addi    t2, t2, -1
    bnez    t2, fib_loop

    # store result to memory for debugger inspection
    la      t4, fib_result
    sw      t1, 0(t4)

    # UART print "FIB\n"
    lui     t5, 0x10000
    li      t6, 'F'
    sb      t6, 0(t5)
    li      t6, 'I'
    sb      t6, 0(t5)
    li      t6, 'B'
    sb      t6, 0(t5)
    li      t6, '\n'
    sb      t6, 0(t5)

    ret

.section .data
.align 2
fib_result:
    .word 0
