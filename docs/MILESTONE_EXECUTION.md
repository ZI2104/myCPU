# OS 里程碑执行记录（逐项验收 + 上下文压缩）

> 目标：按 `docs/ROADMAP.md` 中的“OS Bring-up 里程碑（用户目标对齐）”逐项推进。  
> 规则：每完成一项，必须补充“完成记录 + 测试验收 + 上下文压缩”。

## 记录模板

### [日期] [阶段-条目]

- 完成内容：
- 变更文件：
- 验收命令：
- 验收结果：
- 风险/未完成项：
- 上下文压缩（供下一步直接续做）：

---

## 执行记录

### 2026-04-01 Phase-META-01（里程碑落档）

- 完成内容：
  - 将用户要求的 Phase 0~6 目标与当前真实完成状态写入里程碑主文档。
  - 明确“每项完成后必须记录 + 压缩上下文 + 验收”的执行约定。
  - 修正 `ROADMAP` 中 P1 RV32M 状态为已完成。
- 变更文件：
  - `docs/ROADMAP.md`
  - `docs/MILESTONE_EXECUTION.md`（本文件）
- 验收命令：
  - `cargo test --test bringup_smoke`
  - `cargo test --lib`
- 验收结果：
  - 通过：`bringup_smoke` 3/3 通过；库回归 217/217 通过。
- 风险/未完成项：
  - 后续仍需按阶段推进功能落地，不可仅停留在文档状态。
- 上下文压缩（供下一步直接续做）：
  - Phase 0 的“里程碑落档”与“测试入口”已落地，下一项进入 Phase 2 的 VirtIO-Block skeleton。

### 2026-04-01 Phase-0-01（OS bring-up 专项测试入口）

- 完成内容：
  - 新增集成测试 `tests/bringup_smoke.rs`，作为 OS bring-up 验收入口。
  - 覆盖三条关键路径：
    - 引导 stub 指令执行（寄存器与 PC 演进正确）；
    - `ecall` 默认陷入 M 态；
    - `ecall`(U) 在 `medeleg.UECL` 使能时委托至 S 态。
- 变更文件：
  - `tests/bringup_smoke.rs`
- 验收命令：
  - `cargo test --test bringup_smoke`
  - `cargo test --lib`
- 验收结果：
  - 通过：bring-up 3/3；库回归 217/217。
- 风险/未完成项：
  - 目前为 CPU 级 smoke，不含真实 xv6 镜像启动路径和块设备依赖。
- 上下文压缩（供下一步直接续做）：
  - 下个最小可交付项：Phase 2 的 VirtIO-Block skeleton（先 MMIO 框架 + 最小读请求通路）。

### 2026-04-01 Phase-2-01（VirtIO-Block 最小骨架）

- 完成内容：
  - 新增 `src/peripheral/virtio_block.rs`，实现最小 VirtIO-Block MMIO 骨架。
  - 支持基本寄存器（identity/status/sector/command/result/control）与 512B 数据窗口。
  - 支持最小命令闭环：`READ_SECTOR`、`WRITE_SECTOR`，并提供中断挂起/确认接口。
  - 默认启动总线挂载 VirtIO-Block 设备（`create_bus`）。
- 变更文件：
  - `src/peripheral/virtio_block.rs`
  - `src/peripheral/mod.rs`
  - `src/main.rs`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo test virtio_block::tests --lib`
  - `cargo test --lib`
- 验收结果：
  - 通过：VirtIO-Block 专项 3/3；库回归 220/220。
- 风险/未完成项：
  - 当前为“命令寄存器 + 数据窗口”骨架，尚未接入标准 VirtIO descriptor queue。
  - 仍未打通 xv6 文件系统镜像与 shell 交互路径。
- 上下文压缩（供下一步直接续做）：
  - 下一项优先：descriptor queue（desc/avail/used）最小实现，替换当前数据窗口命令模型。

### 2026-04-01 Phase-2-02（VirtIO 最小队列闭环）

- 完成内容：
  - 在 `VirtioBlock` 中新增最小队列相关寄存器：queue addr/num/ready/notify、avail/used idx、queue head、last used head。
  - 新增 queue-notify 处理路径：根据 `req_type + sector + data_window` 执行读写请求，并推进 `used_idx` 与 `used_ring`。
  - 修复两处关键问题：
    - 字节写入导致 notify/command 被重复触发（改为仅低字节触发 side-effect）；
    - used ring 寄存器区与数据窗口地址冲突（迁移至非重叠地址段）。
  - 保持旧命令路径兼容（`READ_SECTOR/WRITE_SECTOR`）。
- 变更文件：
  - `src/peripheral/virtio_block.rs`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo test virtio_block::tests --lib`
  - `cargo test --lib`
  - `cargo test --test bringup_smoke`
- 验收结果：
  - 通过：VirtIO 专项 6/6；库回归 223/223；bring-up 3/3。
- 风险/未完成项：
  - 当前 queue 路径仍为“简化请求模型”，尚未接入完整 desc/avail/used DMA 访存。
  - 仍未接入 xv6 文件系统镜像和 shell 交互。
- 上下文压缩（供下一步直接续做）：
  - 下一项优先：引入最小 desc 链解析 + guest RAM 访问桥接（Bus 侧 DMA 读写接口）。

### 2026-04-01 Phase-2-03（最小 desc 链解析 + Bus DMA 桥接）

- 完成内容：
  - 在 `VirtioBlock` 增加最小 descriptor chain 处理：读取 request/data/status 三段描述符，并完成 IN/OUT 请求的数据搬运。
  - 新增 `pending descriptor notify` 机制：在 queue 配置完整时，`QUEUE_NOTIFY` 先标记待处理，再由总线侧触发 DMA 处理。
  - 在 `Bus::write_byte` 增加 VirtIO 后处理钩子：当写入 VirtIO 并存在 pending notify 时，使用 RAM 区域作为 guest memory 完成描述符访存。
  - 保持旧路径兼容：未配置 desc/avail/used 地址时，仍走前一阶段的简化 queue-notify/数据窗口路径。
  - 新增测试：
    - `peripheral::virtio_block::tests::test_virtio_block_descriptor_chain_read_flow`
    - `memory::bus::tests::test_bus_virtio_descriptor_notify_bridge`
- 变更文件：
  - `src/peripheral/virtio_block.rs`
  - `src/memory/bus.rs`
  - `src/peripheral/mod.rs`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo test virtio_block::tests --lib`
  - `cargo test memory::bus::tests::test_bus_virtio_descriptor_notify_bridge --lib`
  - `cargo test --lib`
  - `cargo test --test bringup_smoke`
- 验收结果：
  - 通过：VirtIO 专项 7/7；Bus 桥接专项 1/1；库回归 225/225；bring-up 3/3。
- 风险/未完成项：
  - 当前 guest memory 仍仅桥接 RAM 区，不覆盖跨外设/复杂 IOMMU 场景。
  - desc 校验策略仍为最小实现（缺少更严格的 flags/len/环一致性检查）。
  - xv6 文件系统镜像和 shell 交互路径仍未打通。
- 上下文压缩（供下一步直接续做）：
  - 下一项优先：补齐 descriptor flags/len 边界校验 + OUT 路径专项测试，再推进 xv6 镜像接入验证。
