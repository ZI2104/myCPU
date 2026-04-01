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

### 2026-04-01 Phase-2-04（desc 边界校验 + OUT 路径专项）

- 完成内容：
  - 在 `VirtioBlock` 的 descriptor 处理路径补齐最小边界校验：
    - 描述符索引必须落在 `queue_num` 范围内；
    - request 描述符长度需满足最小请求头长度（16 字节）；
    - data 描述符长度不得为 0；
    - status 描述符长度至少 1 字节。
  - 强化异常健壮性：desc 链异常时不再向上抛出总线错误，而是写回 IOERR 状态并推进 used ring（避免 guest 错误导致宿主侧中断式失败）。
  - 补充 OUT 路径专项测试（descriptor OUT 写盘后再 IN 读回）并增加 Bus 侧 OUT→IN 桥接验证。
- 变更文件：
  - `src/peripheral/virtio_block.rs`
  - `src/memory/bus.rs`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo test virtio_block::tests --lib`
  - `cargo test memory::bus::tests::test_bus_virtio_descriptor_notify_bridge --lib`
  - `cargo test memory::bus::tests::test_bus_virtio_descriptor_notify_bridge_out_then_in --lib`
  - `cargo test --lib`
  - `cargo test --test bringup_smoke`
- 验收结果：
  - 通过：VirtIO 专项 9/9；Bus 桥接专项 2/2；库回归 228/228；bring-up 3/3。
- 风险/未完成项：
  - 目前仍未覆盖完整 virtio 标准语义（如更完整 flags 组合、间接描述符、event idx 等）。
  - xv6 文件系统镜像接入与 shell 交互尚未验证。
- 上下文压缩（供下一步直接续做）：
  - 下一项优先：推进 xv6 镜像接入验证（加载镜像、启动参数与最小块设备对接）。

### 2026-04-01 Phase-2-05（xv6 镜像接入前置：CLI 磁盘预载）

- 完成内容：
  - 为 `run/debug/visualize` 三个子命令新增 `--virtio-disk <path>` 参数。
  - `create_bus` 新增可选磁盘镜像加载路径：根据镜像大小自动扩展 VirtIO 磁盘扇区（最小 1024 sectors），并预载原始镜像字节。
  - `VirtioBlock` 新增磁盘镜像接口：`disk_size_bytes()` 与 `load_disk_image(&[u8])`。
  - 新增镜像加载单测（正常读回 + 超容量失败）。
- 变更文件：
  - `src/main.rs`
  - `src/peripheral/virtio_block.rs`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `cargo test virtio_block::tests --lib`
  - `cargo test memory::bus::tests::test_bus_virtio_descriptor_notify_bridge --lib`
  - `cargo test memory::bus::tests::test_bus_virtio_descriptor_notify_bridge_out_then_in --lib`
  - `cargo test --lib`
  - `cargo test --test bringup_smoke`
- 验收结果：
  - 通过：构建通过；VirtIO 专项 11/11；Bus 桥接专项 2/2；库回归 230/230；bring-up 3/3。
- 风险/未完成项：
  - 当前只完成“镜像可挂载”的前置链路，尚未进行真实 xv6 镜像启动到 shell 的端到端验收。
- 上下文压缩（供下一步直接续做）：
  - 下一项优先：接入真实 xv6 kernel/fs 镜像并执行首次端到端启动验证（串口输出与 trap 日志）。

### 2026-04-01 Phase-2-06（保留 xv6 源码 + 端到端启动验证）

- 完成内容：
  - 将 xv6 源码保留在 `third_party/xv6-riscv`（不再使用 `.workbuddy` 路径）。
  - 通过 WSL 构建真实镜像产物：`kernel/kernel` 与 `fs.img`。
  - 使用 `mycpu run --virtio-disk` 执行端到端启动尝试，验证镜像可加载路径。
- 变更文件：
  - `.gitignore`
  - `scripts/cleanup_workspace.ps1`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - `wsl -e bash -lc "cd /mnt/d/code/myCPU/third_party/xv6-riscv && make kernel/kernel fs.img"`
  - `cargo run -- run --count 50000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-riscv\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-riscv\\fs.img`
- 验收结果：
  - xv6 构建通过；镜像加载成功。
  - 启动被 ELF 位宽检查拦截：`Expected 32-bit ELF for RV32I, got 64-bit`。
- 风险/未完成项：
  - 当前模拟器为 RV32I 路线，而 `xv6-riscv` 默认产物为 RV64，位宽不匹配。
- 上下文压缩（供下一步直接续做）：
  - 下一项优先：二选一推进
    1) 引入 RV64 ELF/执行支持（工作量大）；
    2) 切换到可用的 RV32 RISC-V OS 镜像进行 bring-up（工作量较小）。

### 2026-04-01 Phase-2-07（切换 RV32 镜像 + 启动链路推进）

- 完成内容：
  - 切换到可构建的 RV32 OS：`third_party/xv6-rv32`，并用现有 `riscv64-unknown-elf-*` 工具链按 `-march=rv32ima -mabi=ilp32` 产出 `ELF32` 内核镜像。
  - 修复模拟器与 xv6-rv32 启动链路不兼容点：
    - 默认总线挂载 `CLINT/PLIC`（QEMU virt 地址布局）；
    - `SYSTEM` 指令执行补齐 CSR 路径（CSRRW/CSRRS/CSRRC + immediate 变体）；
    - 增加关键只读 CSR 返回（`mhartid` 等 machine ID）；
    - 修正 `CSRRS/CSRRC` 在 `rs1/x0=0` 时应“只读不写”的语义；
    - 新增最小 RV32A 原子支持：`amoswap.w`（满足 xv6 自旋锁 acquire/release）；
    - 兼容 `WFI/SFENCE.VMA` 为当前模型下的安全 no-op。
  - 实机启动结果推进到串口 banner：`xv6 kernel is booting`。
  - 对“长时间评估”给出结论：非卡死，主要为早期初始化（内存清零/分配器初始化）导致的高指令量阶段。
- 变更文件：
  - `src/main.rs`
  - `src/cpu/core.rs`
  - `src/cpu/csr/mod.rs`
  - `src/instruction/decoder.rs`
  - `src/instruction/execute.rs`
  - `src/instruction/opcode.rs`
  - `src/cpu/pipeline/stages/decode.rs`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
  - `cargo run --release -- run --count 5000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
- 验收结果：
  - 通过：库回归 233/233；启动输出出现 `xv6 kernel is booting`，并可稳定跑满 5,000,000 指令窗口。
- 风险/未完成项：
  - 目前尚未推进到 shell 提示符，启动仍停留在早期高指令量阶段；
  - 仍需更高效的可观测性（如周期性 PC 心跳/阶段打点）来缩短定位时间。
- 上下文压缩（供下一步直接续做）：
  - 下一项优先：
    1) 增加轻量级运行期进度可观测（每 N 指令输出 PC/模式/中断状态）；
    2) 继续以 release 模式扩大窗口，确认进入 `init`/shell 的拐点；
    3) 如仍缓慢，优先优化热点（内存批量写/zero-fill 路径）而非盲目扩步。

### 2026-04-01 Phase-2-08（机器级中断门控语义修复）

- 完成内容：
  - 修复单周期与流水线 CPU 的中断门控逻辑：
    - 旧行为：无论当前特权级都要求 `mstatus.mie=1` 才接收 MIP 中断；
    - 新行为：仅在当前处于 M 态时才由 `mstatus.mie` 门控；处于 S/U 态时机器级中断可被接收（与当前实现模型一致）。
  - 新增回归测试覆盖该语义：
    - `cpu::core::tests::test_machine_timer_interrupt_taken_in_supervisor_mode_when_mie_clear`
    - `cpu::pipeline::tests::test_pipeline_machine_timer_interrupt_taken_in_supervisor_mode_when_mie_clear`
  - 通过长窗口运行验证修复后无回归，启动链路继续向前推进。
- 变更文件：
  - `src/cpu/core.rs`
  - `src/cpu/pipeline/mod.rs`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `cargo test --lib`
  - `cargo run --release -- run --count 40000000 --heartbeat-every 10000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
  - `cargo run --release -- run --count 120000000 --heartbeat-every 20000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
  - `cargo run --release -- run --count 200000000 --heartbeat-every 40000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
- 验收结果：
  - 通过：库回归 236/236。
  - 长窗口运行稳定完成，无新增 panic；PC 从早期 `memset` 热点推进至 `push_off/mycpu` 路径（`0x80000cd4` 附近）。
  - 200M 指令窗口稳定完成，最终 PC 推进至 `0x80002168`（`mycpu` 路径），无新增 panic。
- 风险/未完成项：
  - 当前心跳窗口仍未直接观察到稳定外设中断服务闭环（`meip` 常为 0，需结合磁盘 I/O场景持续验证）。
  - 尚未达到 xv6 shell 可交互里程碑。
- 上下文压缩（供下一步直接续做）：
  - 中断门控语义已修复且有双执行模型单测兜底；
  - 下一步优先在磁盘 I/O 场景验证 PLIC claim/complete 的端到端触发，并继续扩大窗口追踪 `init`/shell 拐点。

### 2026-04-01 Phase-2-09（PLIC S态窗口 + SIP 转发语义补齐）

- 完成内容：
  - 补齐 PLIC 的 Supervisor 上下文寄存器窗口，兼容 xv6-rv32 访问路径：
    - `PLIC_SENABLE(hart)` (`0x0c00_2080 + hart*0x100`)
    - `PLIC_SPRIORITY(hart)` (`0x0c20_1000 + hart*0x2000`)
    - `PLIC_SCLAIM(hart)` (`0x0c20_1004 + hart*0x2000`)
  - 新增 PLIC 双上下文状态能力：Machine/Supervisor enable 与 threshold 独立维护，`Bus::get_plic_interrupt_status()` 同时返回 `(meip, seip)`。
  - CPU（单周期/流水线）中断判定路径升级为：
    1. 先判定 Machine pending（保持机器中断优先）；
    2. 再在 S 态且 `sstatus.sie=1` 时判定 Supervisor pending。
  - 修正 `SIP` 写入语义：M 态可置/清 `SSIP`（满足 xv6 `timervec` 通过 `csrw sip, ...` 转发软中断），S 态保持“清除 SSIP”语义。
  - 新增回归测试：
    - `interrupt::plic::tests::test_plic_supervisor_context_windows`
    - `cpu::csr::tests::test_sip_machine_write_can_set_ssip`
- 变更文件：
  - `src/interrupt/plic.rs`
  - `src/memory/bus.rs`
  - `src/cpu/core.rs`
  - `src/cpu/pipeline/mod.rs`
  - `src/cpu/csr/mod.rs`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `cargo test --lib`
  - `cargo test --test bringup_smoke`
  - `cargo run --release -- run --count 120000000 --heartbeat-every 20000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
- 验收结果：
  - 通过：库回归 238/238；bringup_smoke 3/3。
  - 长窗口运行稳定完成（120M），无新增 panic。
  - 心跳观测到 `sip=0x2` 在大窗口内持续出现，证明 `SSIP` 转发链路已生效；后段 `sie` 进入 `0x222`。
- 风险/未完成项：
  - 目前心跳中 `meip` 仍多为 0，尚未形成“可稳定复现的外设中断服务闭环”证据。
  - 当前运行吞吐下降明显（约 0.37 MIPS 量级），需后续做中断扫描路径性能优化。
  - 尚未达到 xv6 shell 可交互里程碑。
- 上下文压缩（供下一步直接续做）：
  - PLIC S 态窗口和 SIP 机器态置位语义已补齐，xv6 中断路径更接近真实行为；
  - 下一步优先：在磁盘 I/O 场景抓取 `SCLAIM` 非 0 的证据，并按需增加轻量级 claim/complete 观测日志，继续推进到 shell。

### 2026-04-01 Phase-2-10（PLIC 扫描性能优化 + 超长窗口验收）

- 完成内容：
  - 优化 `PLIC` pending 源扫描算法：从“每次全量遍历 1..1023 source”改为“仅遍历 pending 位图中置位 bit”（`trailing_zeros` + 位清零迭代）。
  - 增强 heartbeat 可观测性：新增 `virtio_irq` / `uart_irq` 字段，直接显示外设 IRQ 线状态。
  - 在优化后执行更长窗口验证（200M、600M、1.2B），用于判断是否进入磁盘 I/O 阶段。
- 变更文件：
  - `src/interrupt/plic.rs`
  - `src/main.rs`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `cargo test --lib`
  - `cargo run --release -- run --count 40000000 --heartbeat-every 10000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
  - `cargo run --release -- run --count 200000000 --heartbeat-every 40000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
  - `cargo run --release -- run --count 600000000 --heartbeat-every 100000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
  - `cargo run --release -- run --count 1200000000 --heartbeat-every 200000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
- 验收结果：
  - 通过：库回归 238/238。
  - 吞吐显著回升：40M 窗口约 `6.30 MIPS`（此前同类窗口最低约 `0.37 MIPS`）。
  - 1.2B 超长窗口稳定完成，无新增 panic；心跳显示 `sstatus.sie` 在部分窗口可置 1，但 `virtio_irq/uart_irq/meip` 均长期为 0。
- 风险/未完成项：
  - 目前尚无“外设 IRQ 线拉高 -> PLIC pending -> claim/complete”的运行时证据，说明仍未进入可观测的设备中断阶段或相关触发条件未满足。
  - 尚未达到 xv6 shell 可交互里程碑。
- 上下文压缩（供下一步直接续做）：
  - 性能瓶颈已显著缓解，长窗口探索成本下降；
  - 下一步应重点增加 `PLIC_SCLAIM` 读值与 VirtIO queue notify 完成路径的轻量日志，定位“为何外设 IRQ 线始终为 0”。

### 2026-04-01 Phase-2-11（RV32C + VirtIO 兼容修复，xv6 达到 shell）

- 完成内容：
  - 修复压缩指令回归与语义一致性：
    - `C.EBREAK` 从“未实现报错”改为“进入 breakpoint trap”语义；
    - `0x0000` 压缩保留编码按非法指令 trap 处理；
    - 修正 `C.LW/C.SW` 立即数位拼接（`imm[6] <- bit5`）。
  - 修复 VirtIO 描述符数据搬运长度：不再固定截断到 512B，改为按 `data_desc.len` 传输，并增加越界保护。
  - 新增回归用例：
    - `cpu::core::tests::test_compressed_ebreak_enters_breakpoint_trap`
    - `cpu::core::tests::test_decode_c_lw_sw_immediate_bit_mapping`
    - `peripheral::virtio_block::tests::test_virtio_block_descriptor_chain_read_two_sectors`
- 变更文件：
  - `src/cpu/core.rs`
  - `src/peripheral/virtio_block.rs`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo test --lib`
  - `cargo run --release -- run --count 120000000 --heartbeat-every 20000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
  - `cargo run --release -- run --count 200000000 --heartbeat-every 40000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img`
- 验收结果：
  - 通过：库回归 `241/241`。
  - 120M 与 200M 窗口均稳定复现：`init: starting sh` 与 shell 提示符 `$`。
  - 200M 窗口稳定完成，最终 PC `0x80000dc8`，无 panic。
- 风险/未完成项：
  - 当前“可交互”已达到提示符级别，尚未将“执行 shell 命令并验收输出”固化为自动化 e2e 用例。
  - heartbeat 诊断字段较重，建议后续抽成可选 verbose 模式。
- 上下文压缩（供下一步直接续做）：
  - xv6 shell 里程碑已达成，Phase 2 可从“启动推进”转入“交互稳定性 + 自动化验收”。
  - 下一步优先：补一条 shell 命令级 smoke（如 `echo`）并收敛 heartbeat 日志开销。

### 2026-04-01 Phase-2-12（UART 主机脚本注入 + shell 命令级 smoke）

- 完成内容：
  - 为 `run` 子命令新增 UART 输入脚本注入参数：
    - `--uart-script`：待注入字符串（支持 `\\n/\\r/\\t` 转义）
    - `--uart-inject-at`：从第 N 条指令开始注入
    - `--uart-inject-every`：每 N 条指令注入 1 字节
  - 在 `Bus` 增加 `inject_uart_byte()`，将主机字节直接送入 UART RX FIFO。
  - 修复 UART 读取副作用一致性问题：
    - 采用内部可变性，确保 `RBR` 读取会真实弹出 RX FIFO；
    - 修复因 `RefCell` 重入导致的借用冲突。
  - 新增解析单测：`parse_escaped_uart_script` 的常见转义与未知转义保留语义。
- 变更文件：
  - `src/main.rs`
  - `src/memory/bus.rs`
  - `src/peripheral/uart.rs`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `cargo test --lib`
  - `cargo run --release -- run --count 200000000 --heartbeat-every 40000000 --memory 128 D:\\code\\myCPU\\third_party\\xv6-rv32\\kernel\\kernel --virtio-disk D:\\code\\myCPU\\third_party\\xv6-rv32\\fs.img --uart-script "echo HI\\n" --uart-inject-at 130000000 --uart-inject-every 5000`
- 验收结果：
  - 通过：库回归 `241/241`。
  - 命令级 smoke 通过：运行日志出现 `echo HI`、`HI` 与后续 `$` 提示符，且统计显示 `UART script injected: 8/8 bytes`。
- 风险/未完成项：
  - 当前注入基于“指令步数”定时，尚非“根据 shell 提示符事件触发”的自适应注入。
  - heartbeat 输出较重，建议后续拆分精简模式与诊断模式。
- 上下文压缩（供下一步直接续做）：
  - 已具备非交互环境下的 shell 命令自动注入能力，可据此沉淀真正的 e2e 自动验收。
  - 下一步优先：新增独立 smoke 流程（脚本/测试）验证 `echo/ls` 并断言关键输出。

### 2026-04-01 Phase-2-13（xv6 shell smoke 自动化脚本落地）

- 完成内容：
  - 新增独立脚本 `scripts/run_xv6_shell_smoke.ps1`，将“xv6 启动 + UART 命令注入 + 输出断言”固化为一键自动化流程。
  - 脚本支持关键参数配置：`Count`、`HeartbeatEvery`、`MemoryMB`、`KernelPath`、`DiskPath`、`UartScript`、`UartInjectAt`、`UartInjectEvery`。
  - 脚本执行后自动聚合 stdout/stderr 到日志文件，并对关键输出做正则断言：`init: starting sh`、`echo HI`、`HI`、`$`、`UART script injected:`。
- 变更文件：
  - `scripts/run_xv6_shell_smoke.ps1`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\\scripts\\run_xv6_shell_smoke.ps1`
- 验收结果：
  - 通过：脚本输出 `PASS`。
  - 200M 指令窗口实跑关键日志包含：`init: starting sh`、`echo HI`、`HI`、`$`。
  - 统计显示：`UART script injected: 8/8 bytes`。
- 风险/未完成项：
  - 当前为“基于指令步数”的注入策略，仍非“基于 shell 提示符事件”的自适应注入。
  - 目前默认断言脚本命令为 `echo HI`，后续可扩展为 `ls`/`cat` 等更丰富交互回归矩阵。
- 上下文压缩（供下一步直接续做）：
  - xv6 命令级 smoke 已形成可复用自动化入口，后续可直接复用该脚本做回归前置。
  - 下一步优先：将 heartbeat 重日志拆为轻量模式与诊断模式，降低常规回归日志噪声与 IO 开销。

### 2026-04-01 Phase-2-14（Heartbeat 轻量/诊断模式拆分）

- 完成内容：
  - `run` 子命令新增 `--heartbeat-mode <compact|diagnostic>` 参数。
  - 默认模式调整为 `compact`：仅输出关键字段（步数、PC、特权级、CSR 中断位、外设 IRQ 状态、VirtIO 关键计数、PC 连续停留计数）。
  - `diagnostic` 模式保留原有全量重字段 heartbeat 输出，兼容深度排障。
- 变更文件：
  - `src/main.rs`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `powershell -ExecutionPolicy Bypass -File .\\scripts\\run_xv6_shell_smoke.ps1`
- 验收结果：
  - 通过：构建成功。
  - shell smoke 通过，日志显示 `Heartbeat enabled: every 40000000 instructions (mode=Compact)` 且输出 `hb-lite` 行。
  - 关键交互输出仍稳定出现：`init: starting sh`、`echo HI`、`HI`、`$`。
- 风险/未完成项：
  - 当前仍是“按指令步数”触发注入，未实现“按 shell prompt 事件”自适应注入。
  - `diagnostic` 模式输出仍较重，后续可按模块开关进一步分层。
- 上下文压缩（供下一步直接续做）：
  - 常规回归可默认使用 `compact`，性能与可读性更均衡；定位复杂中断/调度问题时再切换到 `diagnostic`。
  - 下一步优先：扩展 shell smoke 为命令矩阵（`echo/ls/cat`）并自动汇总断言结果。

### 2026-04-01 Phase-2-15（xv6 shell 命令矩阵 smoke）

- 完成内容：
  - 升级 `scripts/run_xv6_shell_smoke.ps1`，新增 `-Mode single|matrix` 与 `-MatrixScenarios`（`echo/ls/cat`）参数。
  - 新增矩阵执行汇总：每个场景分别输出日志尾部、断言结果与总览 PASS/FAIL。
  - 修复脚本在 `Set-StrictMode` 下的集合计数问题（`MissingPatterns` 与失败场景集合统一数组化）。
- 变更文件：
  - `scripts/run_xv6_shell_smoke.ps1`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\\scripts\\run_xv6_shell_smoke.ps1 -Mode matrix`
- 验收结果：
  - 通过：矩阵汇总 `echo/ls/cat` 全部 PASS。
  - `ls` 场景成功输出目录项（`README`、`init`、`sh` 等）。
  - `cat` 场景成功输出 `README` 正文，并执行后续 `echo CAT_DONE`。
- 风险/未完成项：
  - 当前注入仍为“按指令步数”调度，未实现“按 shell prompt 事件”自适应输入。
  - 断言仍以正则文本匹配为主，尚未做结构化会话状态机校验。
- 上下文压缩（供下一步直接续做）：
  - 命令矩阵 smoke 已成为可复用回归入口，后续可直接扩展更多场景（如 `grep/wc/usertests`）。
  - 下一步优先：实现基于 prompt 事件的自适应注入，降低对固定注入步数的依赖。

### 2026-04-01 Phase-2-16（prompt 触发注入 + 矩阵验收）

- 完成内容：
  - `run` 子命令新增 `--uart-inject-trigger <step|prompt>`，支持按 shell prompt 输出事件启动注入。
  - 在 UART 输出回调中新增 prompt 检测（`$ ` 序列），并通过共享信号与注入器解耦。
  - `scripts/run_xv6_shell_smoke.ps1` 新增 `-UartInjectTrigger` 参数并默认使用 `prompt`。
  - 为 prompt 检测补充单元测试：`PromptDetectorState` 序列识别/误报防护。
- 变更文件：
  - `src/main.rs`
  - `scripts/run_xv6_shell_smoke.ps1`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `cargo test --lib`
  - `powershell -ExecutionPolicy Bypass -File .\\scripts\\run_xv6_shell_smoke.ps1 -Mode matrix`
- 验收结果：
  - 通过：构建成功，库测试 `241/241`。
  - `echo/ls/cat` 三场景矩阵 smoke 全部 PASS。
  - 运行日志确认注入模式为 `trigger=Prompt` 且 `UART script injected` 计数完整。
- 风险/未完成项：
  - prompt 检测当前基于 `$ ` 文本序列，后续可扩展对不同 shell/提示符风格的可配置匹配。
  - 当前断言仍基于正则文本，尚未升级为结构化会话状态机。
- 上下文压缩（供下一步直接续做）：
  - 注入链路已具备 step/prompt 双模式，常规回归建议使用 `prompt` 降低固定步数参数调优成本。
  - 下一步优先：引入结构化会话断言（命令回显/输出/提示符三段状态机）并扩展场景到 `grep/wc/usertests`。

### 2026-04-01 Phase-2-17（有序会话断言 + 二次矩阵验收）

- 完成内容：
  - 为 `scripts/run_xv6_shell_smoke.ps1` 增加有序标记断言：在正则匹配之外，验证关键交互片段的出现顺序。
  - `echo/ls/cat` 场景分别引入 `OrderedMarkers`，覆盖“命令回显 → 结果标记/输出”的顺序检查。
- 变更文件：
  - `scripts/run_xv6_shell_smoke.ps1`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\\scripts\\run_xv6_shell_smoke.ps1 -Mode matrix`
- 验收结果：
  - 通过：在开启 `prompt` 触发注入与有序断言后，矩阵 `echo/ls/cat` 全部 PASS。
  - 每场景均输出完整日志尾部与总结，且退出码为 0。
- 风险/未完成项：
  - 当前“有序断言”仍是轻量状态验证，尚未上升为完整会话自动机（含异常分支与重试）。
  - 提示符检测仍基于 `$ `，需为其他 shell 风格预留可配置项。
- 上下文压缩（供下一步直接续做）：
  - 当前 smoke 已具备：step/prompt 双触发 + 正则匹配 + 有序断言三层保障。
  - 下一步优先：扩展矩阵场景（`grep/wc/usertests`）并沉淀失败分类（启动失败/注入失败/断言失败）。

### 2026-04-01 Phase-2-18（矩阵扩展到 grep/wc + 失败分类）

- 完成内容：
  - 升级 `scripts/run_xv6_shell_smoke.ps1` 的矩阵能力：在默认矩阵中加入 `grep` 与 `wc` 场景（`echo/ls/cat/grep/wc`）。
  - 增加场景模板 `usertests`（可选触发，默认不纳入矩阵）。
  - 增加失败分类机制：`startup` / `injection` / `assertion`，并在失败摘要中输出分类与缺失项。
- 变更文件：
  - `scripts/run_xv6_shell_smoke.ps1`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\\scripts\\run_xv6_shell_smoke.ps1 -Mode matrix`
- 验收结果：
  - 通过：矩阵 `echo/ls/cat/grep/wc` 全部 PASS（5/5）。
  - `grep` 场景确认命令输出与 DONE 标记顺序正确（`grep xv6 README` → `GREP_DONE`）。
  - `wc` 场景确认统计输出与 DONE 标记顺序正确（`wc README` → `WC_DONE`）。
  - 所有场景均在 200M 指令窗口完成，退出码为 0。
- 风险/未完成项：
  - `usertests` 场景目前仅提供模板，默认矩阵未启用（避免显著拉长回归时长）。
  - 失败分类当前基于输出模式匹配，后续可升级为更结构化的事件分类。
- 上下文压缩（供下一步直接续做）：
  - shell smoke 现已覆盖 5 条常用命令路径并具备失败分类，可直接用于 PR 前回归。
  - 下一步优先：为 `usertests` 增加长窗口预设（如独立模式/单场景高步数）与分类细化统计。

### 2026-04-01 Phase-2-19（prompt 逐命令自适应注入 + 会话状态机断言）

- 完成内容：
  - 将 `UartInjector` 的 prompt 模式从“整段脚本连续注入”升级为“按 prompt 事件逐命令分片注入”（按 `\n` 切片）。
  - 保留 step 模式语义不变；prompt 模式下每次检测到 shell 提示符后仅发送一条命令片段，避免“下一条命令与当前输出互相串扰”。
  - `scripts/run_xv6_shell_smoke.ps1` 断言升级为会话状态机验证：支持“同一行多状态前进”（处理 `$ cmd` 同行场景），并对命令回显加入可选提示符前缀匹配。
  - smoke 脚本执行链路从 `cargo run` 切换为“单次 `cargo build --release` + 直接执行 `target\\release\\mycpu.exe`”，移除包装层导致的非确定性退出噪声。
- 变更文件：
  - `src/main.rs`
  - `scripts/run_xv6_shell_smoke.ps1`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `cargo test --lib`
  - `powershell -ExecutionPolicy Bypass -File .\\scripts\\run_xv6_shell_smoke.ps1 -Mode matrix`
- 验收结果：
  - 通过：库回归 `241/241`。
  - 通过：矩阵 `echo/ls/cat/grep/wc` 在 prompt 自适应注入 + 状态机断言下全部 PASS（5/5）。
  - 关键行为验证：`ls/cat/grep/wc` 场景中“命令执行完成后再注入下一条命令”的顺序稳定复现。
- 风险/未完成项：
  - `usertests` 仍未默认纳入矩阵（时长成本较高）。
  - 当前状态机仍为日志驱动验证，尚未抽象为独立可复用状态机模块。
- 上下文压缩（供下一步直接续做）：
  - 交互回归链路已具备：prompt 逐命令注入 + 状态机断言 + 失败分类，可直接承接更重场景。
  - 下一步优先：为 `usertests` 增加长窗口 profile，并细化失败分类到“命令执行超时/输出不匹配/提示符未回归”。

### 2026-04-01 Phase-3-01（Linux 启动上下文注入骨架：FDT/bootargs/hartid）

- 完成内容：
  - `run` 子命令新增 Linux 启动上下文参数：
    - `--linux-boot`
    - `--linux-hartid`
    - `--linux-dtb` / `--linux-dtb-addr`
    - `--linux-bootargs` / `--linux-bootargs-addr`
  - 启动前注入逻辑：
    - 可选加载 DTB 到指定 guest 地址（并将 `a1` 指向 DTB 地址）；
    - 可选写入 NUL 结尾 bootargs 字符串到指定 guest 地址；
    - 将 `a0` 设置为 hartid。
- 变更文件：
  - `src/main.rs`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `target\\release\\mycpu.exe run --count 1000000 --memory 128 third_party\\xv6-rv32\\kernel\\kernel --virtio-disk third_party\\xv6-rv32\\fs.img --linux-boot --linux-hartid 0 --linux-bootargs "console=ttyS0 root=/dev/vda rw"`
- 验收结果：
  - 通过：运行日志出现 `Linux boot: wrote bootargs ...` 与 `Linux boot context: a0(hartid)=0, a1(dtb)=0x00000000`。
  - 通过：1,000,000 指令窗口稳定完成，无新增 panic。
- 风险/未完成项：
  - 当前为“引导参数注入骨架”，尚未完成 SBI firmware 链接与完整 FDT 自动构建。
  - 尚未完成 Buildroot Linux 到 `init/userland` 的端到端启动验收。
- 上下文压缩（供下一步直接续做）：
  - Phase 3 已具备最小运行时上下文注入能力（hartid/bootargs/可选DTB），可作为 Linux bring-up 基础。
  - 下一步优先：补齐 SBI 引导入口（firmware + payload）并接入 Buildroot Linux 镜像验证。

### 2026-04-01 Phase-3-02（SBI + payload 双镜像加载链路）

- 完成内容：
  - `run` 子命令新增 Linux SBI 启动链参数：
    - `--linux-sbi <path>`
    - `--linux-sbi-addr <hex>`
    - `--linux-payload-addr <hex>`
  - 新增双镜像加载流程：
    - payload（位置参数 `FILE`）按 raw image 加载到 `--linux-payload-addr`；
    - SBI firmware 按 `load_file()` 加载（支持 ELF 入口），模拟器起始 PC 选择 SBI 入口。
  - 增加参数约束：`--linux-sbi` 需要配合 `--linux-boot` 使用，避免误配置。
- 变更文件：
  - `src/main.rs`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `cargo test --lib`
  - `cargo run --release -- run --count 1000000 --memory 128 third_party\\xv6-rv32\\kernel\\kernel --virtio-disk third_party\\xv6-rv32\\fs.img --linux-boot --linux-sbi third_party\\xv6-rv32\\kernel\\kernel --linux-sbi-addr 0x80000000 --linux-payload-addr 0x80200000 --linux-hartid 0 --linux-bootargs "console=ttyS0 root=/dev/vda rw"`
- 验收结果：
  - 通过：构建成功，库测试回归 `241/241`。
  - 通过：运行日志确认 payload 与 SBI 分别加载，并以 SBI 入口作为起始 PC：
    - `Linux boot chain: loaded payload ... at 0x80200000`
    - `Linux boot chain: loaded SBI firmware ... start 0x80000000`
  - 通过：1,000,000 指令窗口稳定完成，无新增 panic。
- 风险/未完成项：
  - 当前仍是“链路级装载能力”，尚未引入真实 OpenSBI 固件与 Buildroot Linux 镜像做端到端启动。
  - payload 当前按 raw image 装载，后续需根据 Linux 镜像类型补充更细粒度的装载策略与校验。
- 上下文压缩（供下一步直接续做）：
  - Phase 3 现已具备：启动上下文注入（hartid/bootargs/DTB）+ SBI/payload 双镜像加载链路。
  - 下一步优先：接入真实 OpenSBI（fw_jump/fw_payload）与 Buildroot Image + DTB，冲刺 `init/userland`。

### 2026-04-01 Phase-3-03（自动 FDT 注入 + Phase3 终验阻塞确认）

- 完成内容：
  - 新增 `--linux-auto-dtb`：当未提供 `--linux-dtb` 时，自动生成最小可引导 DTB（含 `chosen.bootargs`、`memory@80000000`、`#address-cells/#size-cells`）并写入 `--linux-dtb-addr`。
  - 启动上下文自动设置 `a1` 指向自动生成的 DTB 地址，保持 `a0=hartid`。
  - 新增 DTB 生成单测：校验 FDT magic 与 `bootargs` 字符串注入。
  - 完成仓库工件核验：扫描 `Image/zImage/bzImage/fw_jump/fw_payload/opensbi/rootfs*.dtb`，当前仓库无真实 OpenSBI/Buildroot Linux 工件。
- 变更文件：
  - `src/main.rs`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `cargo test --lib`
  - `cargo run --release -- run --count 2000000 --memory 128 third_party\\xv6-rv32\\kernel\\kernel --virtio-disk third_party\\xv6-rv32\\fs.img --linux-boot --linux-auto-dtb --linux-sbi third_party\\xv6-rv32\\kernel\\kernel --linux-sbi-addr 0x80000000 --linux-payload-addr 0x80200000 --linux-hartid 0 --linux-bootargs "console=ttyS0 root=/dev/vda rw"`
  - `Get-ChildItem -Path . -Recurse -File ...`（关键工件扫描）
- 验收结果：
  - 通过：构建成功，库测试回归 `241/241`。
  - 通过：运行日志确认自动 DTB 注入成功：
    - `Linux boot: auto-generated DTB ... at 0x87f00000`
    - `Linux boot context: a0(hartid)=0, a1(dtb)=0x87f00000`
  - 阻塞确认：仓库内未发现真实 OpenSBI 与 Buildroot Linux 端到端启动所需镜像，当前无法完成“进入 init/userland”的最终验收。
- 风险/未完成项：
  - Phase 3 的最终目标（Buildroot Linux 进入 `init/userland`）依赖外部工件，当前仓库资产不足。
- 上下文压缩（供下一步直接续做）：
  - 模拟器侧 Phase 3 链路能力已齐全：SBI + 自动/外部 FDT + bootargs + payload 装载。
  - 下一步只需补齐外部工件（OpenSBI + Buildroot Image/rootfs/dtb）即可执行终验并闭环。

### 2026-04-01 Phase-4-01（输入外设通路：MMIO + 可视化命令 + 前端面板）

- 完成内容：
  - 新增输入外设 `InputDevice`（`0x1000_2000`）：
    - `key_state` 位图、`last_event`、`event_count`、`control/status`（IRQ pending）寄存器。
    - 支持主机注入按键按下/抬起、清空按键状态与中断确认。
  - 总线新增输入能力：
    - `inject_input_key()` / `clear_input_keys()` / `get_input_snapshot()`。
  - 可视化服务新增输入命令：
    - `input <key> <down|up>`、`input clear`、`input state`。
  - 前端新增 `InputPanel`：
    - 鼠标按键（↑←↓→/A/B）与键盘映射（WASD/方向键 + J/K）上报输入命令。
    - 支持 `Release All` 一键释放。
- 变更文件：
  - `src/peripheral/input.rs`
  - `src/peripheral/mod.rs`
  - `src/memory/bus.rs`
  - `src/visualize/server.rs`
  - `src/main.rs`
  - `frontend/src/components/InputPanel.tsx`
  - `frontend/src/App.tsx`
  - `frontend/src/App.css`
  - `docs/ROADMAP.md`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：`cargo test --lib` -> `248/248` 全通过（新增 `peripheral::input`、`memory::bus`、`visualize::server` 输入相关测试）。
  - 通过：前端构建成功（`vite build` 完成）。
- 风险/未完成项：
  - 当前已打通“输入设备通路”，但 Linux 用户态游戏程序本体与 Overlay 指标展示尚未完成。
  - 输入协议当前为轻量文本命令，后续可演进为结构化事件通道（含按键重复、时间戳与批量提交）。
- 上下文压缩（供下一步直接续做）：
  - Phase 4 已完成“渲染 + 输入”两条基础链路（Framebuffer + Input）。
  - 下一步优先：
    1) 增加 Linux 用户态小游戏 demo 程序并接入输入寄存器；
    2) 在前端补 Overlay（FPS/IPC/stall/syscall）并与运行态联动；
    3) 为 Phase 4 增加一键验收脚本（渲染+输入回环断言）。

### 2026-04-01 Phase-4-02（Guest 输入脚本注入 + Mario CPU 验收脚本）

- 完成内容：
  - `run` 子命令新增输入脚本注入参数：
    - `--input-script`：按键事件脚本（支持 `;` / `\n` 分隔）
    - `--input-inject-at`：从第 N 条指令开始注入
    - `--input-inject-every`：每 N 条指令注入一个输入事件
  - 新增输入脚本解析能力：
    - 支持 `key:down|up` 与 `key down|up` 两种语法；
    - 支持 `clear` 指令（批量释放 key0~key7）；
    - 支持十六进制/十进制 key code 与常见别名（方向键/WASD/J/K）。
  - 在执行主循环中接入 Input 注入器，并增加执行统计输出：
    - `Input script injected: X/Y actions`。
  - 新增一键验收脚本 `scripts/run_mario_cpu_validation.ps1`，用于 guest 程序 CPU 行为验证（含最小指令数阈值、输入注入计数和可选日志 marker 断言）。
- 变更文件：
  - `src/main.rs`
  - `scripts/run_mario_cpu_validation.ps1`
- 验收命令：
  - `cargo test --lib`
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_mario_cpu_validation.ps1 -GuestBinary .\third_party\xv6-rv32\kernel\kernel -VirtioDisk .\third_party\xv6-rv32\fs.img -ExpectedMarkers "init: starting sh" -RequireInputFullyInjected`
- 验收结果：
  - 通过：新增解析单测和全量库测试通过。
  - 通过：Mario CPU 验收脚本可完成一次 guest 运行并输出 PASS，总结包含指令计数与输入注入计数。
- 风险/未完成项：
  - 当前仍是“输入事件注入 + CPU 里程碑验收”基础能力，尚未接入真实 guest NES 模拟器应用与 ROM 端到端链路。
- 上下文压缩（供下一步直接续做）：
  - 现已具备可脚本化的 guest 输入回放与 CPU 验收框架；
  - 下一步优先：
    1) 引入 guest 侧 NES 应用二进制与 ROM 资源；
    2) 用同一验收脚本补齐“标题画面出现 + 输入响应 + 帧缓冲变化”断言。
