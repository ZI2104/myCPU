#include <stddef.h>

/* Minimal typedefs to avoid requiring newlib headers in cross toolchain */
typedef unsigned int uint32_t;
typedef unsigned int uintptr_t;

#define NPU_BASE 0x20000000u
#define REG_DESC_ADDR_LOW 0x20
#define REG_DESC_LEN 0x28
#define REG_DESC_NOTIFY 0x2C
#define REG_TASKS_DONE 0x30
#define VECTOR_FLAG 0x1000
#define NPU_OP_ADD 0u

volatile uint32_t desc[4];
volatile uint32_t op_a[4] = {1, 2, 3, 4};
volatile uint32_t op_b[4] = {10, 20, 30, 40};
volatile uint32_t out[4];

static inline void mmio_write32(uint32_t addr, uint32_t value) {
    *(volatile uint32_t *)(uintptr_t)(addr) = value;
}

static inline uint32_t mmio_read32(uint32_t addr) {
    return *(volatile uint32_t *)(uintptr_t)(addr);
}

void _start() {
    // Compose descriptor (opcode | VECTOR_FLAG, op_a_addr, op_b_addr, result_addr)
    desc[0] = (uint32_t)(NPU_OP_ADD | VECTOR_FLAG);
    desc[1] = (uint32_t)(uintptr_t)op_a;
    desc[2] = (uint32_t)(uintptr_t)op_b;
    desc[3] = (uint32_t)(uintptr_t)out;

    // Write descriptor address and element count
    mmio_write32(NPU_BASE + REG_DESC_ADDR_LOW, (uint32_t)(uintptr_t)desc);
    mmio_write32(NPU_BASE + REG_DESC_LEN, 4u);

    // Notify NPU
    mmio_write32(NPU_BASE + REG_DESC_NOTIFY, 1u);

    // Wait for completion
    while (mmio_read32(NPU_BASE + REG_TASKS_DONE) == 0u) {
        ; // spin
    }

    // Exit via ecall (Linux exit syscall)
    asm volatile("li a7, 93\nli a0, 0\necall");

    // Should not reach here; spin to be safe
    for (;;) {
        asm volatile("wfi");
    }
}
