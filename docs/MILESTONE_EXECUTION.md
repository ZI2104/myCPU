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
