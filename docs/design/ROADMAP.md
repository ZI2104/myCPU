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
Phase 5.2: GPU/TPU 模拟加速器 ✅
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
> - ✅ 已完成
> - 🟡 部分完成
> - ⏳ 未完成

| 阶段      | 目标（用户要求）                                           | 当前状态   | 验收标准                                              | 当前证据                                                                                                                                                                                                                                                      |
| --------- | ---------------------------------------------------------- | ---------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 0   | 冻结可用基线 + OS bring-up 专项测试入口                    | ✅ 已完成   | xv6 到 shell prompt 作为首个里程碑                    | 基线与 bring-up smoke 测试入口已落地，且已在长窗口运行中稳定看到 xv6 shell 提示符 `$`                                                                                                                                                                         |
| Phase 1   | xv6 启动关键能力补齐（RV32M、Trap闭环、Sv32、MMU统一路径） | ✅ 已完成   | xv6 内核入口与串口输出稳定                            | RV32M/Trap/Sv32/MMU 路径均已实现并有测试，详见本文件 P1/P3 验收段                                                                                                                                                                                             |
| Phase 2   | xv6 可交互运行（CLINT/PLIC 稳定 + 最小块设备）             | ✅ 已完成   | xv6 文件系统镜像进入 shell 可交互                     | 已完成 PLIC/SIP 兼容修复与 VirtIO 描述符传输修复（含 1KiB 场景），并新增 `scripts/run_xv6_shell_smoke.ps1` 自动验收流程；200M 窗口稳定通过 `echo/ls/cat/grep/wc` 命令矩阵                                                                                     |
| Phase 3   | 精简 Linux 启动链路（SBI + FDT + 启动参数）                | 🟡 部分完成 | Buildroot Linux 进入 init/userland                    | 模拟器侧链路能力已齐全（SBI/FDT/bootargs/payload）；默认仓库不内置完整 Linux 工件，需通过脚本准备 OpenSBI/Buildroot 工件后完成终验                                                                                                                            |
| Phase 4   | Linux 用户态 SDL/FB 游戏演示                               | ✅ 已完成   | 简化 2D 游戏跑通 + 输入设备 + Overlay                 | `fb_game` 游戏流程控制（init/step/run/reset）+ 输入面板 + Framebuffer Overlay 已落地；host/guest 自动验收脚本通过，库测 274/274 与前端构建通过                                                                                                                |
| Phase 5   | NPU/LPU 模拟（MMIO 优先）                                  | ✅ 已完成   | CTRL/STATUS/DESC_ADDR/IRQ + 描述符/DMA + IRQ 完整闭环 | NPU/LPU 已补齐 DESC_ADDR/描述符 DMA/Bus 桥接、可视化状态面板、CUSTOM-0 自定义指令 fast-path 与任务时间线；LPU 已迁移为 Language Processing Unit（仅支持语言 opcode，MVP 含 ByteTokenize + EmbeddingBag + GreedyDecode + TopKSampleDecode + TopPSampleDecode） |
| Phase 5.2 | GPU/TPU 模拟加速器                                         | ✅ 已完成   | Conv2d/Pool2d/INT8量化 + 可视化 + 集成测试            | GPU (15 内核) + TPU (INT8 量化) 已实现，304 库测试 + 3 集成测试通过，前端 GPU/TPU 面板已落地，API 文档已完成                                                                                                                                                  |
| Phase 6   | 亮点封装发布（脚本化演示+回归矩阵）                        | ✅ 已完成   | xv6→Linux→游戏→NPU 对比演示可复现                     | 在具备 Linux 工件并启用 `-EnableLinux` 的条件下，`run_phase6_showcase_pipeline.ps1` 全链路通过：xv6、Linux、Phase4 host/guest、NPU/LPU 回归与前端构建均 PASS                                                                                                  |

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

- [X] 项目初始化 (Cargo 配置)
- [X] 基础 trait 定义
  - [X] `Memory` trait (read/write)
  - [X] `Peripheral` trait
- [X] 内存模块
  - [X] RAM 实现
  - [X] ROM 实现
  - [X] 总线 (Bus) 实现
- [X] CPU 寄存器组
  - [X] 通用寄存器 x0-x31
  - [X] PC 寄存器
- [X] 主循环框架
  - [X] 单周期执行循环
  - [X] 时钟计数
- [X] 错误处理
  - [X] SimError 类型定义
  - [X] Result 类型别名

### 产出

- ✅ 可编译运行的空模拟器
- ✅ 基础内存读写测试通过 (35 个测试全部通过)

---

## Phase 2: 指令集实现 ✅ 完成

### 目标

实现 RV32I 基础指令集 (40 条)，可运行简单程序。

### RV32I 指令清单

- [X] AND  - 与
- [X] OR   - 或
- [X] XOR  - 异或
- [X] SLL  - 逻辑左移
- [X] SRL  - 逻辑右移
- [X] SRA  - 算术右移
- [X] SLT  - 有符号小于比较
- [X] SLTU - 无符号小于比较

#### I-type (14 条)

- [X] ADDI  - 加立即数
- [X] ANDI  - 与立即数
- [X] ORI   - 或立即数
- [X] XORI  - 异或立即数
- [X] SLTI  - 有符号小于比较立即数
- [X] SLTIU - 无符号小于比较立即数
- [X] SLLI  - 逻辑左移立即数
- [X] SRLI  - 逻辑右移立即数
- [X] SRAI  - 算术右移立即数
- [X] LB    - 加载字节
- [X] LH    - 加载半字
- [X] LW    - 加载字
- [X] LBU   - 加载无符号字节
- [X] LHU   - 加载无符号半字

#### S-type (3 条)

- [X] SB - 存储字节
- [X] SH - 存储半字
- [X] SW - 存储字

#### B-type (6 条)

- [X] BEQ  - 相等跳转
- [X] BNE  - 不等跳转
- [X] BLT  - 有符号小于跳转
- [X] BGE  - 有符号大于等于跳转
- [X] BLTU - 无符号小于跳转
- [X] BGEU - 无符号大于等于跳转

#### U-type (2 条)

- [X] LUI   - 加载高位立即数
- [X] AUIPC - PC 加高位立即数

#### J-type (2 条)

- [X] JAL  - 跳转并链接
- [X] JALR - 跳转并链接寄存器

#### System (3 条)

- [X] EBREAK - 断点
- [X] FENCE - 内存屏障

### 产出

- ✅ 所有 RV32I 指令测试通过 (67 个单元测试)
- ✅ 可运行简单算术程序

---

## Phase 3: 流水线实现 ✅ 完成

### 目标

实现 5 级流水线，处理数据冒险和控制冒险。

### 任务清单

- [X] 流水线寄存器
  - [X] IF/ID 寄存器
  - [X] ID/EX 寄存器
  - [X] EX/MEM 寄存器
  - [X] MEM/WB 寄存器
- [X] 各阶段实现
  - [X] IF (Instruction Fetch)
  - [X] ID (Instruction Decode)
  - [X] EX (Execute)
  - [X] MEM (Memory Access)
  - [X] WB (Write Back)
- [X] 冒险处理
  - [X] 数据冒险检测
  - [X] 前递逻辑 (EX/MEM, MEM/WB)
  - [X] Load-Use 暂停
  - [X] 控制冒险 - 静态预测 (Predict Not Taken)
  - [X] 分支冲刷

### 产出

- ✅ 流水线测试通过 (115 个测试全部通过)
- ✅ Load-Use 冒险正确暂停

## Phase 4: 特权级与异常 ✅ 完成

### 目标

实现 M/S/U 三级特权模式，支持异常和中断处理。

### 任务清单

- [X] CSR 寄存器
  - [X] M-mode: mstatus, mtvec, mepc, mcause, mie, mip, mscratch, misa, mtval, mideleg, medeleg
  - [X] S-mode: sstatus, stvec, sepc, scause, sie, sip, sscratch, stval
  - [X] U-mode: ustatus, utvec, uepc, ucause
  - [X] CSR 访问指令 (csrrw, csrrs, csrrc, csrrwi, csrrsi, csrrci)
- [X] 特权级切换
  - [X] ecall 指令 (单周期 CPU)
  - [X] mret 指令 (单周期 CPU + 流水线)
  - [X] sret/uret 指令 (框架已实现)
  - [X] 特权级检查 (CSR 访问权限)
  - [X] 流水线 CPU 中的 CSR 指令支持
- [X] 异常处理
  - [X] 异常入口 (xtvec) - 单周期 CPU + 流水线
  - [X] 上下文保存/恢复 - 单周期 CPU + 流水线
  - [X] 异常返回 - 单周期 CPU + 流水线 (mret)
- [X] 中断系统 - CLINT
  - [X] CLINT 实现 (mtime, mtimecmp, msip)
  - [X] InterruptSource trait
  - [X] 中断同步到 MIP (单周期 CPU + 流水线)
  - [X] 中断优先级处理
- [X] 中断系统 - PLIC
  - [X] PLIC 实现 (外部中断控制器)
  - [X] PLIC 与 Bus 集成
  - [X] PLIC 与 CPU 集成 (MEIP/SEIP)
  - [X] 中断委托 (M → S) via mideleg/medeleg

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

- [X] UART (NS16550A 兼容)
  - [X] 发送/接收寄存器 (THR/RBR)
  - [X] 状态寄存器 (LSR)
  - [X] 中断支持 (IER/IIR)
  - [X] FIFO Control Register (FCR)
  - [X] Line Control Register (LCR)
  - [X] Modem Control Register (MCR)
  - [X] Scratch Register (SCR)
  - [X] Divisor Latch (DLL/DLM)
- [X] Timer (已在 Phase 4 实现于 CLINT)
  - [X] mtime 寄存器
  - [X] mtimecmp 寄存器
  - [X] 时钟中断
- [X] ELF 加载器
  - [X] 解析 ELF 头 (使用 goblin crate)
  - [X] 加载程序段
  - [X] 设置入口点
  - [X] BSS 段零填充
- [X] GDB Remote Protocol (核心)
  - [X] TCP Server 基础框架
  - [X] 基础命令: ?, g, G, m, M, c, s
  - [X] 断点支持: Z0, z0
  - [X] 查询命令: qSupported, qAttached
  - [X] VSCode 集成配置 (.vscode/launch.json)
- [X] DiffTest 框架
  - [X] QEMU GDB Stub 集成
  - [X] 状态对比逻辑
  - [X] 错误报告与日志

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

- [X] RISC-V HPM CSR 寄存器

  - [X] mcycle/mcycleh (周期计数器)
  - [X] minstret/minstreth (指令计数器)
  - [X] mhpmcounter3-31 (可编程计数器)
  - [X] mhpmevent3-31 (事件选择器)
  - [X] mcountinhibit (计数器禁止)
- [X] 性能事件收集器 (PerfCollector)

  - [X] 周期计数 (Cycles)
  - [X] 指令退休计数 (InstructionsRetired)
  - [X] Load-Use 暂停计数
  - [X] 控制冒险计数
  - [X] 分支统计 (Taken/NotTaken)
  - [X] 内存访问统计
- [X] 流水线性能集成

  - [X] 单周期 CPU 集成
  - [X] 流水线 CPU 集成
  - [X] CSR HPM 计数器更新
- [X] 性能报告生成

  - [X] IPC/CPI 计算
  - [X] 暂停率统计
  - [X] 分支预测准确率
  - [X] 格式化输出
- [X] CLI 集成

  - [X] --perf-report 选项

### 产出

- ✅ 性能计数器 CSR 实现 (符合 RISC-V HPM 规范)
- ✅ PerfCollector 事件收集器
- ✅ PerfReport 格式化报告
- ✅ CLI --perf-report 选项
- ✅ 182 个测试全部通过

---

### P1: M 扩展 (乘除法) (可选)

- [X] MUL, MULH, MULHSU, MULHU
- [X] DIV, DIVU, REM, REMU

### P2: C 扩展 (压缩指令) (可选)

- [ ] 16 位压缩指令解码
- [ ] 常用指令的压缩形式

### P3: Sv32 分页 (可选)

- [X] 页表结构（SATP CSR + Sv32 两级页表遍历骨架）
- [X] TLB 缓存
- [X] 地址翻译入口（单周期 CPU：取指/Load/Store）
- [X] 页错误异常（Instruction/Load/Store Page Fault）

#### P3 当前里程碑验收（2026-03-31）

- [X] 核心产出：Sv32 基线链路打通（`satp` + 页表遍历 + 取指/访存翻译入口）。
- [X] 核心产出：页故障语义收敛到 trap 路径（`mcause/mtval/mepc` 写入正确）。
- [X] 工程落位：`src/cpu/mmu.rs`、CSR 路径、单周期 CPU 地址翻译接线。
- [X] 验收结论：`cargo test --lib` 通过（202 passed, 0 failed），页故障验收用例通过。
- [ ] 当前边界：TLB 尚未实现。
- [ ] 当前边界：A/D 位自动更新与 SUM/MXR 权限细则尚未实现。

#### P3 同步校验（2026-04-09）

- [X] 核心产出：TLB 已接入（ASID 隔离 + 按地址/ASID/全量刷新）。
- [X] 核心产出：MMU/Pipeline 翻译路径接入 TLB 命中-未命中采样。
- [X] 核心产出：性能与可视化链路暴露 `cache_hits/cache_misses/tlb_hits/tlb_misses`。
- [X] 工程落位：`src/cpu/tlb.rs`、MMU/Pipeline、PerfCollector/PerfReport、可视化快照与前端面板。
- [X] 验收结论：`cargo test --lib` 通过（386 passed, 0 failed），TLB 单测通过。
- [ ] 当前边界：A/D 位自动更新与 SUM/MXR 细粒度权限仍在后续优化清单。

### P4: 多核支持 (可选)

- [ ] 多个 Hart (硬件线程)
- [ ] 核间中断 (IPI)
- [ ] 共享内存

### P5: 协处理器能力（NPU/LPU + GPU/TPU + V2 拓扑）（已完成）

#### P5 合并后能力总览（截至 2026-04-16）

- [X] P5.0（NPU/LPU）主链路闭环：MMIO 外设、描述符批处理、Bus DMA 桥接、IRQ、可视化状态面板、`CUSTOM-0` 快路径与任务时间线。
- [X] P5.2（GPU/TPU）模拟加速器完成：Kernel 抽象 + MMIO + 命令队列 + DMA 桥接 + 前端状态面板 + API 文档。
- [X] P5.3（拓扑优化）收敛完成：协处理器地址空间切换为纯 V2 拓扑。
- [X] 当前基址口径（纯 V2）：
  - [X] NPU `0x2001_0000`
  - [X] LPU `0x2001_1000`
  - [X] GPU `0x2001_2000`
  - [X] TPU `0x2001_3000`
- [X] 最新同步校验：`cargo test --lib` 持续通过（413 passed, 0 failed）；关键拓扑回归用例已覆盖 pure V2 窗口与 Doorbell 提交/错误路径。

#### P5 里程碑验收（时间线）

> 说明：本时间线保留阶段快照；早期里程碑中的“当前边界”若已在后续版本关闭，会显式标记为“已关闭”。

#### P5 当前里程碑验收（2026-03-31）

- [X] 核心产出：NPU/LPU 外设骨架落地，P5 协处理器主线启动。
- [X] 工程落位：`src/peripheral/npu.rs`、`src/peripheral/lpu.rs`。
- [X] 验收结论：`cargo test --lib`（208 passed, 0 failed）与 `cargo build` 均通过。

#### P5 当前里程碑验收（2026-04-02）

- [X] 核心产出：NPU/LPU 描述符批处理链路落地（寄存器、批执行、任务统计）。
- [X] 核心产出：总线接入 pending-notify DMA 风格桥接，PLIC 补齐 NPU/LPU IRQ 源映射。
- [X] 工程落位：`src/peripheral/{npu,lpu}.rs`、`src/memory/bus.rs`、PLIC 路径。
- [X] 验收结论：`cargo test --lib` 通过（267 passed, 0 failed）。

#### P5 当前里程碑验收（2026-04-02-R2）

- [X] 核心产出：后端支持 `npu state` / `lpu state`，前端新增 `CoprocessorPanel` 状态面板。
- [X] 核心产出：总线补齐协处理器快照查询接口，打通“后端状态 → 前端展示”。
- [X] 工程落位：`src/visualize/server`、`src/memory/bus.rs`、`frontend/src/components/CoprocessorPanel.tsx`。
- [X] 验收结论：`cargo test --lib` 通过（268 passed, 0 failed），`frontend` 构建通过。

#### P5 当前里程碑验收（2026-04-02-R3）

- [X] 核心产出：`CUSTOM-0 (0x0B)` 快路径落地，CPU 可直接触发 NPU/LPU 同步运算并回写 `rd`。
- [X] 核心产出：R-type 编码规范化（`funct3` 路由引擎，`funct7[4:0]` 作为协处理器 opcode）。
- [X] 工程落位：`src/instruction/execute.rs`（`execute_custom0()`）及 CPU 执行路径接线。
- [X] 验收结论：`cargo test --lib` 通过（271 passed, 0 failed）。

#### P5 当前里程碑验收（2026-04-02-R4）

- [X] 核心产出：协处理器任务时间线面板落地（`notify/done/error/pending` 事件可视）。
- [X] 核心产出：新增自动轮询 + 增量去重 + 最近窗口保留，便于演示与回归观察。
- [X] 工程落位：`frontend/src/components/CoprocessorPanel.tsx`、`frontend/src/App.tsx`、`frontend/src/App.css`。
- [X] 验收结论：`cargo test --lib` 与 `frontend` 构建均通过。
- [X] 阶段结论：P5 主链路能力闭环完成，后续聚焦性能与可观测性增强。

#### P5 分项状态（已完成 / 规划中）

##### P5.2：GPU/TPU 模拟加速器（已完成）

> 时间范围：2026-04-08 ~ 2026-04-09

- [X] 能力收敛：GPU/TPU MMIO、命令队列、DMA 桥接、状态面板与 API 文档已闭环。
- [X] 内核覆盖：GPU（线代/激活/卷积/池化），TPU（INT8 量化矩阵乘）。
- [X] 证据口径：详见下方 `P5.2 里程碑验收（2026-04-09）`。

##### P5.2 里程碑验收（2026-04-09）

- [X] 核心产出：GPU/TPU 模拟加速器主链路完成（trait 抽象 + MMIO 外设 + 命令队列 + DMA 桥接）。
- [X] 核心产出：推理内核族完成（GPU：线代/激活/卷积/池化；TPU：INT8 量化矩阵乘）。
- [X] 工程落位：`src/traits/accelerator.rs`、`src/peripheral/{gpu,tpu}.rs`、`tests/gpu_tpu_integration.rs`、`docs/guides/GPU_TPU_API.md`。
- [X] 验收结论：`cargo test --lib`（304 passed）/ `cargo test --test gpu_tpu_integration`（3 passed）/ `frontend` 构建全部通过。
- [X] 阶段结论：P5.2 进入稳定可复用状态，可支撑课程演示与后续异构协同优化。

##### P5.3：异构协处理器拓扑优化（V2，已完成）

> 时间范围：2026-04-16

- [X] LPU 清理 legacy 逻辑 opcode（`0~5`）执行路径，仅保留语言 opcode（`0x10~0x14`）
- [X] 形成控制面/数据面/事件面三平面架构方案
- [X] 形成 V2 地址映射草案（`ACC_CTRL_ROOT + ACC_DOORBELL + NPU/LPU/GPU/TPU CTRL`）
- [X] 落地双地址窗口（V1 alias + V2 primary，`ACC_CTRL_ROOT.MODE` 控制 V2 overlay）
- [X] 完成 Doorbell 汇聚中断与 descriptor ring 统一 ABI（统一提交寄存器 + root 汇总计数）
- [X] 收敛并移除 V1 alias，切换到 V2 拓扑

##### P5.3 里程碑验收（2026-04-16-R2）

- [X] 核心产出：统一控制面 `ACC_CTRL_ROOT` + 统一提交面 `ACC_DOORBELL` 落地，形成 V2 拓扑迁移路径。
- [X] 核心产出：Doorbell 统一 ABI（engine/desc_addr/desc_len/notify）打通四类协处理器。
- [X] 工程落位：`src/memory/bus.rs` 统一窗口、提交桥接与汇总计数逻辑。
- [X] 验收结论：阶段验收记录 `cargo test --lib` 全通过（413 passed, 0 failed）。
- [X] 阶段结论：R2 进入“双窗口迁移期”，为 R3 纯 V2 收敛做铺垫。
- [X] 后续收敛状态：该迁移期边界已在 R3 关闭，当前为纯 V2 稳定态。

##### P5.3 里程碑验收（2026-04-16-R3）

- [X] 核心产出：协处理器地址空间完成纯 V2 收敛（NPU/LPU/GPU/TPU 基址统一为 `0x2001_x000`）。
- [X] 核心产出：V1 alias/overlay 迁移逻辑移除，`ACC_CTRL_ROOT.MODE` 固定 V2 启用态。
- [X] 工程落位：`src/memory/bus.rs` 与各协处理器基址常量（`src/peripheral/{npu,lpu,gpu,tpu}.rs`）。
- [X] 验收结论：`cargo test --lib` 与 `cargo test` 均通过；纯 V2 关键回归测试通过。
- [X] 阶段结论：P5.3 收敛完成，拓扑进入“纯 V2 稳定态”。

#### 规划中项

##### P5.1：Hybrid Offload（规划中）

> 来源：`docs/HYBRID_OFFLOAD.md`（草案，2026-04-03 并入路线图）

- [ ] 建立 CPU/NPU 混合调度决策（算子类型、数据规模、对齐、stride、时延预算）
- [ ] 在保持 16B descriptor ABI 兼容前提下扩展标志位（`ASYNC_FLAG`、`WIDE_FLAG`、`STRIDE_FLAG`）
- [ ] 在现有同步 MMIO 路径之外新增异步队列路径（后台 worker + 完成中断）
- [ ] 完善错误上报与回退语义（`tasks_error`/`REG_STATUS`/IRQ + CPU fallback）
- [ ] 建立批量阈值基准（建议从 `>128` 元素起测）并形成调优策略

##### P5.1 最小可行实施分期

- **Phase A（文档/ABI）**：先引入 `ASYNC_FLAG` 定义，不改变默认同步执行语义。
- **Phase B（运行时）**：当设置 `ASYNC_FLAG` 时，descriptor 入后台队列并立即返回；worker 完成后更新 `REG_TASKS_DONE/REG_STATUS` 并置 IRQ pending。
- **Phase C（性能）**：按批量大小做基准，确定 offload 阈值与小批次合并策略。

##### P5.1 兼容性约束

- 默认路径保持现有同步行为，不破坏已落地测试与演示链路。
- 新能力通过标志位渐进启用，支持快速回滚到同步路径。

### P6: Linux + SDL/Framebuffer 演示链路（已完成）

- [X] 可视化后端支持 `framebuffer/fb` 命令（读取内存并转换 RGBA）
- [X] 前端新增 Framebuffer 面板（地址/分辨率/像素格式可配置）
- [X] 支持 `gray8/rgb565/rgb888` 三种源格式渲染
- [X] 演示帧生成命令 `fb_demo <pong|checker|gradient>`（一键生成可视化画面）
- [X] Windows 一键演示脚本 `scripts/run_framebuffer_demo.ps1`
- [X] 接入 Linux 用户态程序输出到约定帧缓冲地址
- [X] 串联 SDL/小游戏演示脚本与一键验收

#### P6 里程碑验收（时间线）

#### P6 当前里程碑验收（2026-04-01）

- [X] 核心产出：帧缓冲可视化链路完成（命令入口 + 后端转换 + 前端渲染）。
- [X] 核心产出：Linux 预设命令与 `fb_demo` 演示模式并存，兼顾验收与展示。
- [X] 工程落位：`visualize` WebSocket 命令、Framebuffer 面板、演示脚本探针。
- [X] 验收结论：已验证“程序写帧缓冲 → 后端读取转换 → 前端渲染”端到端闭环。

#### P6 当前里程碑验收（2026-04-02）

- [X] 核心产出：Phase6 编排脚本落地，实现多阶段一键串联与统一日志汇总。
- [X] 核心产出：支持按开关裁剪阶段，形成“轻量烟测 / 全链路演示”两种运行档位。
- [X] 工程落位：`scripts/run_phase6_showcase_pipeline.ps1` 与 `target/phase6-demo-logs`。
- [X] 验收结论：轻量烟测路径通过，协处理器回归阶段 PASS。
- [ ] 当前边界：默认仓库不内置完整 Linux 工件，全链路演示依赖预置工件。

#### Phase 6 全链路终验（2026-04-02）

- [X] 核心产出：构建、xv6、Phase4、Linux Phase3、协处理器回归与前端构建已纳入统一编排验收。
- [X] 核心产出：形成可复现的一键演示/回归入口，覆盖课程展示主链路。
- [X] 工程落位：`scripts/run_phase6_showcase_pipeline.ps1` 串联 `run_linux_phase3_acceptance.ps1`、`run_phase4_input_framebuffer_acceptance.ps1` 等脚本。
- [X] 验收结论：`-EnableLinux` 全链路 required stages PASS。

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

#### riscv-tests 持续回归（2026-04-09）

- [X] 工作流：`.github/workflows/run-riscv-tests.yml`
- [X] 脚本：`scripts/run_riscv_tests.sh`
- [X] 产物：`artifacts/riscv-tests/*.perf.json`（`if: always()` 上传）
- [X] 限流：`workflow_dispatch` 支持 `max_tests`

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
