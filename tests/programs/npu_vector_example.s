.equ NPU_BASE, 0x20000000
.equ REG_DESC_ADDR_LOW, 0x20
.equ REG_DESC_LEN, 0x28
.equ REG_DESC_NOTIFY, 0x2c
.equ REG_TASKS_DONE, 0x30
.equ VECTOR_FLAG, 0x1000
.equ NPU_OP_ADD, 0

    .section .data
    .align 2
op_a:
    .word 1,2,3,4
op_b:
    .word 10,20,30,40
out:
    .space 16
desc:
    .word (NPU_OP_ADD | VECTOR_FLAG)
    .word op_a
    .word op_b
    .word out

    .section .text
    .globl _start
_start:
    la t0, desc
    la t1, op_a
    la t2, op_b
    la t3, out

    # Write descriptor address low
    li t4, (NPU_BASE + REG_DESC_ADDR_LOW)
    sw t0, 0(t4)

    # Write descriptor element count
    li t4, (NPU_BASE + REG_DESC_LEN)
    li t5, 4
    sw t5, 0(t4)

    # Notify NPU
    li t4, (NPU_BASE + REG_DESC_NOTIFY)
    li t5, 1
    sw t5, 0(t4)

wait:
    # Poll tasks_done
    li t4, (NPU_BASE + REG_TASKS_DONE)
    lw t5, 0(t4)
    beqz t5, wait

done:
    j done
