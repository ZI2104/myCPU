# NPU_SPEC v1 — Descriptor ABI

> 本文档定义 NPU 的稳定 ABI 与行为语义。实现可演进，但不得破坏本规范中的兼容性约束。

## 1. 版本与范围

- 版本：v1
- 范围：descriptor 布局、MMIO 寄存器、vector-mode 语义、错误语义与软异步行为。
- 兼容性原则：v1 descriptor 固定为 16 字节，不引入破坏性字段变更。

## 2. Descriptor ABI（16B）

每个 descriptor 固定 16 字节，字段按顺序为 little-endian `u32`：

1. `opcode`
2. `op_a_addr`：operand A 基地址（数组或标量）
3. `op_b_addr`：operand B 基地址（数组或标量）
4. `result_addr`：结果写回基地址（数组或标量）

## 3. MMIO 寄存器语义

- `REG_DESC_ADDR_LOW` / `REG_DESC_ADDR_HIGH`：descriptor 基地址（组合为 `u64`）
- `REG_DESC_LEN`：
  - vector-mode：表示 `element_count`（元素数量）
  - scalar-mode：保持既有兼容语义（descriptor 数量）
- `REG_DESC_NOTIFY`：写入非零值设置 pending notify，并触发后续处理
- `REG_TASKS_DONE` / `REG_TASKS_ERROR`：完成任务数 / 错误任务数统计

## 4. Vector-mode 约定

- 模式标识：`VECTOR_FLAG = 0x1000`
- 判定规则：当 `(opcode & VECTOR_FLAG) != 0` 时进入 vector-mode
- 算子编码：`opcode & 0xFFF`（例如 `0=Add`, `1=Mul`, `2=Max`, `3=Relu`）
- 元素宽度：初始实现为 32-bit（`u32`），默认步长 `stride = 4` 字节
- 地址语义：`op_a_addr / op_b_addr / result_addr` 在 vector-mode 下均视为数组基地址

## 5. 错误语义

- 对齐错误或越界错误：
  - 当前 descriptor 计为错误（`REG_TASKS_ERROR` 增加）
  - 调用方可根据状态寄存器与中断策略执行恢复流程
- `element_count == 0`：视为 no-op（记录错误或忽略，按实现文档与调用约定）

## 6. 对齐与异常（详细）

- 元素宽度为 4 字节，故以下地址必须满足 4 字节对齐：
  - `op_a_addr + i*4`
  - `op_b_addr + i*4`
  - `result_addr + i*4`
- 当前实现（`src/peripheral/npu.rs`）在不对齐时返回 `MemoryAlignment` 并计入 `REG_TASKS_ERROR`。
- 越界访问返回 `MemoryOutOfBounds` 并计入 `REG_TASKS_ERROR`。
- 建议软件端在构造 descriptor 前预检基地址与长度，减少运行时失败。

## 7. 算子语义（VMul / VMax / VRelu）

- `VMul`（`opcode & 0xFFF == 1`）：`res[i] = a[i] * b[i]`，当前实现使用 `u32::wrapping_mul`。
- `VMax`（`opcode & 0xFFF == 2`）：`res[i] = max(a[i], b[i])`（无符号比较）。
- `VRelu`（`opcode & 0xFFF == 3`）：`res[i] = (a[i] as i32) < 0 ? 0 : a[i]`。

> 说明：当前实现采用 wrapping 语义以保证定义性并避免 panic；如后续引入饱和算术，需在版本变更中显式声明。

## 8. 软异步（soft-asynchronous / slice-based）

为在单线程仿真中模拟“异步 DMA 体验”，提供软异步模式：

- `REG_DESC_NOTIFY` 后先入队，不要求立即完整执行。
- 通过 `Npu::tick(ram_regions, budget)` 分片推进，每次最多处理 `budget` 个元素。
- 由 `Npu::set_soft_async_budget(Some(b))` 开启；默认保持同步行为以兼容既有测试。

优点：

- 在仿真层支持 CPU/NPU 交错推进，提升可观测性并保留可复现性。

边界：

- 仍为模拟异步（非真实并发）；若升级为线程化 worker，需要额外处理并发一致性与可复现性。

## 9. 与 Hybrid Offload 的边界

- `docs/HYBRID_OFFLOAD.md` 中的 `ASYNC_FLAG/WIDE_FLAG/STRIDE_FLAG` 属于演进提案。
- 在未升级到新 ABI 版本前，本规范仍以 v1（16B descriptor）为稳定基线。
- 后续扩展应优先采用向后兼容方式，避免破坏既有测试与工具链。

## 10. 相关文档

- `docs/NPU_OVERVIEW.md`（快速入口）
- `docs/NPU_INTEGRATION_SUMMARY.md`（集成复盘）
- `docs/HYBRID_OFFLOAD.md`（演进草案）
