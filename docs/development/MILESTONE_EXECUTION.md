# OS 里程碑执行记录（逐项验收 + 上下文压缩）

> 目标：按 `docs/design/ROADMAP.md` 中的“OS Bring-up 里程碑（用户目标对齐）”逐项推进。
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - 在 UART 输出回调中新增 prompt 检测（`$` 后跟空格的序列），并通过共享信号与注入器解耦。
  - `scripts/run_xv6_shell_smoke.ps1` 新增 `-UartInjectTrigger` 参数并默认使用 `prompt`。
  - 为 prompt 检测补充单元测试：`PromptDetectorState` 序列识别/误报防护。
- 变更文件：
  - `src/main.rs`
  - `scripts/run_xv6_shell_smoke.ps1`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/design/ROADMAP.md`
- 验收命令：
  - `cargo build`
  - `cargo test --lib`
  - `powershell -ExecutionPolicy Bypass -File .\\scripts\\run_xv6_shell_smoke.ps1 -Mode matrix`
- 验收结果：
  - 通过：构建成功，库测试 `241/241`。
  - `echo/ls/cat` 三场景矩阵 smoke 全部 PASS。
  - 运行日志确认注入模式为 `trigger=Prompt` 且 `UART script injected` 计数完整。
- 风险/未完成项：
  - prompt 检测当前基于 `$` 后跟空格的文本序列，后续可扩展对不同 shell/提示符风格的可配置匹配。
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
  - `docs/design/ROADMAP.md`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\\scripts\\run_xv6_shell_smoke.ps1 -Mode matrix`
- 验收结果：
  - 通过：在开启 `prompt` 触发注入与有序断言后，矩阵 `echo/ls/cat` 全部 PASS。
  - 每场景均输出完整日志尾部与总结，且退出码为 0。
- 风险/未完成项：
  - 当前“有序断言”仍是轻量状态验证，尚未上升为完整会话自动机（含异常分支与重试）。
  - 提示符检测仍基于 `$` 后跟空格，需为其他 shell 风格预留可配置项。
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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
  - `docs/design/ROADMAP.md`
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

### 2026-04-01 Phase-4-03（渲染+输入自动验收脚本闭环）

- 完成内容：
  - 新增 Phase 4 自动验收脚本 `scripts/run_phase4_input_framebuffer_acceptance.ps1`：
    - 自动启动 `visualize --linux-fb-demo --warmup` 后端；
    - 通过 WebSocket 串联 `fb_game init/step`、`input right down/up/clear`、`input state`、`fb linux`；
    - 断言输入状态位变化（RIGHT 按下后置位、clear 后归零）；
    - 断言帧缓冲有效且发生变化（非零像素 + 签名变化），完成“渲染+输入回环”自动验收。
  - 脚本执行后自动清理后端进程，并输出日志路径。
- 变更文件：
  - `scripts/run_phase4_input_framebuffer_acceptance.ps1`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1`
  - `cargo test --lib`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：Phase 4 自动验收脚本 PASS（frame signature changed + input loop verified）。
  - 通过：`cargo test --lib` -> `250/250`。
  - 通过：前端构建成功（`vite build`）。
- 风险/未完成项：
  - 当前仍是 host 侧 `fb_game` 演示闭环，尚未切换到 guest 侧 NES 应用 + ROM 的端到端链路。
- 上下文压缩（供下一步直接续做）：
  - Phase 4 已具备“Framebuffer + Input + 自动验收脚本”闭环能力；
  - 下一步优先：将验收目标从 `fb_game` 迁移到 guest 侧 NES 程序（标题画面、输入响应、帧变化三类断言）。

### 2026-04-01 Phase-4-04（Guest 就绪验收：stepn + 双模式脚本）

- 完成内容：
  - 可视化后端新增 WebSocket 命令 `stepn <N>`（批量执行 N 个 CPU step），用于 guest 程序场景下快速推进运行态。
  - 修复可见性告警：`CommandContext::new` 由 `pub` 收敛为模块内私有，避免 `private_interfaces` 警告。
  - 升级 `scripts/run_phase4_input_framebuffer_acceptance.ps1` 为双模式：
    - `host-demo`：沿用 `fb_game + input + framebuffer` 回环；
    - `guest-binary`：新增 `stepn` 推进 + guest 帧缓冲签名变化断言 + 输入状态断言。
  - `guest-binary` 在未提供 `-GuestProgram` 时自动生成最小 RV32 帧缓冲 demo 二进制，确保脚本可独立运行完成验收。
- 变更文件：
  - `src/visualize/server.rs`
  - `scripts/run_phase4_input_framebuffer_acceptance.ps1`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode host-demo`
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode guest-binary`
  - `cargo test --lib`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：`host-demo` 模式 PASS。
  - 通过：`guest-binary` 模式 PASS（自动生成 guest demo + `stepn` 推进 + 帧变化断言）。
  - 通过：库测试 `251/251`；前端构建通过。
- 风险/未完成项：
  - 当前 guest 模式默认二进制为“最小帧缓冲 demo”，尚未接入真实 NES guest 应用与 ROM 资源。
- 上下文压缩（供下一步直接续做）：
  - Phase 4 验收已从 host-only 扩展为 host/guest 双模式，具备 `stepn` 快速推进能力；
  - 下一步优先：接入真实 guest NES 应用并将断言从“帧变化”升级到“标题画面 + 输入响应 + 帧变化”三联验收。

### 2026-04-01 Phase-4-04-R1（待办收尾复验 + 文档同步）

- 完成内容：
  - 基于当前工作区最新文件状态，复验 `stepn` + Phase4 双模式验收脚本链路。
  - 复跑 host/guest 两条自动验收路径，确认输入回环与帧签名变化断言稳定通过。
  - 复跑库测试与前端构建，并同步记录到里程碑与路线图文档。
- 变更文件：
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/design/ROADMAP.md`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode host-demo`
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode guest-binary -SkipBuild`
  - `cargo test --lib`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：`host-demo` 模式 PASS（input loop verified + frame signature changed）。
  - 通过：`guest-binary` 模式 PASS（自动 guest demo + `stepn=120000` + frame signature changed）。
  - 通过：库测试 `251/251`。
  - 通过：前端构建成功（Vite build）。
- 风险/未完成项：
  - 仍未接入真实 NES guest 应用与 ROM，当前 guest 验收目标仍是“最小帧缓冲 demo”。
- 上下文压缩（供下一步直接续做）：
  - 待办中的“stepn/双模式验收/回归测试/文档同步”已全部闭环；
  - 下一步可直接进入真实 guest NES 工件接入与“三联业务断言”升级。

### 2026-04-01 Phase-4-04-R2（相关文档整理更新）

- 完成内容：
  - 整理并更新用户入口文档，补齐 Phase 4 自动验收命令与说明，降低“脚本存在但入口文档缺失”的使用门槛。
  - 在 `README.md` 的快速开始中新增：
    - `run_phase4_input_framebuffer_acceptance.ps1`（`host-demo` / `guest-binary`）
    - `run_mario_cpu_validation.ps1`（guest CPU 行为验收）
  - 在 `docs/guides/DEMO_GUIDE.md` 增加“Phase 4 快速验收”章节，包含 host/guest 一键验收命令与 PASS 关键字说明。
  - 同步修正文档中的 Markdown 代码块语言标注（避免无语言围栏告警）。
- 变更文件：
  - `README.md`
  - `docs/guides/DEMO_GUIDE.md`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - 文档整理项，无额外运行时回归；功能链路沿用 R1 复验结果（host/guest 验收 + `cargo test --lib` + 前端构建均通过）。
- 验收结果：
  - 通过：文档入口与当前实现能力对齐，Phase 4 验收路径可直接按文档执行。
- 风险/未完成项：
  - 真实 NES guest 应用与 ROM 仍未接入，文档中的 guest 验收仍基于最小 demo binary。
- 上下文压缩（供下一步直接续做）：
  - 用户入口文档已补齐到 Phase 4 当前能力；
  - 下一步可在文档中继续补“真实 NES 工件接入与三联断言”操作手册。

### 2026-04-01 Phase-4-05（前端游戏流程控制闭环）

- 完成内容：
  - 新增前端 `GameFlowPanel`，打通 `fb_game` 的可视化操作流程：`init/step/run-pause/reset/sync`。
  - 在 `framebuffer` 页签内接入游戏状态与输入状态联动展示：
    - 实时显示 `tick/score`；
    - 同步显示 `input_state` 位图与 IRQ 状态；
    - 支持运行间隔配置，形成“输入→游戏推进→帧刷新”的前端闭环。
  - `App` 消息分发补齐 `framebuffer_game` / `input_state` 响应接线。
- 变更文件：
  - `frontend/src/App.tsx`
  - `frontend/src/components/GameFlowPanel.tsx`
- 验收命令：
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：前端构建通过（含新增组件与状态分发）。
- 风险/未完成项：
  - 当前 guest 路径仍以最小 demo binary 为默认工件，真实 NES guest 工件接入不在本条目范围。
- 上下文压缩（供下一步直接续做）：
  - 游戏流程控制已进入前端主界面，下一步聚焦 Overlay 完成度与自动验收断言增强。

### 2026-04-01 Phase-4-06（Overlay 完成 + Phase4 终验）

- 完成内容：
  - `FramebufferView` 新增 Overlay HUD：显示 `FPS/IPC/Stalls/Tick/Score/InputBits`。
  - 增强 Phase 4 自动验收脚本 `run_phase4_input_framebuffer_acceptance.ps1`：
    - host 模式新增 `fb_game state` 断言（`tick >= 6` 且包含 `ball_x/ball_y`）；
    - guest 模式新增 `stepn.executed > 0` 断言。
  - 形成“游戏流程 + 输入 + 帧缓冲 + Overlay 指标”的 Phase 4 完整交付。
- 变更文件：
  - `frontend/src/components/FramebufferView.tsx`
  - `frontend/src/App.css`
  - `scripts/run_phase4_input_framebuffer_acceptance.ps1`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode host-demo -SkipBuild`
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode guest-binary -SkipBuild`
  - `cargo test --lib`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：`host-demo` PASS（含新增游戏状态断言）。
  - 通过：`guest-binary` PASS（含 `stepn executed` 断言）。
  - 通过：库测试 `251/251`。
  - 通过：前端构建成功。
- 风险/未完成项：
  - 真实 NES guest 应用与 ROM 仍属下一阶段增强项，不影响本阶段“简化 2D 游戏 + 输入 + Overlay”验收闭环。
- 上下文压缩（供下一步直接续做）：
  - Phase 4 已完成并具备自动化验收；
  - 下一阶段可直接转向“真实 NES guest 工件接入 + 三联业务断言（标题画面/输入响应/帧变化）”。

### 2026-04-01 Phase-3-04（工件自动接入脚本 + 终验前置校验）

- 完成内容：
  - 新增 `scripts/setup_phase3_artifacts.ps1`：
    - 自动下载 OpenSBI 发布包（默认 `v1.8.1`）；
    - 自动选择 `ilp32/generic` 的 `fw_jump.elf`；
    - 标准化 Phase 3 工件目录（`artifacts/phase3`）。
  - 新增 `scripts/build_phase3_buildroot_artifacts.ps1`：
    - 自动拉取 Buildroot（默认 `2024.02.1`）并执行 `qemu_riscv32_virt_defconfig`；
    - 产出并复制 `fw_jump.elf / Image / rootfs.ext2` 到 `artifacts/phase3`。
  - 增强 `scripts/run_linux_phase3_acceptance.ps1`：
    - 新增 `-AutoResolveArtifacts` 自动探测工件；
    - 新增 `-DownloadOpenSbiIfMissing` / `-SkipBuild`；
    - 新增 payload 前置校验：默认拒绝 ELF 作为 raw Linux payload（可用 `-AllowElfPayloadRaw` 仅做调试）。
  - 工件阻塞诊断从“运行后崩溃”升级为“运行前明确提示缺失 Linux Image/rootfs”。
- 变更文件：
  - `scripts/setup_phase3_artifacts.ps1`
  - `scripts/build_phase3_buildroot_artifacts.ps1`
  - `scripts/run_linux_phase3_acceptance.ps1`
  - `README.md`
  - `docs/design/ROADMAP.md`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\setup_phase3_artifacts.ps1`
  - `powershell -ExecutionPolicy Bypass -File .\scripts\build_phase3_buildroot_artifacts.ps1 -SkipClone -SkipBuild`（脚本烟测）
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_linux_phase3_acceptance.ps1 -AutoResolveArtifacts -AutoDtb -SkipBuild`
- 验收结果：
  - 通过：OpenSBI 自动下载与工件目录初始化成功，且正确选择 `ilp32/generic/fw_jump.elf`。
  - 通过：Buildroot 构建脚本可执行并输出明确前置条件/路径诊断。
  - 通过：Phase 3 验收脚本可自动探测工件并输出明确阻塞信息（缺失 Linux payload 时即时报错）。
  - 说明：当前仓库仍缺少真实 Buildroot Linux `Image/rootfs/dtb`，`StrictUserlandMarker` 终验暂不可达。
- 风险/未完成项：
  - Phase 3 最终目标仍依赖外部 Linux 工件（raw `Image` + rootfs + dtb）。
  - `UseXv6SmokeFallback` 仅用于链路调试，不代表 Linux `init/userland` 终验通过。
- 上下文压缩（供下一步直接续做）：
  - 工件准备与验收脚本已具备“自动发现 + 前置校验 + 明确阻塞”能力；
  - 下一步只需补齐真实 Linux 工件后执行：
    - `powershell -ExecutionPolicy Bypass -File .\scripts\run_linux_phase3_acceptance.ps1 -AutoResolveArtifacts -AutoDtb -StrictUserlandMarker`。

### 2026-04-02 Phase-5-01（NPU/LPU 描述符 DMA 最小闭环）

- 完成内容：
  - 为 `Npu` / `Lpu` 增加描述符寄存器组（`DESC_ADDR/DESC_LEN/DESC_NOTIFY`）与任务统计寄存器。
  - 增加 pending-notify 处理路径：外设收到 `DESC_NOTIFY` 后进入“待处理”状态。
  - 在 `Bus::write_byte` 中增加 NPU/LPU 后处理钩子：若存在 pending notify，由总线桥接 RAM 完成 descriptor 批处理（DMA 风格）并写回结果。
  - 扩展 PLIC pending 同步，新增 NPU/LPU IRQ 源映射（保持 VirtIO/UART 行为不变）。
  - 新增回归测试覆盖外设级与总线级路径。
- 变更文件：
  - `src/peripheral/npu.rs`
  - `src/peripheral/lpu.rs`
  - `src/memory/bus.rs`
  - `docs/design/ROADMAP.md`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
- 验收结果：
  - 通过：库回归 `267/267`。
  - 新增测试均通过：
    - `peripheral::npu::tests::test_npu_descriptor_dma_batch`
    - `peripheral::lpu::tests::test_lpu_descriptor_dma_batch`
    - `memory::bus::tests::test_bus_npu_descriptor_notify_bridge`
    - `memory::bus::tests::test_bus_lpu_descriptor_notify_bridge`
- 风险/未完成项：
  - 前端暂无 NPU/LPU 寄存器面板与任务时间线；
  - 自定义加速指令路径尚未接入。
- 上下文压缩（供下一步直接续做）：
  - Phase 5 已从“MMIO 骨架”推进到“描述符 DMA + IRQ”可执行闭环；
  - 下一步优先：
    1) 在可视化端新增 NPU/LPU 状态面板与任务计数趋势；
    2) 规划并实现一条最小自定义指令到 NPU/LPU 的 fast-path。

### 2026-04-02 Phase-5-02（NPU/LPU 状态可视化面板）

- 完成内容：
  - 可视化后端新增协处理器查询命令：`npu state` / `lpu state`。
  - `Bus` 增加 `get_npu_snapshot()` / `get_lpu_snapshot()`，统一暴露协处理器快照。
  - 前端新增 Coprocessor 页签与 `CoprocessorPanel`，可展示并刷新 NPU/LPU 的 control/status/opcode/cycles/desc/task 统计。
  - 新增命令解析回归测试，确保命令协议稳定。
- 变更文件：
  - `src/peripheral/npu.rs`
  - `src/peripheral/lpu.rs`
  - `src/peripheral/mod.rs`
  - `src/memory/bus.rs`
  - `src/visualize/server.rs`
  - `frontend/src/components/CoprocessorPanel.tsx`
  - `frontend/src/App.tsx`
  - `frontend/src/types/snapshot.ts`
  - `frontend/src/App.css`
  - `docs/design/ROADMAP.md`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：库回归 `268/268`。
  - 通过：前端构建成功（Vite build）。
- 风险/未完成项：
  - 任务时间线仍未实现（当前为快照刷新模式）；
  - 自定义加速指令路径尚未接入。
- 上下文压缩（供下一步直接续做）：
  - Phase 5 已具备“MMIO + 描述符 DMA + 可视化状态面板”主链路；
  - 下一步优先：
    1) 增加任务时间线（notify/done/error 时间序列）；
    2) 设计并接入最小自定义指令 fast-path（CPU -> NPU/LPU）。

### 2026-04-02 Phase-5-03（CUSTOM-0 自定义指令 fast-path）

- 完成内容：
  - 为指令系统新增 `CUSTOM-0 (0x0B)` opcode 常量。
  - 在 CPU 执行主路径中于通用解码前接入 `execute_custom0()` 分发。
  - 新增最小自定义编码约定（R-type 布局）：
    - `funct3=0` -> NPU；`funct3=1` -> LPU；
    - `funct7[4:0]` -> 协处理器 opcode；
    - `rs1/rs2` -> 输入，`rd` -> 结果。
  - 执行逻辑采用 MMIO fast-path：CPU 将操作数/opcode 写入 NPU/LPU 寄存器，拉起 `START`，同步读取 `RESULT` 并回写寄存器。
  - 增加非法 opcode 拒绝逻辑，避免未知协处理器操作静默退化。
- 变更文件：
  - `src/instruction/opcode.rs`
  - `src/cpu/core.rs`
  - `src/instruction/execute.rs`
  - `docs/design/ROADMAP.md`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
- 验收结果：
  - 通过：库回归 `271/271`。
  - 新增测试均通过：
    - `instruction::execute::tests::test_custom0_npu_add_fast_path`
    - `instruction::execute::tests::test_custom0_lpu_xor_fast_path`
    - `instruction::execute::tests::test_custom0_invalid_opcode_rejected`
- 风险/未完成项：
  - 任务时间线（notify/done/error）仍未实现；
  - 目前 fast-path 为“同步 MMIO 触发”模型，后续可按性能需求演进为异步队列化提交。
- 上下文压缩（供下一步直接续做）：
  - Phase 5 现已具备“MMIO + 描述符 DMA + 可视化状态 + 自定义指令 fast-path”主链路。
  - 下一步优先：补齐任务时间线，并在前端增加趋势图/事件序列展示。

### 2026-04-02 Phase-5-04（NPU/LPU 任务时间线）

- 完成内容：
  - `CoprocessorPanel` 新增任务时间线区块，展示最近协处理器状态变化：`notify/done/error/pending`。
  - 增加时间线增量去重逻辑：状态未变化时不重复入列。
  - 增加协处理器页签自动轮询（1s）以持续采样，形成“快照 -> 趋势”的可观测闭环。
  - 时间线默认保留最近 24 条记录，并显示与上一条的增量变化。
- 变更文件：
  - `frontend/src/components/CoprocessorPanel.tsx`
  - `frontend/src/App.tsx`
  - `frontend/src/App.css`
  - `docs/design/ROADMAP.md`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：库回归 `271/271`。
  - 通过：前端构建成功（Vite build）。
- 风险/未完成项：
  - 当前时间线为前端采样视角，未直接显示每条 descriptor 的详细执行元数据（如地址/opcode 细节）。
- 上下文压缩（供下一步直接续做）：
  - Phase 5 已实现“MMIO + 描述符 DMA + IRQ + 可视化状态 + custom fast-path + 任务时间线”闭环。
  - 下一步可转向 Phase 6：将 xv6→Linux→游戏→NPU/LPU 路线做脚本化串联验收与演示封装。

### 2026-04-02 Phase-6-01（演示链路一键编排脚本）

- 完成内容：
  - 新增 `scripts/run_phase6_showcase_pipeline.ps1`，用于串联以下阶段：
    - `xv6 shell smoke matrix`
    - `Linux Phase3 acceptance`（`-EnableLinux` 可选启用）
    - `Phase4 host-demo + guest-binary acceptance`
    - `NPU/LPU custom fast-path + descriptor bridge` 回归
    - `frontend build`
  - 新增统一阶段汇总与日志归档（`target/phase6-demo-logs`），支持按阶段跳过（`-SkipXv6/-SkipPhase4/...`）。
- 变更文件：
  - `scripts/run_phase6_showcase_pipeline.ps1`
  - `docs/design/ROADMAP.md`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_phase6_showcase_pipeline.ps1 -SkipBuild -SkipXv6 -SkipPhase4 -SkipFrontendBuild`
- 验收结果：
  - 通过：轻量烟测 PASS。
  - 通过：`coprocessor-fastpath-tests`、`coprocessor-dma-bridge-tests` 两阶段在编排脚本内执行成功。
- 风险/未完成项：
  - 目前仅完成轻量烟测，尚未在单轮中完成完整“xv6→Linux→游戏→NPU/LPU”全链路 PASS。
  - Linux 阶段仍依赖外部工件完整性，默认未启用。
- 上下文压缩（供下一步直接续做）：
  - Phase 6 已具备统一入口脚本，下一步可在工件齐全环境执行：
    1) `-EnableLinux` 打开 Linux 阶段；
    2) 执行全量编排并固化最终演示日志；
    3) 将全链路 PASS 结果回填路线图状态为完成。

### 2026-04-02 Phase-6-02（全链路终验 PASS）

- 完成内容：
  - 执行 Phase6 编排脚本全量模式（启用 Linux 阶段），完成从 xv6 到 Linux、再到 Phase4 游戏链路、NPU/LPU 回归与前端构建的一体化验收。
  - 统一输出阶段总结并确认所有 required stage 均通过。
- 变更文件：
  - `docs/design/ROADMAP.md`
  - `docs/MILESTONE_EXECUTION.md`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_phase6_showcase_pipeline.ps1 -EnableLinux`
- 验收结果：
  - 通过：
    - `build-release`
    - `xv6-shell-matrix`
    - `phase4-host-demo`
    - `phase4-guest-demo`
    - `linux-phase3-acceptance`
    - `coprocessor-fastpath-tests`
    - `coprocessor-dma-bridge-tests`
    - `frontend-build`
  - 总结：`[phase6] PASS: showcase pipeline completed (required stages).`
- 风险/未完成项：
  - 无阻塞项；后续以演示体验优化与执行耗时优化为主。
- 上下文压缩（供下一步直接续做）：
  - Phase 6 已完成。下一步可进入发布与演示包装优化（如日志可视化摘要、并行化缩短时长、演示材料模板化）。

### 2026-04-03 Phase-META-02（调研文档并入主线文档）

- 完成内容：
  - 将 `docs/research/CPU_SIMULATOR_RESEARCH_REVIEW_20260331.md` 的可执行结论并入 `docs/design/ROADMAP.md`。
  - 在路线图中新增“调研结论并入”章节，统一收口：对标启示、风险优先级、状态追踪、执行口径。
  - 同步更新 `docs/INDEX.md`，明确“调研结论主入口”已迁移到 `ROADMAP`。
- 变更文件：
  - `docs/design/ROADMAP.md`
  - `docs/INDEX.md`
  - `docs/MILESTONE_EXECUTION.md`
  - `docs/research/CPU_SIMULATOR_RESEARCH_REVIEW_20260331.md`
- 验收命令：
  - 文档合并项（无代码逻辑改动），采用文档一致性检查：入口与索引互相可达。
- 验收结果：
  - 通过：调研报告关键结论在主线文档可直接检索，且原文档保留详版证据。
- 风险/未完成项：
  - 报告中“历史测试数（185/193）”为当时快照，当前基线请以 `ROADMAP` 最新同步校验口径为准。
- 上下文压缩（供下一步直接续做）：
  - 后续新增调研报告时，先并入 `ROADMAP` 的“调研结论并入”章节，再保留原文作详细证据归档。

### 2026-04-16 Phase-5-05（LPU 语言化迁移：ByteTokenize MVP）

- 完成内容：
  - 将 `LPU` 从“纯逻辑协处理器”升级为 **Language Processing Unit**（保留 legacy 逻辑 opcode 兼容）。
  - 新增 Language MVP opcode：`ByteTokenize (0x10)`，支持：
    - `CUSTOM-0` fast-path（`funct3=1` 路由 LPU）；
    - descriptor 批处理路径（`DESC_ADDR/DESC_LEN/DESC_NOTIFY`）。
  - 增加语言统计快照：`bytes_processed`、`tokens_generated`，并接入 `lpu state` 协议与前端面板展示。
- 变更文件：
  - `src/peripheral/lpu.rs`
  - `src/peripheral/mod.rs`
  - `src/instruction/execute.rs`
  - `src/memory/bus.rs`
  - `src/visualize/server.rs`
  - `frontend/src/types/snapshot.ts`
  - `frontend/src/components/CoprocessorPanel.tsx`
  - `frontend/src/components/PipelineVisualizer.tsx`
  - `docs/design/ARCHITECTURE.md`
  - `docs/design/ROADMAP.md`
  - `docs/development/MILESTONE_EXECUTION.md`
  - `docs/guides/DEMO_GUIDE.md`
- 验收命令：
  - `cargo test --lib`
  - `cargo test`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：`cargo test --lib`，`395 passed; 0 failed`。
  - 通过：`cargo test`（含 lib/main/integration/doc tests），全部通过：
    - lib: `395 passed`
    - main: `11 passed`
    - integration: `3 + 3 + 1 passed`
    - doc-tests: `5 passed`
  - 通过：`frontend` 构建成功（`tsc -b && vite build`），产物输出到 `frontend/dist/`。
- 风险/未完成项：
  - `GreedyDecode` 待下一阶段；当前 `EmbeddingBag` 已在同一阶段以 MVP 方式补齐。
- 上下文压缩（供下一步直接续做）：
  - LPU 语言化基础骨架已就位（`ByteTokenize + EmbeddingBag`），下一阶段可在同一 descriptor 协议下增量扩展 `GreedyDecode`。

### 2026-04-16 Phase-5-06（LPU 语言化迁移：EmbeddingBag MVP）

- 完成内容：
  - 新增 Language opcode：`EmbeddingBag (0x11)`。
  - 执行语义（MVP）：
    - 单次模式：`op_a` 作为 token id，返回固定 embedding 表 lookup 值；
    - descriptor 模式：`[opcode, input_addr, bag_len, output_addr]`，对 u32 token id 序列执行 sum pooling。
  - 新增语言统计：`embedding_lookups`、`embedding_bags`，贯通后端 `lpu_state` 与前端展示。
  - `CUSTOM-0` fast-path 新增 `0x11` 放行并补回归。
- 变更文件：
  - `src/peripheral/lpu.rs`
  - `src/peripheral/mod.rs`
  - `src/instruction/execute.rs`
  - `src/memory/bus.rs`
  - `src/visualize/server.rs`
  - `frontend/src/types/snapshot.ts`
  - `frontend/src/components/CoprocessorPanel.tsx`
  - `docs/design/ARCHITECTURE.md`
  - `docs/design/ROADMAP.md`
  - `docs/development/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
  - `cargo test`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：`cargo test --lib`，`399 passed; 0 failed`。
  - 通过：`cargo test`（含 lib/main/integration/doc tests），全部通过：
    - lib: `399 passed`
    - main: `11 passed`
    - integration: `3 + 3 + 1 passed`
    - doc-tests: `5 passed`
  - 通过：`frontend` 构建成功（`tsc -b && vite build`），产物输出到 `frontend/dist/`。
- 风险/未完成项：
  - 当前 EmbeddingBag 为固定 embedding 表 MVP；可配置 embedding table / 多维向量池化待后续阶段。
- 上下文压缩（供下一步直接续做）：
  - 语言路径现已具备 tokenizer + bag pooling 的最小闭环，下一步可在此基础上增加 decode 类算子与可配置词表。

### 2026-04-16 Phase-5-07（LPU 语言化迁移：GreedyDecode MVP）

- 完成内容：
  - 新增 Language opcode：`GreedyDecode (0x12)`。
  - 执行语义（MVP）：
    - 单次模式：`op_a/op_b` 作为两个候选分数，输出 token id `0|1`（二元 argmax）；
    - descriptor 模式：`[opcode, input_addr, vocab_size, output_addr]`，对 `vocab_size` 个 u32 分数执行 argmax，写回 token id。
  - 新增解码统计：`decode_candidates_evaluated`、`decoded_tokens`，贯通后端 `lpu_state` 与前端展示。
  - `CUSTOM-0` fast-path 新增 `0x12` 放行并补回归。
- 变更文件：
  - `src/peripheral/lpu.rs`
  - `src/peripheral/mod.rs`
  - `src/instruction/execute.rs`
  - `src/memory/bus.rs`
  - `src/visualize/server.rs`
  - `frontend/src/types/snapshot.ts`
  - `frontend/src/components/CoprocessorPanel.tsx`
  - `docs/design/ARCHITECTURE.md`
  - `docs/design/ROADMAP.md`
  - `docs/development/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
  - `cargo test`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：`cargo test --lib`，`403 passed; 0 failed`。
  - 通过：`cargo test`（含 lib/main/integration/doc tests），全部通过：
    - lib: `403 passed`
    - main: `11 passed`
    - integration: `3 + 3 + 1 passed`
    - doc-tests: `5 passed`
  - 通过：`frontend` 构建成功（`tsc -b && vite build`），产物输出到 `frontend/dist/`。
- 风险/未完成项：
  - 当前 GreedyDecode 为 argmax MVP；采样策略（top-k/top-p/temperature）与可配置词表待后续阶段。
- 上下文压缩（供下一步直接续做）：
  - LPU 语言路径已形成 `tokenize + embedding + decode` 最小闭环，下一步建议扩展可配置词表与采样解码策略。

### 2026-04-16 Phase-5-08（LPU 语言化迁移：TopKSampleDecode MVP）

- 完成内容：
  - 新增 Language opcode：`TopKSampleDecode (0x13)`。
  - 执行语义（MVP）：
    - 单次模式：在二元候选（`op_a/op_b`）上执行可配置 top-k + temperature 采样；
    - descriptor 模式：`[opcode, input_addr, vocab_size, output_addr]`，在 `vocab_size` 个候选分数上执行 top-k + temperature 采样。
  - 新增采样解码参数寄存器：
    - `decode_top_k`（`0x3C`）
    - `decode_temperature_milli`（`0x40`，1000=1.0）
    - `decode_seed`（`0x44`）
  - 新增采样统计：`sampled_decodes`，并贯通后端 `lpu_state` 与前端展示。
  - `CUSTOM-0` fast-path 新增 `0x13` 放行并补回归。
- 变更文件：
  - `src/peripheral/lpu.rs`
  - `src/peripheral/mod.rs`
  - `src/instruction/execute.rs`
  - `src/memory/bus.rs`
  - `src/visualize/server.rs`
  - `frontend/src/types/snapshot.ts`
  - `frontend/src/components/CoprocessorPanel.tsx`
  - `docs/design/ARCHITECTURE.md`
  - `docs/design/ROADMAP.md`
  - `docs/development/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
  - `cargo test`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：`cargo test --lib`，`407 passed; 0 failed`。
  - 通过：`cargo test`（含 lib/main/integration/doc tests），全部通过：
    - lib: `407 passed`
    - main: `11 passed`
    - integration: `3 + 3 + 1 passed`
    - doc-tests: `5 passed`
  - 通过：`frontend` 构建成功（`tsc -b && vite build`），产物输出到 `frontend/dist/`。
- 风险/未完成项：
  - 当前为线性权重近似采样 MVP；softmax / top-p 采样与可配置词表仍在后续阶段。
- 上下文压缩（供下一步直接续做）：
  - LPU 语言路径已具备 `argmax + top-k/temperature 采样` 两类解码策略，下一步可扩展 top-p 与词表管理。

### 2026-04-16 Phase-5-09（LPU 语言化迁移：TopPSampleDecode MVP）

- 完成内容：
  - 新增 Language opcode：`TopPSampleDecode (0x14)`。
  - 执行语义（MVP）：
    - 单次模式：在二元候选（`op_a/op_b`）上执行可配置 top-p (nucleus) + temperature 采样；
    - descriptor 模式：`[opcode, input_addr, vocab_size, output_addr]`，在 `vocab_size` 个候选分数上执行 top-p 采样。
  - 新增 Top-p 配置/统计寄存器：
    - `decode_top_p_milli`（`0x4C`，900=0.9）
    - `nucleus_decodes`（`0x50`）
  - 贯通后端 `lpu_state` 与前端展示字段；`CUSTOM-0` fast-path 新增 `0x14` 放行并补回归。
- 变更文件：
  - `src/peripheral/lpu.rs`
  - `src/peripheral/mod.rs`
  - `src/instruction/execute.rs`
  - `src/memory/bus.rs`
  - `src/visualize/server.rs`
  - `frontend/src/types/snapshot.ts`
  - `frontend/src/components/CoprocessorPanel.tsx`
  - `docs/design/ARCHITECTURE.md`
  - `docs/design/ROADMAP.md`
  - `docs/development/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
  - `cargo test`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过（本条目对应代码与测试已补齐，回归结果见本次执行记录）。
- 风险/未完成项：
  - 当前为线性权重近似 nucleus 采样 MVP；softmax/logit 归一化与动态词表管理仍在后续阶段。
- 上下文压缩（供下一步直接续做）：
  - LPU 语言路径已具备 `argmax + top-k + top-p` 三类解码策略，下一步可聚焦词表管理、softmax 采样精度与批量性能优化。

### 2026-04-16 Phase-5-10（LPU 去 Legacy + 协处理器拓扑 V2 规划）

- 完成内容：
  - 移除 LPU legacy 逻辑 opcode（`0~5`）执行路径：
    - descriptor 模式不再 fallback 到逻辑运算；未知 opcode 计入 `tasks_error`；
    - 单次模式未知 opcode 计入 `tasks_error` 并返回 `result=0`。
  - `CUSTOM-0` LPU fast-path 移除 legacy 放行，仅允许语言 opcode（`0x10~0x14`）。
  - 前端 `CoprocessorPanel` 移除 LPU legacy opcode 名称映射。
  - 架构层面完成 NPU/LPU/GPU/TPU V2 规划并写入 `ARCHITECTURE.md`：
    - 控制面 / 数据面 / 事件面分离；
    - 统一 `ACC_CTRL_ROOT + ACC_DOORBELL + Engine CTRL` 映射；
    - 明确 V1→V2 双地址窗口迁移策略。
- 变更文件：
  - `src/peripheral/lpu.rs`
  - `src/instruction/execute.rs`
  - `src/memory/bus.rs`
  - `frontend/src/components/CoprocessorPanel.tsx`
  - `docs/design/ARCHITECTURE.md`
  - `docs/design/ROADMAP.md`
  - `docs/development/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
  - `npm run build`（`frontend/`）
- 验收结果：
  - 通过：`cargo test --lib`，`410 passed; 0 failed`。
  - 通过：`frontend` 构建成功（`tsc -b && vite build`，`built in 362ms`）。
- 风险/未完成项：
  - V2 拓扑与新地址映射目前是文档规划，尚未进入运行时代码迁移；
  - 仍需在后续阶段落地 Doorbell 汇聚中断与双地址窗口别名。
- 上下文压缩（供下一步直接续做）：
  - 语义层已完成“LPU 纯语言化”；下一步可按 `P5.3` 清单实现 V2 控制面与地址映射迁移。

### 2026-04-16 Phase-5-11（V2 双窗口 + 统一 Doorbell ABI 落地）

- 完成内容：
  - 在 `Bus` 引入统一控制面寄存器窗口：
    - `ACC_CTRL_ROOT`（`0x2000_0000`，保留低 `0x100` legacy NPU 子窗口）；
    - `ACC_DOORBELL`（`0x2000_1000`，保留低 `0x100` legacy LPU 子窗口）。
  - 落地 V2 engine overlay 双地址窗口（由 `ACC_CTRL_ROOT.MODE[0]` 控制）：
    - `0x2001_0000`→NPU、`0x2001_1000`→LPU、`0x2001_2000`→GPU、`0x2001_3000`→TPU。
  - 落地 Doorbell 统一提交 ABI：`engine + desc_addr + desc_len + notify`，并桥接到各引擎 descriptor 通知寄存器。
  - `ACC_CTRL_ROOT` 增加统一汇总寄存器：`engine_mask`、`irq_summary`、`doorbell_submits/completes/errors`。
- 变更文件：
  - `src/memory/bus.rs`
  - `docs/design/ARCHITECTURE.md`
  - `docs/design/ROADMAP.md`
  - `docs/development/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
- 验收结果：
  - 通过：`cargo test --lib`，`413 passed; 0 failed`。
  - 新增用例通过：
    - `memory::bus::tests::test_bus_acc_v2_overlay_window_for_npu`
    - `memory::bus::tests::test_bus_acc_doorbell_submit_npu_descriptor`
    - `memory::bus::tests::test_bus_acc_doorbell_invalid_engine_records_error`
- 风险/未完成项：
  - 当前仍处于迁移态，V1 alias 尚未移除（`P5.3` 最后一项仍待完成）。
- 上下文压缩（供下一步直接续做）：
  - V2 控制面与双窗口已可运行，下一步可进入“工具链默认切 V2 + 收敛移除 V1 alias”的收口阶段。

### 2026-04-16 Phase-5-12（P5.3 收敛：移除 V1 alias，切换纯 V2 拓扑）

- 完成内容：
  - 协处理器基址统一切换到纯 V2 拓扑：
    - `NPU_BASE=0x2001_0000`
    - `LPU_BASE=0x2001_1000`
    - `GPU_BASE=0x2001_2000`
    - `TPU_BASE=0x2001_3000`
  - `Bus` 移除 V1 alias/overlay 迁移逻辑：
    - 删除 `translate_v2_engine_addr()` 迁移翻译路径；
    - `ACC_CTRL_ROOT/ACC_DOORBELL` 低 `0x100` 子窗口不再透传 legacy engine 寄存器。
  - `ACC_CTRL_ROOT.MODE` 收敛为固定 V2 启用态（写入不再关闭 V2）。
  - 回归测试更新为纯 V2 语义：
    - `test_bus_acc_pure_v2_window_for_npu`（替代原 overlay 启用测试）。
- 变更文件：
  - `src/peripheral/npu.rs`
  - `src/peripheral/lpu.rs`
  - `src/peripheral/gpu.rs`
  - `src/peripheral/tpu.rs`
  - `src/memory/bus.rs`
  - `docs/design/ROADMAP.md`
  - `docs/design/ARCHITECTURE.md`
  - `docs/guides/GPU_TPU_API.md`
  - `docs/development/MILESTONE_EXECUTION.md`
- 验收命令：
  - `cargo test --lib`
  - `cargo test`
- 验收结果：
  - 通过：`cargo test --lib`，`413 passed; 0 failed`。
  - 通过：`cargo test` 全通过（lib/main/integration/doc-tests）。
- 风险/未完成项：
  - 无阻塞项；P5.3 最后一项已完成。
- 上下文压缩（供下一步直接续做）：
  - 协处理器控制面已稳定在纯 V2 拓扑，可进入后续 Hybrid Offload 与性能优化阶段。
