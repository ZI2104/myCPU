# NPU_SPEC v1 — Descriptor ABI

概述

- descriptor 大小：16 字节（每 entry）
- descriptor 字段（按顺序，均为 little-endian u32）：
  1. opcode (u32)
  2. op_a_addr (u32)   ; guest physical base address of operand A (array or scalar)
  3. op_b_addr (u32)   ; guest physical base address of operand B (array or scalar)
  4. result_addr (u32) ; guest physical base address for result (array or scalar)

寄存器（MMIO）

- `REG_DESC_ADDR_LOW` / `_HIGH` : descriptor 基地址（u64）
- `REG_DESC_LEN` : 当 descriptor 表示 vector-mode 时，表示 element_count（u32）；对 scalar descriptor 语义保持兼容（表示 descriptor 数量）
- `REG_DESC_NOTIFY` : 写非零值会 set pending notify（并在 Bus/driver 侧触发处理）
- `REG_TASKS_DONE` / `REG_TASKS_ERROR` : 统计完成任务/错误计数

Vector-mode 约定

- 使用 opcode 高位作为 vector 模式标识：
  - `VECTOR_FLAG = 0x1000`
  - 当 `(opcode & VECTOR_FLAG) != 0` 时，解释为 vector-mode
  - 实际操作类型由 `opcode & 0xFFF` 指定（例如 0=Add,1=Mul,2=Max,3=Relu）
- 元素宽度（初始实现）：32-bit（u32），stride = 4 字节
- `REG_DESC_LEN` 在 vector-mode 下表示 element_count（元素数）
- Descriptor 的 `op_a_addr/op_b_addr/result_addr` 被视为数组基地址

错误语义

- 越界或对齐错误：对应 descriptor 导致 `tasks_error` 增加，current descriptor 跳过或产生定义的错误结果（实现细节在实现中说明）
- descriptors 中的 element_count 为 0：视为无-op（记录 tasks_error 或忽略，按实现文档定义）

性能建议

- 推荐把大量元素放在连续内存（对齐到 4 字节）并尽量增大单个 descriptor 的 element_count 以摊薄启动开销
