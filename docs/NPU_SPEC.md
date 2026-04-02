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

对齐与异常语义（详细）
-----------------------
- 元素宽度为 32-bit（4 字节），因此所有 element 的地址（包括 `op_a_addr + i*4`, `op_b_addr + i*4`, `result_addr + i*4`）必须满足 4 字节对齐。
- 在当前实现中（`src/peripheral/npu.rs`）：如果在处理 descriptor 时检测到不满足 4 字节对齐，将返回 `MemoryAlignment` 错误，调用方会把该 descriptor 标记为错误并增加 `REG_TASKS_ERROR`。
- 对于越界（访问超出已映射或 RAM 区域）会返回 `MemoryOutOfBounds`，并同样计入 `tasks_error`，以便软件能够检测并恢复。
- 推荐软件端在构造 descriptor 前自行验证基地址与长度以避免运行时错误。

算子细节（VMul / VMax / VRelu）
-------------------------------
- VMul: 对应 `opcode & 0xFFF == 1`，对每个元素执行 `res[i] = a[i] * b[i]`，按无符号或有符号语义取决于具体 ABI（当前实现使用 u32 wrapping_mul）。
- VMax: 对应 `opcode & 0xFFF == 2`，对每个元素执行 `res[i] = max(a[i], b[i])`（按无符号比较）。
- VRelu: 对应 `opcode & 0xFFF == 3`，对每个元素执行 `res[i] = (a[i] as i32) < 0 ? 0 : a[i]`（按 32-bit 有符号解释输入）。

注意：算子实现必须考虑溢出、饱和语义（如需）以及输入解释（有符号/无符号），目前实现使用了 wrapping 算法以保持定义性并避免 panic。

软异步（soft-asynchronous / slice-based）处理说明
-------------------------------------------------
- 为了在模拟器中获得“异步 DMA”行为但保留单线程的确定性，仓库提供了一种软异步模式：
  - 描述符在 `REG_DESC_NOTIFY` 写操作时被入队（enqueue），而不立即全部执行。
  - 仿真主循环或测试代码可以通过调用 `Npu::tick(ram_regions, budget)` 来按 slice 处理入队工作，每次最多处理 `budget` 个元素。
  - 该模式由 `Npu::set_soft_async_budget(Some(b))` 开启（`b` 为每次 tick 的元素预算）；默认保持同步行为以兼容现有测试。
- 优点：允许 CPU 步骤与 NPU 工作在仿真中交错，从而模拟并行执行和重叠传输/计算的效果，同时保持单线程可重复性。
- 缺点：仍为模拟（非真实并行），需要软件轮询或等待中断；如果想要真实线程并行，需要引入线程/锁与内存一致性开销。

改进项记录
---------------
- 若后续想在仿真中更真实地模拟 DMA 并行性，可将 soft-async 扩展为真实后台 worker（线程），但需注意：
  - `Memory`/`Ram` 类型需为 `Send + Sync` 或通过 `Arc<Mutex<_>>` 包装；
  - 需实现内存所有权/屏障机制以避免竞态；
  - 测试复现性会受线程调度影响，需要引入 deterministic scheduler 或记录/回放机制。

以上改动已在实现中加入基本的对齐检查与软异步支持，并在单元测试中验证典型错误路径与分片处理行为。
