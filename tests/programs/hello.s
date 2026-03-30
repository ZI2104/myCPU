# Simple RISC-V program that writes "Hello" to UART
# UART base address: 0x10000000 (QEMU virt machine)

.equ UART_BASE, 0x10000000
.equ UART_THR,  0x00    # Transmit Holding Register

.section .text
.globl _start

_start:
    # Load UART base address
    lui     t0, 0x10000       # t0 = 0x10000000

    # Print 'H'
    li      t1, 'H'
    sb      t1, 0(t0)

    # Print 'e'
    li      t1, 'e'
    sb      t1, 0(t0)

    # Print 'l'
    li      t1, 'l'
    sb      t1, 0(t0)

    # Print 'l'
    li      t1, 'l'
    sb      t1, 0(t0)

    # Print 'o'
    li      t1, 'o'
    sb      t1, 0(t0)

    # Print '\n'
    li      t1, '\n'
    sb      t1, 0(t0)

    # Exit via ecall
    li      a7, 93            # syscall: exit
    li      a0, 0             # exit code: 0
    ecall

.section .bss
.section .data
