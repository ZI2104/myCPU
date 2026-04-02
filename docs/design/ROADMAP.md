# myCPU 开发路线图

## 课设周次安排（第 4-8 周）

| 周次  | 阶段      | 内容                    | 关键产出                          |
| ----- | --------- | ----------------------- | --------------------------------- |
| 第4周 | Phase 1-2 | 基础框架 + RV32I 指令集 | 可编译骨架、40 条基础指令可执行   |
| 第5周 | Phase 3   | 5 级流水线与冒险处理    | IF/ID/EX/MEM/WB 全链路与前递/暂停 |
| 第6周 | Phase 4   | 特权级、异常与中断系统  | M/S/U、CSR、CLINT/PLIC 稳定       |
| 第7周 | Phase 5   | 外设与调试能力          | UART/ELF/GDB/DiffTest 闭环        |
| 第8周 | Phase 6   | 演示封装与回归矩阵      | xv6→Linux→Phase4→NPU/LPU 一键验收 |

> 说明：该表用于对齐课程要求的周次口径（第 4-8 周）。

## 阶段依赖关系图

```text
Phase 1: 基础框架 ✅
    ↓
Phase 2: 指令集 ✅
    ↓
Phase 3: 流水线/OS Bring-up ✅
    ↓
Phase 4: 特权+异常+输入/渲染 ✅
    ↓
Phase 5: NPU/LPU ✅
    ↓
Phase 6: 一键编排验收 ✅
    ↓
Phase 7: CI/发布工程化（规划）
```

## 编号与排序统一规范

- Phase 统一采用：`Phase 0`、`Phase 1`…`Phase 6`（按数字递增）。
- Step 统一采用：`Step <Phase>.<序号>`（示例：`Step 4.1`、`Step 4.2`）。
- 里程碑记录统一采用：`Phase-<phase>-<index>`（示例：`Phase-6-02`）。
- 文档中若出现历史别名（如 `P0/P1`），默认视为对应 Phase 的子项，不再作为并列主阶段。

---

## OS Bring-up 里程碑（用户目标对齐，持续执行中）

> 更新日期：2026-04-02
>
> 说明：本节用于对齐“xv6 → Linux → 游戏 → NPU/LPU”路线，状态分为：
>
>
> - ✅ 已完成
> - 🟡 部分完成
> - ⏳ 未完成

| 阶段    | 目标（用户要求）                                           | 当前状态   | 验收标准                                              | 当前证据                                                                                                                                                                  |
| ------- | ---------------------------------------------------------- | ---------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 0 | 冻结可用基线 + OS bring-up 专项测试入口                    | ✅ 已完成   | xv6 到 shell prompt 作为首个里程碑                    | 基线与 bring-up smoke 测试入口已落地，且已在长窗口运行中稳定看到 xv6 shell 提示符 `$`                                                                                     |
| Phase 1 | xv6 启动关键能力补齐（RV32M、Trap闭环、Sv32、MMU统一路径） | ✅ 已完成   | xv6 内核入口与串口输出稳定                            | RV32M/Trap/Sv32/MMU 路径均已实现并有测试，详见本文件 P1/P3 验收段                                                                                                         |
| Phase 2 | xv6 可交互运行（CLINT/PLIC 稳定 + 最小块设备）             | ✅ 已完成   | xv6 文件系统镜像进入 shell 可交互                     | 已完成 PLIC/SIP 兼容修复与 VirtIO 描述符传输修复（含 1KiB 场景），并新增 `scripts/run_xv6_shell_smoke.ps1` 自动验收流程；200M 窗口稳定通过 `echo/ls/cat/grep/wc` 命令矩阵 |
| Phase 3 | 精简 Linux 启动链路（SBI + FDT + 启动参数）                | 🟡 部分完成 | Buildroot Linux 进入 init/userland                    | 模拟器侧链路能力已齐全（SBI/FDT/bootargs/payload）；默认仓库不内置完整 Linux 工件，需通过脚本准备 OpenSBI/Buildroot 工件后完成终验                                        |
| Phase 4 | Linux 用户态 SDL/FB 游戏演示                               | ✅ 已完成   | 简化 2D 游戏跑通 + 输入设备 + Overlay                 | `fb_game` 游戏流程控制（init/step/run/reset）+ 输入面板 + Framebuffer Overlay 已落地；host/guest 自动验收脚本通过，库测 274/274 与前端构建通过                            |
| Phase 5 | NPU/LPU 模拟（MMIO 优先）                                  | ✅ 已完成   | CTRL/STATUS/DESC_ADDR/IRQ + 描述符/DMA + IRQ 完整闭环 | NPU/LPU 已补齐 DESC_ADDR/描述符 DMA/Bus 桥接、可视化状态面板、CUSTOM-0 自定义指令 fast-path 与任务时间线                                                                  |
| Phase 6 | 亮点封装发布（脚本化演示+回归矩阵）                        | ✅ 已完成   | xv6→Linux→游戏→NPU 对比演示可复现                     | 在具备 Linux 工件并启用 `-EnableLinux` 的条件下，`run_phase6_showcase_pipeline.ps1` 全链路通过：xv6、Linux、Phase4 host/guest、NPU/LPU 回归与前端构建均 PASS              |

> 最新进展（2026-04-01）：在补齐 PLIC S 态窗口与 `SIP` 机器态 `SSIP` 语义后，进一步完成 RV32C 兼容修复与 VirtIO 描述符传输长度修复（避免 512B 截断导致用户程序加载不完整）。UART 注入升级为 prompt 逐命令自适应分片注入，并与会话状态机断言结合，`scripts/run_xv6_shell_smoke.ps1` 在 200M 长窗口下 `echo/ls/cat/grep/wc` 命令矩阵稳定通过（5/5）。此外，`run` 子命令已支持 Linux 启动上下文注入（`--linux-boot`、`--linux-hartid`、`--linux-dtb*`、`--linux-bootargs*`）、`--linux-sbi/--linux-sbi-addr/--linux-payload-addr` 双镜像加载，以及 `--linux-auto-dtb` 自动 FDT 生成，链路级实跑验收通过；并已新增 MMIO 输入外设（`0x10002000`）+ 可视化输入命令（`input ...`）+ 前端 `InputPanel`。当前已补齐 `GameFlowPanel`（`fb_game` 流程控制）、`Framebuffer` Overlay HUD（FPS/IPC/Stalls/Tick/Score/InputBits），并增强 `scripts/run_phase4_input_framebuffer_acceptance.ps1` 的 host/guest 断言（`fb_game state` 与 `stepn executed`）。针对 Phase 3，新增 `scripts/setup_phase3_artifacts.ps1`（自动下载 OpenSBI）、`scripts/build_phase3_buildroot_artifacts.ps1`（Buildroot qemu_riscv32_virt 工件产出）与 `run_linux_phase3_acceptance.ps1` 工件自动探测/前置校验（ELF payload 拦截），可在缺失 Linux Image 时给出明确阻塞诊断。`cargo test --lib` 当前 `274/274` 通过，前端 `npm run build` 通过。
> 复验同步（2026-04-01）：已基于当前工作区状态再次执行 Phase 4 host/guest 双模式验收，结果均 PASS；并复跑 `cargo test --lib`（274/274）与前端 `npm run build`，结果均通过。
> 本轮同步校验（2026-04-02）：再次执行 `cargo test --lib`，结果 `274 passed, 0 failed`；工作区最近一次 `cargo test --test npu_elf_integration -- --nocapture` 退出码为 `0`。

### 执行约定（从本次开始）

- 每完成一项：
  1. 在 `docs/development/MILESTONE_EXECUTION.md` 追加“完成记录”；
  2. 写入“上下文压缩”摘要（便于后续连续推进）；
  3. 执行对应测试并记录验收结果（通过/失败+关键输出）。

---

## 调研结论并入（来自 `CPU_SIMULATOR_RESEARCH_REVIEW_20260331.md`）

> 更新时间：2026-04-03  
> 目的：把调研报告中的“对标结论 + 风险优先级 + 改进路线”合并到主线路线图，避免信息分散。

### 对标项目启示（NEMU / QEMU / Spike）

- **NEMU 启示**：教学项目应优先保证“可验证性闭环”（DiffTest + 调试器 + 快照）。
- **QEMU 启示**：调试能力应从“可用”推进到“可集成”（GDB/RSP 完整性），性能能力应从“展示”推进到“可归因”。
- **Spike 启示**：语义正确性回归应体系化（分层对比、失败复盘），并保持与工具链标准接口兼容。

### 风险项与当前状态（调研优先级映射）

| 调研项                         | 原优先级 | 当前状态                 | 对应落位                  |
| ------------------------------ | -------- | ------------------------ | ------------------------- |
| ELF Loader unsafe 生命周期转换 | 高       | ✅ 已修复                 | `src/loader/mod.rs`       |
| GDB RSP 核心读写命令占位实现   | 高       | ✅ 已落地真实数据通路     | `src/debug/mod.rs`        |
| ELF entry 未驱动启动 PC        | 高       | ✅ 已修复                 | `src/main.rs`             |
| CLINT 非对齐访问语义偏宽松     | 中       | ✅ 已收敛                 | `src/interrupt/clint.rs`  |
| 可视化历史 O(n) 头删           | 中       | ✅ 已优化为 `VecDeque`    | `src/visualize/server.rs` |
| Bus 地址路由线性扫描           | 低       | 🟡 保留（规模扩大后优化） | `src/memory/bus.rs`       |

### 路线图执行优先级（并入后口径）

- **P0（正确性/可调试性）**：ELF 启动语义、GDB RSP、CLINT 对齐语义（已完成）。
- **P1（可观测/性能）**：DiffTest 上下文增强、历史队列性能优化、停顿归因指标（已完成）。
- **P2（扩展/长期）**：M/C 扩展、回归集常态化、文档与 lint 治理（持续推进）。

### 使用约定

- 本节作为“调研结论主线入口”；`CPU_SIMULATOR_RESEARCH_REVIEW_20260331.md` 保留详版复盘与证据细节。
- 后续如新增类似调研文档，优先先合并到本节再归档原文，保持主线可检索与可执行。

---

## Phase 1: 基础框架 ✅ 完成

### 任务清单

- [x] 项目初始化 (Cargo 配置)
- [x] 基础 trait 定义
  - [x] `Memory` trait (read/write)
  - [x] `Peripheral` trait
- [x] 内存模块
  - [x] RAM 实现
  - [x] ROM 实现
  - [x] 总线 (Bus) 实现
- [x] CPU 寄存器组
  - [x] 通用寄存器 x0-x31
  - [x] PC 寄存器
- [x] 主循环框架
  - [x] 单周期执行循环
  - [x] 时钟计数
- [x] 错误处理
  - [x] SimError 类型定义
  - [x] Result 类型别名

### 产出

- ✅ 可编译运行的空模拟器
- ✅ 基础内存读写测试通过 (35 个测试全部通过)

---

## Phase 2: 指令集实现 ✅ 完成

### 目标

实现 RV32I 基础指令集 (40 条)，可运行简单程序。

### RV32I 指令清单

- [x] AND  - 与
- [x] OR   - 或
- [x] XOR  - 异或
- [x] SLL  - 逻辑左移
- [x] SRL  - 逻辑右移
- [x] SRA  - 算术右移

- [x] SLT  - 有符号小于比较
- [x] SLTU - 无符号小于比较

#### I-type (14 条)

- [x] ADDI  - 加立即数
- [x] ANDI  - 与立即数
- [x] ORI   - 或立即数
- [x] XORI  - 异或立即数
- [x] SLTI  - 有符号小于比较立即数
- [x] SLTIU - 无符号小于比较立即数
- [x] SLLI  - 逻辑左移立即数
- [x] SRLI  - 逻辑右移立即数
- [x] SRAI  - 算术右移立即数
- [x] LB    - 加载字节
- [x] LH    - 加载半字

- [x] LW    - 加载字
- [x] LBU   - 加载无符号字节
- [x] LHU   - 加载无符号半字

#### S-type (3 条)

- [x] SB - 存储字节
- [x] SH - 存储半字
- [x] SW - 存储字

#### B-type (6 条)

- [x] BEQ  - 相等跳转

- [x] BNE  - 不等跳转
- [x] BLT  - 有符号小于跳转
- [x] BGE  - 有符号大于等于跳转
- [x] BLTU - 无符号小于跳转

- [x] BGEU - 无符号大于等于跳转

#### U-type (2 条)

- [x] LUI   - 加载高位立即数
- [x] AUIPC - PC 加高位立即数

#### J-type (2 条)

- [x] JAL  - 跳转并链接
- [x] JALR - 跳转并链接寄存器

#### System (3 条)

- [x] EBREAK - 断点
- [x] FENCE - 内存屏障

### 产出

- ✅ 所有 RV32I 指令测试通过 (67 个单元测试)
- ✅ 可运行简单算术程序

---

## Phase 3: 流水线实现 ✅ 完成

### 目标

实现 5 级流水线，处理数据冒险和控制冒险。

### 任务清单

- [x] 流水线寄存器
  - [x] IF/ID 寄存器
  - [x] ID/EX 寄存器
  - [x] EX/MEM 寄存器
  - [x] MEM/WB 寄存器
- [x] 各阶段实现
  - [x] IF (Instruction Fetch)
  - [x] ID (Instruction Decode)
  - [x] EX (Execute)
  - [x] MEM (Memory Access)
  - [x] WB (Write Back)
- [x] 冒险处理
  - [x] 数据冒险检测
  - [x] 前递逻辑 (EX/MEM, MEM/WB)
  - [x] Load-Use 暂停
  - [x] 控制冒险 - 静态预测 (Predict Not Taken)
  - [x] 分支冲刷

### 产出

- ✅ 流水线测试通过 (115 个测试全部通过)
- ✅ Load-Use 冒险正确暂停

## Phase 4: 特权级与异常 ✅ 完成

### 目标

实现 M/S/U 三级特权模式，支持异常和中断处理。

### 任务清单

- [x] CSR 寄存器
  - [x] M-mode: mstatus, mtvec, mepc, mcause, mie, mip, mscratch, misa, mtval, mideleg, medeleg
  - [x] S-mode: sstatus, stvec, sepc, scause, sie, sip, sscratch, stval
  - [x] U-mode: ustatus, utvec, uepc, ucause
  - [x] CSR 访问指令 (csrrw, csrrs, csrrc, csrrwi, csrrsi, csrrci)
- [x] 特权级切换
  - [x] ecall 指令 (单周期 CPU)
  - [x] mret 指令 (单周期 CPU + 流水线)
  - [x] sret/uret 指令 (框架已实现)
  - [x] 特权级检查 (CSR 访问权限)
  - [x] 流水线 CPU 中的 CSR 指令支持
- [x] 异常处理
  - [x] 异常入口 (xtvec) - 单周期 CPU + 流水线
  - [x] 上下文保存/恢复 - 单周期 CPU + 流水线

  - [x] 异常返回 - 单周期 CPU + 流水线 (mret)
- [x] 中断系统 - CLINT
  - [x] CLINT 实现 (mtime, mtimecmp, msip)
  - [x] InterruptSource trait
  - [x] 中断同步到 MIP (单周期 CPU + 流水线)
  - [x] 中断优先级处理
- [x] 中断系统 - PLIC
  - [x] PLIC 实现 (外部中断控制器)
  - [x] PLIC 与 Bus 集成
  - [x] PLIC 与 CPU 集成 (MEIP/SEIP)
  - [x] 中断委托 (M → S) via mideleg/medeleg

### 产出

- ✅ CSR 寄存器测试通过
- ✅ CLINT 测试通过 (7 个测试)
- ✅ PLIC 测试通过 (8 个测试)
- ✅ 单周期 CPU 支持完整中断和异常处理

- ✅ 流水线 CPU 支持 CSR 指令和 mret

- ✅ 中断委托机制实现 (M-mode → S-mode)

---

## Phase 5: 外设与调试 ✅ 完成

### 目标

实现 UART 串口输出，支持程序加载，实现 GDB 调试接口。

### 任务清单

- [x] UART (NS16550A 兼容)
  - [x] 发送/接收寄存器 (THR/RBR)
  - [x] 状态寄存器 (LSR)
  - [x] 中断支持 (IER/IIR)
  - [x] FIFO Control Register (FCR)
  - [x] Line Control Register (LCR)
  - [x] Modem Control Register (MCR)
  - [x] Scratch Register (SCR)
  - [x] Divisor Latch (DLL/DLM)
- [x] Timer (已在 Phase 4 实现于 CLINT)
  - [x] mtime 寄存器
  - [x] mtimecmp 寄存器
  - [x] 时钟中断
- [x] ELF 加载器
  - [x] 解析 ELF 头 (使用 goblin crate)

  - [x] 加载程序段
  - [x] 设置入口点
  - [x] BSS 段零填充
- [x] GDB Remote Protocol (核心)
  - [x] TCP Server 基础框架
  - [x] 基础命令: ?, g, G, m, M, c, s
  - [x] 断点支持: Z0, z0
  - [x] 查询命令: qSupported, qAttached
  - [x] VSCode 集成配置 (.vscode/launch.json)
- [x] DiffTest 框架
  - [x] QEMU GDB Stub 集成
  - [x] 状态对比逻辑
  - [x] 错误报告与日志

### 产出

- ✅ UART 串口模块实现 (NS16550A 兼容)
- ✅ ELF 加载器实现 (使用 goblin crate)
- ✅ GDB 调试服务器框架
- ✅ VSCode 调试配置
- ✅ DiffTest 框架实现
- ✅ CLI 支持 `run` 和 `debug` 子命令

---

## Phase 6: 加分项 (可选)

> 说明：本章节中按日期记录的“里程碑验收通过数”属于当时快照，用于追溯演进；当前基线状态以本文件顶部“OS Bring-up 里程碑”中的最近同步校验为准。

### P0: 性能监控 ✅ 完成

- [x] RISC-V HPM CSR 寄存器
  - [x] mcycle/mcycleh (周期计数器)
  - [x] minstret/minstreth (指令计数器)
  - [x] mhpmcounter3-31 (可编程计数器)
  - [x] mhpmevent3-31 (事件选择器)
  - [x] mcountinhibit (计数器禁止)
- [x] 性能事件收集器 (PerfCollector)
  - [x] 周期计数 (Cycles)
  - [x] 指令退休计数 (InstructionsRetired)
  - [x] Load-Use 暂停计数

  - [x] 控制冒险计数
  - [x] 分支统计 (Taken/NotTaken)
  - [x] 内存访问统计
- [x] 流水线性能集成
  - [x] 单周期 CPU 集成
  - [x] 流水线 CPU 集成
  - [x] CSR HPM 计数器更新
- [x] 性能报告生成
  - [x] IPC/CPI 计算

  - [x] 暂停率统计
  - [x] 分支预测准确率
  - [x] 格式化输出
- [x] CLI 集成

  - [x] --perf-report 选项

### 产出

- ✅ 性能计数器 CSR 实现 (符合 RISC-V HPM 规范)
- ✅ PerfCollector 事件收集器
- ✅ PerfReport 格式化报告
- ✅ CLI --perf-report 选项
- ✅ 182 个测试全部通过

---

### P1: M 扩展 (乘除法) (可选)

- [x] MUL, MULH, MULHSU, MULHU
- [x] DIV, DIVU, REM, REMU

### P2: C 扩展 (压缩指令) (可选)

- [ ] 16 位压缩指令解码
- [ ] 常用指令的压缩形式

### P3: Sv32 分页 (可选)

- [x] 页表结构（SATP CSR + Sv32 两级页表遍历骨架）
- [ ] TLB 缓存

- [x] 地址翻译入口（单周期 CPU：取指/Load/Store）
- [x] 页错误异常（Instruction/Load/Store Page Fault）

#### P3 当前里程碑验收（2026-03-31）

- 已实现内容：
  - `satp` CSR（RV32: MODE/ASID/PPN）接入 `CsrFile`
  - `src/cpu/mmu.rs`：Sv32 软件页表遍历（Bare 直通）
  - 单周期 CPU 取指、Load/Store 统一接入地址翻译入口
  - 页故障从错误返回改为 trap 路径（`mcause/mtval/mepc` 正确写入）
- 验收测试：
  - `cargo test --lib` 通过（202 passed, 0 failed）
  - 新增验收用例：
    - `test_sv32_instruction_page_fault_enters_trap`
    - `test_sv32_load_page_fault_enters_trap`
- 当前边界：
  - 暂未实现 TLB
  - 暂未实现 A/D 位硬件更新语义与权限细则（SUM/MXR 等）

### P4: 多核支持 (可选)

- [ ] 多个 Hart (硬件线程)
- [ ] 核间中断 (IPI)
- [ ] 共享内存

### P5: NPU/LPU 协处理器 (MMIO 路径)（已完成）

- [x] NPU MMIO 外设骨架（`0x2000_0000`）
- [x] LPU MMIO 外设骨架（`0x2000_1000`）
- [x] 启动命令默认挂载到系统总线（CLI 运行/调试/可视化）
- [x] 中断查询与确认接口（`has_interrupt` / `acknowledge_interrupt`）
- [x] 单元测试覆盖基础算子和中断行为
- [x] DESC_ADDR + 描述符批处理（任务队列）
- [x] Bus 侧 DMA 桥接触发（notify -> RAM 访存执行）
- [x] 可视化前端寄存器面板（NPU/LPU state）
- [x] 自定义指令加速路径（CUSTOM-0，CPU -> NPU/LPU）
- [x] 任务时间线（notify/done/error）

#### P5 里程碑验收（时间线）

#### P5 当前里程碑验收（2026-03-31）

- 新增文件：
  - `src/peripheral/npu.rs`
  - `src/peripheral/lpu.rs`
- 验收测试：
  - `cargo test --lib` 通过（208 passed, 0 failed）
  - `cargo build` 通过

#### P5 当前里程碑验收（2026-04-02）

- 新增能力：
  - NPU/LPU 新增描述符寄存器：`DESC_ADDR/DESC_LEN/DESC_NOTIFY` 与任务统计寄存器。
  - NPU/LPU 支持按描述符批处理执行（opcode + opA_addr + opB_addr + dst_addr）。
  - `Bus::write_byte` 新增 NPU/LPU pending-notify 桥接：通知触发后由总线代执行 RAM 读写（DMA 风格）。
  - PLIC pending 同步新增 NPU/LPU IRQ 源映射（保留 VirtIO/UART 兼容行为）。
- 新增测试：
  - `peripheral::npu::tests::test_npu_descriptor_dma_batch`
  - `peripheral::lpu::tests::test_lpu_descriptor_dma_batch`
  - `memory::bus::tests::test_bus_npu_descriptor_notify_bridge`
  - `memory::bus::tests::test_bus_lpu_descriptor_notify_bridge`
- 验收测试：
  - `cargo test --lib` 通过（267 passed, 0 failed）
- 当前边界：
  - 前端尚未提供 NPU/LPU 寄存器面板与任务时间线；
  - 自定义加速指令路径尚未接入。

#### P5 当前里程碑验收（2026-04-02-R2）

- 新增能力：
  - 可视化后端新增命令：`npu state`、`lpu state`，返回结构化协处理器状态快照。
  - 总线新增快照查询接口：`get_npu_snapshot()`、`get_lpu_snapshot()`。
  - 前端新增 Coprocessor 页签与 `CoprocessorPanel`，可手动/批量刷新 NPU/LPU 关键寄存器与任务统计。
- 新增测试：
  - `visualize::server::tests::test_parse_npu_lpu_state_commands`
- 验收测试：
  - `cargo test --lib` 通过（268 passed, 0 failed）
  - `frontend` 构建通过（`npm run build`）
- 当前边界：
  - “任务时间线”仍为后续增强项；
  - 自定义加速指令路径尚未接入。

#### P5 当前里程碑验收（2026-04-02-R3）

- 新增能力：
  - 新增 `CUSTOM-0 (0x0B)` 指令路径，编码采用 R-type 布局：`funct3=0` 路由 NPU、`funct3=1` 路由 LPU。
  - `funct7[4:0]` 作为协处理器 opcode，`rs1/rs2` 作为输入操作数，执行结果回写 `rd`。
  - CPU 执行路径在通用解码前接入 `execute_custom0()`，通过 MMIO 快速驱动 NPU/LPU 完成一次同步运算。
- 新增测试：
  - `instruction::execute::tests::test_custom0_npu_add_fast_path`
  - `instruction::execute::tests::test_custom0_lpu_xor_fast_path`
  - `instruction::execute::tests::test_custom0_invalid_opcode_rejected`
- 验收测试：
  - `cargo test --lib` 通过（271 passed, 0 failed）
- 当前边界：
  - “任务时间线”仍为后续增强项。

#### P5 当前里程碑验收（2026-04-02-R4）

- 新增能力：
  - 前端 `CoprocessorPanel` 新增任务时间线视图，按时间记录 `notify/done/error/pending` 变化。
  - 协处理器页签新增自动轮询刷新（1s），持续沉淀时间线点位。
  - 时间线采用增量去重与最近窗口保留（最近 24 条），便于观察任务趋势。
- 变更文件：
  - `frontend/src/components/CoprocessorPanel.tsx`
  - `frontend/src/App.tsx`
  - `frontend/src/App.css`
- 验收测试：
  - `cargo test --lib` 通过（271 passed, 0 failed）
  - `frontend` 构建通过（`npm run build`）
- 当前边界：
  - Phase 5 主链路能力已闭环，后续以性能优化与可观测性增强为主。

### P5.1: Hybrid Offload（规划中）

> 来源：`docs/HYBRID_OFFLOAD.md`（草案，2026-04-03 并入路线图）

- [ ] 建立 CPU/NPU 混合调度决策（算子类型、数据规模、对齐、stride、时延预算）
- [ ] 在保持 16B descriptor ABI 兼容前提下扩展标志位（`ASYNC_FLAG`、`WIDE_FLAG`、`STRIDE_FLAG`）
- [ ] 在现有同步 MMIO 路径之外新增异步队列路径（后台 worker + 完成中断）
- [ ] 完善错误上报与回退语义（`tasks_error`/`REG_STATUS`/IRQ + CPU fallback）
- [ ] 建立批量阈值基准（建议从 `>128` 元素起测）并形成调优策略

#### P5.1 最小可行实施分期

- **Phase A（文档/ABI）**：先引入 `ASYNC_FLAG` 定义，不改变默认同步执行语义。
- **Phase B（运行时）**：当设置 `ASYNC_FLAG` 时，descriptor 入后台队列并立即返回；worker 完成后更新 `REG_TASKS_DONE/REG_STATUS` 并置 IRQ pending。
- **Phase C（性能）**：按批量大小做基准，确定 offload 阈值与小批次合并策略。

#### P5.1 兼容性约束

- 默认路径保持现有同步行为，不破坏已落地测试与演示链路。
- 新能力通过标志位渐进启用，支持快速回滚到同步路径。

### P6: Linux + SDL/Framebuffer 演示链路（已完成）

- [x] 可视化后端支持 `framebuffer/fb` 命令（读取内存并转换 RGBA）
- [x] 前端新增 Framebuffer 面板（地址/分辨率/像素格式可配置）
- [x] 支持 `gray8/rgb565/rgb888` 三种源格式渲染
- [x] 演示帧生成命令 `fb_demo <pong|checker|gradient>`（一键生成可视化画面）
- [x] Windows 一键演示脚本 `scripts/run_framebuffer_demo.ps1`
- [x] 接入 Linux 用户态程序输出到约定帧缓冲地址
- [x] 串联 SDL/小游戏演示脚本与一键验收

#### P6 里程碑验收（时间线）

#### P6 当前里程碑验收（2026-04-01）

- 新增能力：
  - WebSocket 命令：`framebuffer <addr> <width> <height> [format]`
  - Linux 预设命令：`fb linux` / `framebuffer linux`（默认 `0x80E00000`, `320x240`, `rgb565`）
  - 前端可视化：Framebuffer Tab，支持手动刷新与自动刷新
  - 内置 RV32I 帧缓冲写入程序（`visualize --linux-fb-demo --warmup <N>`）
  - 一键演示脚本内置 WebSocket 探针验收（确认 `fb linux` 返回非零像素）
- 验收结论：
  - 已完成“程序写帧缓冲 → 后端读取转换 → 前端渲染”的端到端闭环
  - 图案命令模式（`fb_demo`）保留用于小游戏画面演示

#### P6 当前里程碑验收（2026-04-02）

- 新增能力：
  - 新增 Phase6 编排脚本：`scripts/run_phase6_showcase_pipeline.ps1`。
  - 支持一键串联阶段：`xv6 shell smoke`、`Linux Phase3 acceptance`（可选启用）、`Phase4 host/guest acceptance`、`NPU/LPU 回归`、`frontend build`。
  - 提供阶段化开关与统一日志汇总（`target/phase6-demo-logs`），便于课程演示与回归复现。
- 验收测试：
  - 轻量烟测通过：`-SkipBuild -SkipXv6 -SkipPhase4 -SkipFrontendBuild`（协处理器回归阶段 PASS）。
- 当前边界：
  - 默认仓库不内置完整 Linux 工件，复现实测前需先准备工件；在具备工件并启用 `-EnableLinux` 的条件下，全链路 required stages 已验证 PASS（见上文“Phase 6 全链路终验”）。

#### Phase 6 全链路终验（2026-04-02）

- 新增能力：
  - 新增统一编排脚本：`scripts/run_phase6_showcase_pipeline.ps1`。
  - 一条命令串联并验收：
    - `build-release`
    - `xv6-shell-matrix`
    - `phase4-host-demo`
    - `phase4-guest-demo`
    - `linux-phase3-acceptance`
    - `coprocessor-fastpath-tests`
    - `coprocessor-dma-bridge-tests`
    - `frontend-build`
- 验收命令：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_phase6_showcase_pipeline.ps1 -EnableLinux`
- 验收结果：
  - 全部 required stage PASS。

---

## 测试计划

### DiffTest 差分测试 (核心)

与 QEMU 逐指令对比，确保行为一致：

- [ ] QEMU GDB Stub 集成
  - [ ] 启动 QEMU 并连接 GDB 端口
  - [ ] 同步加载测试程序到两个模拟器
- [ ] 状态对比
  - [ ] 通用寄存器 (x1-x31)
  - [ ] PC 寄存器
  - [ ] 关键 CSR (mstatus, mepc, mcause 等)
- [ ] 错误报告
  - [ ] 差异发生时的详细日志
  - [ ] 最近执行的指令历史
  - [ ] 寄存器/内存状态快照

### 单元测试

- 每条指令独立测试 (配合 DiffTest)
- 边界值测试 (溢出、符号扩展等)
- 异常情况测试 (非法指令、地址错误等)

### 集成测试

- riscv-tests 官方测试套件
- 流水线正确性测试
- 特权级切换测试

### 端到端测试

- 运行真实程序 (CoreMark, Rust hello world)
- 性能基准测试 (IPC 统计)

---

## Phase 7: 发布工程化（规划中）

> 当前阶段：Phase 1-6 已完成，进入 CI/CD 与持续回归优化阶段。

### 建议目标

1. **CI 集成**：将 Phase 6 编排脚本纳入 CI（夜间全量 + PR 轻量回归）
2. **回归收敛**：收敛回归耗时（分层并行、缓存工件、失败快速定位）
3. **可观测性**：增强演示可观测性（统一摘要报告 + 时间线导出）
4. **应用链路**：按需推进 guest 侧 NES/应用链路，强化"CPU 行为验证优先"证据

---

## 回滚策略

每个 Phase 都保持向后兼容：

| Phase   | Feature Flag   | 回滚方法                |
| ------- | -------------- | ----------------------- |
| Phase 2 | N/A (核心功能) | git revert              |
| Phase 3 | `pipeline`     | `--no-default-features` |
| Phase 4 | `privilege`    | 环境变量禁用            |
| Phase 5 | `debug`        | 不使用调试参数          |

**回滚验证**:

```bash
# 回滚后验证
git revert <commit-hash>
cargo test
cargo run -- --test-program tests/simple.bin
```

---

## 不变量检查

每个步骤后执行以下验证：

```bash
#!/bin/bash
# invariant-check.sh

echo "=== Invariant Check ==="

# 1. 编译通过
echo "1. Building..."
cargo build --release 2>&1 | grep -i "error" && exit 1

# 2. 测试通过
echo "2. Running tests..."
cargo test --quiet 2>&1 | grep -i "FAILED" && exit 1

# 3. 代码风格
echo "3. Checking formatting..."
cargo fmt -- --check 2>&1 | grep -i "warning" && exit 1

# 4. Clippy
echo "4. Running clippy..."
cargo clippy -- -D warnings 2>&1 | grep -i "error" && exit 1

# 5. 文档同步 (Phase 2+)
if [ -f docs/design/ROADMAP.md ]; then
    echo "5. Checking documentation..."
    grep -E "✅|❌" docs/design/ROADMAP.md || echo "No progress markers found"
fi

echo "=== All Invariants Passed ==="
```

---

## 风险评估

| 风险               | 影响 | 概率 | 缓解措施                                    |
| ------------------ | ---- | ---- | ------------------------------------------- |
| 流水线冒险处理复杂 | 高   | 中   | 参考 CVT/DHD 课件，逐步实现，每阶段独立测试 |
| CSR 规范理解偏差   | 高   | 低   | 对照官方规范，使用 riscv-tests 验证         |
| DiffTest 环境搭建  | 中   | 中   | 提前验证 QEMU 版本兼容性，提供备用方案      |
| GDB 协议实现       | 低   | 低   | 参考开源实现 (gdbstub)，使用标准库          |
| 并行开发合并冲突   | 中   | 中   | 使用 feature branches，及时 rebase          |

---

## 资源参考

### 规范文档

- [RISC-V 规范 (非特权级)](https://riscv.org/technical/specifications/)
- [RISC-V 规范 (特权级)](https://riscv.org/specifications/privileged-isa/)
- [RISC-V Reader](https://riscvbook.com/)

### 开源参考

- [riscv-tests](https://github.com/riscv/riscv-tests)
- [QEMU RISC-V](https://www.qemu.org/docs/master/system/target-riscv.html)

### 课程参考

- CVT/DHD 课件 (流水线设计)
- OS 课程参考 (特权级与异常)
