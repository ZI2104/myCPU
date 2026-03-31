# myCPU 开发路线图

## 开发阶段总览

| 阶段       | 内容     | 产出                     | 周期     |
| ---------- | -------- | ------------------------ | -------- |
| Phase 1    | 基础框架 | 可编译的项目骨架         | 1 周     |
| Phase 2    | 指令集   | 可运行简单程序           | 1.5 周   |
| Phase 3    | 流水线   | 5 级流水线 + 冒险处理    | 1.5 周   |
| Phase 4    | 特权级   | M/S/U 模式 + 异常中断    | 2 周     |
| Phase 5    | 外设     | UART + 调试器 + DiffTest | 1 周     |
| Phase 6 P0 | 性能监控 | HPM CSR + PerfCollector  | 1 周     |
| **总计**   | -        | -                        | **8 周** |

---

## Phase 1: 基础框架 ✅ 完成

### 目标
搭建项目骨架，实现内存和寄存器基础模块。

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

#### R-type (10 条)
- [x] ADD  - 加法
- [x] SUB  - 减法
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
- [x] ECALL - 环境调用
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
- ✅ 数据前递正确处理 RAW 冒险
- ✅ Load-Use 冒险正确暂停
- ✅ 分支预测错误正确冲刷流水线

---

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
- [ ] MUL, MULH, MULHSU, MULHU
- [ ] DIV, DIVU, REM, REMU

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

### P5: NPU/LPU 协处理器 (MMIO 路径)（进行中）

- [x] NPU MMIO 外设骨架（`0x2000_0000`）
- [x] LPU MMIO 外设骨架（`0x2000_1000`）
- [x] 启动命令默认挂载到系统总线（CLI 运行/调试/可视化）
- [x] 中断查询与确认接口（`has_interrupt` / `acknowledge_interrupt`）
- [x] 单元测试覆盖基础算子和中断行为
- [ ] 可视化前端寄存器面板与任务时间线
- [ ] DMA/描述符队列（大任务模式）
- [ ] 自定义指令加速路径（后续阶段）

### P6: Linux + SDL/Framebuffer 演示链路（已完成）

- [x] 可视化后端支持 `framebuffer/fb` 命令（读取内存并转换 RGBA）
- [x] 前端新增 Framebuffer 面板（地址/分辨率/像素格式可配置）
- [x] 支持 `gray8/rgb565/rgb888` 三种源格式渲染
- [x] 演示帧生成命令 `fb_demo <pong|checker|gradient>`（一键生成可视化画面）
- [x] Windows 一键演示脚本 `scripts/run_framebuffer_demo.ps1`
- [x] 接入 Linux 用户态程序输出到约定帧缓冲地址
- [x] 串联 SDL/小游戏演示脚本与一键验收

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

#### P5 当前里程碑验收（2026-03-31）

- 新增文件：
  - `src/peripheral/npu.rs`
  - `src/peripheral/lpu.rs`
- 验收测试：
  - `cargo test --lib` 通过（208 passed, 0 failed）
  - `cargo build` 通过

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
