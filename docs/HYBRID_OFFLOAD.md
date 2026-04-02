# Hybrid Offload Design (草案)

目标

- 在运行时根据工作负载决定是把向量/批量算子放到 CPU 本地执行还是下发到 NPU 加速器。
- 提供回退路径（NPU 不可用或失败时在 CPU 上执行），并考虑异步 DMA 路径以减少主 CPU 阻塞。

关键点

1. 决策维度
   - 操作种类（支持的算子集合，如 Add/Mul/Max/Relu）
   - 数据大小（元素数）和对齐情况
   - 数据布局（连续/非连续，stride）
   - 运行时延迟预算（短任务倾向 CPU，长批量任务倾向 NPU）

2. ABI 与运行时元数据
   - 保持当前 `NPU_SPEC.md` 的 descriptor ABI（16 字节）兼容
   - 扩展 descriptor 标志位用于：异步 DMA（ASYNC_FLAG）、64-bit 元素（WIDE_FLAG）、strided loads（STRIDE_FLAG）等

3. 同步 vs 异步 路径
   - 目前实现：写 MMIO -> Bus 同步触发 `process_pending_descriptor_notify`（阻塞路径）
   - 异步改造提案：当设置 ASYNC_FLAG 时，MMIO 触发将 descriptor 放入后台工作队列（内存或 host 线程），并由背景 worker 处理并通过中断/事件完成通知
   - 后台 worker 可以是：Rust 线程（std::thread）或 tokio/runtime 任务（如果项目引入 async），优先选择简单的 std::thread + crossbeam::channel

4. 错误与回退
   - NPU 在执行时检测到越界/对齐错误应将 `tasks_error` 加 1 并在 REG_STATUS 或通过 IRQ 报告错误码
   - 运行时库（guest 或宿主驱动）应监控错误并在必要时将工作回退到 CPU 实现

5. 性能注意事项
   - 批量长度应足够大以摊薄启动延迟（建议阈值：>128 元素，需测量）
   - 使用连续缓冲区并对齐到 4 字节以获得最佳内存带宽
   - 在异步路径上合并小批次以提高吞吐

6. 下一步实现计划（最小可行设计）
   - Phase A: 保持同步实现不变，新增 ASYNC_FLAG（只是一个标志）并在文档/ABI 中说明
   - Phase B: 实现背景 worker：在 Bus 写入激活描述符时，若 ASYNC_FLAG 被设置，将 descriptor 推送到后台队列并立即返回；worker 处理并在完成时写入 REG_TASKS_DONE/REG_STATUS，并触发 IRQ（set IRQ_PENDING）
   - Phase C: 性能基准与调整：对不同批量大小进行基准，确定阈值与合并策略

7. 兼容性与回滚
   - 保持原有 descriptor 的语义不变（默认同步处理）
   - 使用新标志位进行向后兼容扩展


