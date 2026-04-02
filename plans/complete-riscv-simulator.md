# myCPU: 完整 RISC-V 模拟器施工蓝图 v3.0

> **目标**: 构建一个可复现、可演示、可回归的 RISC-V RV32 模拟器平台（CPU + OS bring-up + 协处理器 + 可视化）
>
> **当前状态**: Phase 1-6 已完成（截至 2026-04-02）
>
> **当前阶段**: 进入发布工程化与持续回归优化（Phase 7，规划中）

---

## 项目背景

### 当前状态快照（2026-04-02）

| 阶段    | 状态 | 关键结果                                                |
| ------- | ---- | ------------------------------------------------------- |
| Phase 1 | ✅    | 基础框架（类型/内存/寄存器/主循环）稳定                 |
| Phase 2 | ✅    | RV32I 指令集与执行链路完成                              |
| Phase 3 | ✅    | 流水线与 OS bring-up 关键能力完成                       |
| Phase 4 | ✅    | 特权/异常中断 + 输入/帧缓冲演示闭环完成                 |
| Phase 5 | ✅    | NPU/LPU（DMA+IRQ+custom fast-path+面板+时间线）完成     |
| Phase 6 | ✅    | 一键编排验收（xv6→Linux→Phase4→NPU/LPU→frontend）全通过 |

### 已完成里程碑证据

- 路线图状态：`docs/ROADMAP.md`
- 执行台账：`docs/MILESTONE_EXECUTION.md`
- 全链路终验命令：`powershell -ExecutionPolicy Bypass -File .\\scripts\\run_phase6_showcase_pipeline.ps1 -EnableLinux`
- 最近结论：required stages 全部 PASS。

### 下一阶段（Phase 7）建议目标

1. 将 Phase 6 编排脚本纳入 CI（夜间全量 + PR 轻量回归）。
2. 收敛回归耗时（分层并行、缓存工件、失败快速定位）。
3. 增强演示可观测性（统一摘要报告 + 时间线导出）。
4. 按需推进 guest 侧 NES/应用链路，强化“CPU 行为验证优先”证据。

---

### 历史背景（归档）

以下是 v2 蓝图撰写时的起点状态，保留作为历史记录：

#### 已完成 (Phase 1 ✅)

- ✅ 基础类型系统 (Addr, Word, Byte, RegIdx, PrivilegeLevel)
- ✅ 错误处理框架 (SimError)
- ✅ Memory trait + RAM/ROM/Bus 实现
- ✅ 32 个通用寄存器 (x0-x31) + PC
- ✅ CPU 状态快照 (CpuState for DiffTest)
- ✅ 主循环框架 (step/run)
- ✅ 35 个单元测试全部通过

#### 待实现 (Phase 2-5)

| Phase   | 内容                       | 关键产出       |
| ------- | -------------------------- | -------------- |
| Phase 2 | RV32I 指令集 (40 条)       | 可运行简单程序 |
| Phase 3 | 5 级流水线                 | 性能提升       |
| Phase 4 | M/S/U 特权级 + 异常中断    | 完整特权支持   |
| Phase 5 | 外设 + GDB 调试 + DiffTest | 可调试、可验证 |

---

## 依赖关系图

```mermaid
graph TB
    P1[Phase 1: 基础框架 ✅] --> P2[Phase 2: 指令集 ✅]
    P2 --> P3[Phase 3: 流水线/OS Bring-up ✅]
    P3 --> P4[Phase 4: 特权+异常+输入/渲染 ✅]
    P4 --> P5[Phase 5: NPU/LPU ✅]
    P5 --> P6[Phase 6: 一键编排验收 ✅]
    P6 --> P7[Phase 7: CI/发布工程化（规划）]

    P2 --> P2A[测试基础设施]
    P2A --> P2B[指令译码器]
    P2B --> P2C[R-type 指令]
    P2B --> P2D[I-type 算术]
    P2B --> P2E[Load 指令]
    P2B --> P2F[Store 指令]
    P2B --> P2G[分支指令]
    P2B --> P2H[跳转指令]
    P2B --> P2I[System/U-type]

    P2C & P2D & P2E & P2F & P2G & P2H & P2I --> P2J[指令集成]

    style P2C & P2D & P2E & P2F & P2G & P2H & P2I fill:#f9f,stroke:#333,stroke-width:1px
    style P2A fill:#ff9,stroke:#333,stroke-width:2px
```

**注意**: 图中 P2A-P2J 及后续 Phase 2-5 细化步骤为历史施工记录，当前已完成并归档。

---

## 历史施工细化（归档）

> 本节保留 v2 细化步骤用于追溯，不再作为当前执行清单。当前执行以 `docs/ROADMAP.md` 与 `docs/MILESTONE_EXECUTION.md` 为准。

## Phase 2: RV32I 指令集实现

### 目标

实现全部 40 条 RV32I 基础指令，使模拟器能够运行简单的汇编程序。

**退出标准** (可测量):

- 所有 40 条指令各有至少 2 个测试用例 = 80+ 测试通过
- 能正确计算 fib(10) = 55 (使用测试程序)
- `cargo test --test instruction_tests` 全部通过

---

### 步骤 2.0: 测试基础设施

**文件**: `tests/instruction_tests.rs`, `tests/README.md`

**任务**:

- [ ] 创建 tests 目录结构
- [ ] 创建指令测试框架 (helper functions)
- [ ] 创建测试程序构建脚本
- [ ] 添加测试文档

**验证**:

```bash
# 验证测试目录存在
test -d tests && echo "OK" || echo "FAIL"
# 验证测试文件存在
test -f tests/instruction_tests.rs && echo "OK" || echo "FAIL"
cargo test --test instruction_tests -- --nocapture
```

**退出标准**: 测试框架可运行，打印 "PASS: 0/0 tests"

---

### 步骤 2.1: 指令译码器

**文件**: `src/instruction/decoder.rs`, `src/instruction/format.rs`

**任务**:

- [ ] 创建 instruction 模块 (`src/instruction/mod.rs`)
- [ ] 定义指令格式枚举 (R/I/S/B/U/J)
- [ ] 实现指令字段解析 (opcode, rd, rs1, rs2, funct3, funct7, imm)
- [ ] 实现指令译码器 `decode(instruction: u32) -> Result<Instruction>`
- [ ] 添加译码器单元测试 (覆盖所有格式)

**验证**:

```bash
cargo test instruction::decoder::tests
cargo test instruction::format::tests
```

**退出标准**:

- 译码器测试 100% 通过
- 能正确解析所有格式的指令字段
- 测试数量 >= 20 (每格式至少 3 个测试)

---

### 步骤 2.2: R-type 算术逻辑指令

**文件**: `src/instruction/r_type.rs`

**指令** (10 条): ADD, SUB, AND, OR, XOR, SLL, SRL, SRA, SLT, SLTU

**任务**:

- [ ] 定义 R-type 指令枚举
- [ ] 实现 ALU 操作函数
- [ ] 实现每条指令的执行逻辑
- [ ] 添加单元测试 (每条指令至少 2 个测试)

**验证**:

```bash
cargo test instruction::r_type::tests -- --exact
```

**退出标准**:

- 10 条指令各 2+ 测试 = 20+ 测试通过
- ALU 操作边界条件测试通过

---

### 步骤 2.3: I-type 立即数算术指令

**文件**: `src/instruction/i_type_arith.rs`

**指令** (9 条): ADDI, ANDI, ORI, XORI, SLTI, SLTIU, SLLI, SRLI, SRAI

**任务**:

- [ ] 定义 I-type 算术指令枚举
- [ ] 实现立即数符号扩展
- [ ] 实现每条指令的执行逻辑
- [ ] 添加单元测试 (每条指令至少 2 个测试，包含边界值)

**验证**:

```bash
cargo test instruction::i_type_arith::tests
```

**退出标准**:

- 9 条指令各 2+ 测试 = 18+ 测试通过
- 立即数边界值测试 (0x7FF, 0x800, 0xFFF) 通过

---

### 步骤 2.4: Load 指令

**文件**: `src/instruction/load.rs`

**指令** (5 条): LB, LH, LW, LBU, LHU

**任务**:

- [ ] 定义 Load 指令枚举
- [ ] 实现字节/半字/字加载
- [ ] 实现有符号/无符号扩展
- [ ] 添加单元测试 (包含对齐/不对齐地址)

**验证**:

```bash
cargo test instruction::load::tests
```

**退出标准**:

- 5 条指令各 2+ 测试 = 10+ 测试通过
- 地址对齐测试通过

---

### 步骤 2.5: Store 指令

**文件**: `src/instruction/s_type.rs`

**指令** (3 条): SB, SH, SW

**任务**:

- [ ] 定义 S-type 指令枚举
- [ ] 实现 S-type 格式解析
- [ ] 实现存立即数计算
- [ ] 实现存储逻辑
- [ ] 添加单元测试

**验证**:

```bash
cargo test instruction::s_type::tests
```

**退出标准**:

- 3 条指令各 2+ 测试 = 6+ 测试通过
- 存储-读取回环测试通过

---

### 步骤 2.6: 分支指令

**文件**: `src/instruction/b_type.rs`

**指令** (6 条): BEQ, BNE, BLT, BGE, BLTU, BGEU

**任务**:

- [ ] 定义 B-type 指令枚举
- [ ] 实现 B-type 格式解析
- [ ] 实现分支目标地址计算
- [ ] 实现条件判断逻辑
- [ ] 添加单元测试

**验证**:

```bash
cargo test instruction::b_type::tests
```

**退出标准**:

- 6 条指令各 2+ 测试 = 12+ 测试通过
- 有符号/无符号比较边界测试通过

---

### 步骤 2.7: 跳转指令

**文件**: `src/instruction/j_type.rs`

**指令** (2 条): JAL, JALR

**任务**:

- [ ] 定义跳转指令枚举
- [ ] 实现 J/JALR 格式解析
- [ ] 实现跳转目标地址计算
- [ ] 实现返回地址保存
- [ ] 添加单元测试

**验证**:

```bash
cargo test instruction::j_type::tests
```

**退出标准**:

- 2 条指令各 3+ 测试 = 6+ 测试通过
- 跳转+返回链测试通过

---

### 步骤 2.8: U-type 和 System 指令

**文件**: `src/instruction/u_type.rs`, `src/instruction/system.rs`

**指令** (5 条): LUI, AUIPC, ECALL, EBREAK, FENCE

**任务**:

- [ ] 定义 U-type 指令枚举
- [ ] 实现 LUI 和 AUIPC
- [ ] 实现 ECALL/EBREAK 基础框架 (抛出异常占位)
- [ ] 实现 FENCE (no-op for now)
- [ ] 添加单元测试

**验证**:

```bash
cargo test instruction::u_type::tests
cargo test instruction::system::tests
```

**退出标准**:

- 5 条指令各 2+ 测试 = 10+ 测试通过
- ECALL/EBREAK 正确抛出异常

---

### 步骤 2.9: 指令执行集成

**文件**: 更新 `src/cpu/core.rs`, `src/instruction/execute.rs`

**任务**:

- [ ] 创建统一执行模块 `execute.rs`
- [ ] 实现完整的指令分发逻辑 (match 译码结果)
- [ ] 更新 CPU 的 execute() 方法
- [ ] 更新 PC 更新逻辑 (处理分支/跳转)
- [ ] 添加集成测试

**验证**:

```bash
# 单元测试
cargo test cpu::execute

# 集成测试
cargo test --test instruction_tests

# 运行示例程序 (如果存在)
if [ -f tests/fib.bin ]; then
    cargo run -- tests/fib.bin
    # 验证输出是 55
fi
```

**退出标准**:

- 所有 40 条指令单元测试通过 (80+ 测试)
- 集成测试通过
- 能正确计算 fib(10) = 55 (或使用简单测试程序)

---

## Phase 3: 5 级流水线实现

### 目标

实现经典 5 级流水线 (IF/ID/EX/MEM/WB)，处理数据冒险和控制冒险。

**退出标准** (可测量):

- 流水线测试 100% 通过
- IPC > 0.7 (非分支密集型负载)
- 单周期 CPU 仍然可用 (feature flag)

---

### 步骤 3.1: 流水线寄存器

**文件**: `src/pipeline/registers.rs`

**任务**:

- [ ] 创建 pipeline 模块
- [ ] 定义 IF/ID 寄存器结构
- [ ] 定义 ID/EX 寄存器结构
- [ ] 定义 EX/MEM 寄存器结构
- [ ] 定义 MEM/WB 寄存器结构
- [ ] 实现 Debug trait

**验证**:

```bash
cargo test pipeline::registers::tests
```

**退出标准**: 寄存器结构测试 100% 通过

---

### 步骤 3.2: IF 阶段实现

**文件**: `src/pipeline/stage/if_stage.rs`

**任务**:

- [ ] 实现 IF 阶段取指逻辑
- [ ] 处理 PC 更新
- [ ] 处理分支冲刷时的 PC 恢复
- [ ] 添加测试

**验证**:

```bash
cargo test pipeline::if_stage::tests
```

---

### 步骤 3.3: ID 阶段实现

**文件**: `src/pipeline/stage/id_stage.rs`

**任务**:

- [ ] 实现 ID 阶段译码
- [ ] 实现寄存器读取
- [ ] 实现立即数生成
- [ ] 实现控制信号生成
- [ ] 添加测试

**验证**:

```bash
cargo test pipeline::id_stage::tests
```

---

### 步骤 3.4: EX 阶段实现

**文件**: `src/pipeline/stage/ex_stage.rs`

**任务**:

- [ ] 实现 EX 阶段 ALU 操作
- [ ] 实现分支目标计算
- [ ] 实现分支条件判断
- [ ] 添加测试

**验证**:

```bash
cargo test pipeline::ex_stage::tests
```

---

### 步骤 3.5: MEM 阶段实现

**文件**: `src/pipeline/stage/mem_stage.rs`

**任务**:

- [ ] 实现 MEM 阶段内存访问
- [ ] 处理 Load/Store 操作
- [ ] 添加测试

**验证**:

```bash
cargo test pipeline::mem_stage::tests
```

---

### 步骤 3.6: WB 阶段实现

**文件**: `src/pipeline/stage/wb_stage.rs`

**任务**:

- [ ] 实现 WB 阶段写回
- [ ] 处理寄存器写回
- [ ] 添加测试

**验证**:

```bash
cargo test pipeline::wb_stage::tests
```

---

### 步骤 3.7: 冒险检测与处理

**文件**: `src/pipeline/hazard.rs`

**任务**:

- [ ] 实现数据冒险检测单元
- [ ] 实现 EX/EX 前递
- [ ] 实现 MEM/EX 前递
- [ ] 实现 Load-Use 冒险检测与暂停
- [ ] 添加冒險测试

**验证**:

```bash
cargo test pipeline::hazard::tests
```

**退出标准**:

- 冒险检测 100% 正确
- Load-Use 暂停正确插入气泡

---

### 步骤 3.8: 控制冒险处理

**文件**: `src/pipeline/branch.rs`

**任务**:

- [ ] 实现静态分支预测 (预测不跳转)
- [ ] 实现分支冲刷逻辑
- [ ] 添加分支测试

**验证**:

```bash
cargo test pipeline::branch::tests
```

**退出标准**:

- 分支预测正确率统计可用
- 冲刷逻辑不泄漏指令

---

### 步骤 3.9: 流水线 CPU 集成

**文件**: `src/cpu/pipeline_core.rs`, `src/cpu/mod.rs`

**任务**:

- [ ] 创建 PipelineCpu 结构
- [ ] 实现流水线 step() 方法
- [ ] 添加 feature flag "pipeline" 切换单周期/流水线
- [ ] 保持向后兼容
- [ ] 性能基准测试

**验证**:

```bash
# 单周期模式仍可用
cargo run --no-default-features

# 流水线模式
cargo run --features pipeline

# 性能测试
cargo bench --bench pipeline

# IPC 计算
cargo run --features pipeline -- --benchmark tests/add_loop.bin | grep IPC
```

**退出标准**:

- 流水线集成测试通过
- IPC > 0.7 (add_loop 基准)
- 单周期 CPU 功能不受影响

---

## Phase 4: 特权级与异常

### 目标

实现 M/S/U 三级特权模式，支持异常和中断处理。

**退出标准** (可测量):

- 通过 riscv-tests 特权级测试 (可用测试)
- ECALL/ERET 正确切换特权级
- 中断响应延迟 < 10 周期

---

### 步骤 4.1: CSR 寄存器框架

**文件**: `src/csr/mod.rs`, `src/csr/register.rs`, `src/csr/bank.rs`

**任务**:

- [ ] 创建 csr 模块
- [ ] 定义 CsrRegister trait (read, write, bits)
- [ ] 实现 CSR 地址常量 (所有 CSR 地址)
- [ ] 实现 CSR 银行结构 (按地址索引)
- [ ] 实现读/写权限检查
- [ ] 添加框架测试

**验证**:

```bash
cargo test csr::framework::tests
```

**退出标准**: CSR 框架测试通过

---

### 步骤 4.2: M-mode CSR 实现

**文件**: `src/csr/machine.rs`

**CSR 列表**: mstatus, mtvec, mepc, mcause, mie, mip, mscratch, mtval

**任务**:

- [ ] 实现每个 M-mode CSR 结构
- [ ] 实现字段访问 (MIE, MPIE, MPP, 等)
- [ ] 实现副作用 (如写 mtvec 时的检查)
- [ ] 添加测试

**验证**:

```bash
cargo test csr::machine::tests
```

**退出标准**: 所有 M-mode CSR 测试通过

---

### 步骤 4.3: S-mode CSR 实现

**文件**: `src/csr/supervisor.rs`

**CSR 列表**: sstatus, stvec, sepc, scause, sie, sip, sscratch

**依赖**: 需要 M-mode CSR 完成 (用于委托机制)

**任务**:

- [ ] 实现每个 S-mode CSR 结构
- [ ] 实现与 M-mode 的委托关系
- [ ] 实现 mideleg/sideleg 交互
- [ ] 添加测试

**验证**:

```bash
cargo test csr::supervisor::tests
```

**退出标准**: S-mode CSR 测试通过，委托机制正确

---

### 步骤 4.4: 特权级切换

**文件**: `src/cpu/privilege.rs`

**依赖**: 需要 M/S CSR 完成

**任务**:

- [ ] 实现 ecall 指令 (陷入 M-mode)
- [ ] 实现 mret/sret/uret 指令
- [ ] 实现特权级检查逻辑
- [ ] 更新 CPU 状态保存/恢复
- [ ] 添加测试

**验证**:

```bash
cargo test privilege::switch::tests
cargo test --test privilege_tests
```

**退出标准**:

- 特权级切换测试通过
- mstatus.MPP/MPIE 正确保存和恢复

---

### 步骤 4.5: 异常处理框架

**文件**: `src/exception/mod.rs`, `src/exception/trap.rs`

**任务**:

- [ ] 定义异常类型枚举
- [ ] 实现异常入口 (xtvec → xepc + xcause + xstatus)
- [ ] 实现上下文保存
- [ ] 实现异常返回
- [ ] 添加测试

**验证**:

```bash
cargo test exception::trap::tests
```

**退出标准**: 异常处理流程测试通过

---

### 步骤 4.6: 具体异常处理

**文件**: `src/exception/handler.rs`

**任务**:

- [ ] 实现非法指令异常
- [ ] 实现访问错误异常
- [ ] 实现断点异常
- [ ] 实现系统调用处理
- [ ] 实现环境断点
- [ ] 添加测试

**验证**:

```bash
cargo test exception::handler::tests
```

**退出标准**: 所有异常类型正确处理

---

### 步骤 4.7: 中断系统

**文件**: `src/interrupt/mod.rs`, `src/interrupt/clint.rs`

**任务**:

- [ ] 实现中断使能/屏蔽逻辑 (mie/mip)
- [ ] 实现中断 Pending 检查
- [ ] 实现中断优先级 (M > S > U)
- [ ] 实现 CLINT (mtime + mtimecmp)
- [ ] 实现时钟中断
- [ ] 添加测试

**验证**:

```bash
cargo test interrupt::clint::tests
cargo test interrupt::pending::tests
```

**退出标准**:

- 中断逻辑测试通过
- 时钟中断能在预期时间触发

---

### 步骤 4.8: PLIC 中断控制器 (可选)

**文件**: `src/interrupt/plic.rs`

**任务**:

- [ ] 实现 PLIC 寄存器
- [ ] 实现中断优先级
- [ ] 实现中断使能/完成
- [ ] 添加测试

**验证**:

```bash
cargo test interrupt::plic::tests
```

---

## Phase 5: 外设与调试

### 目标

实现外设、程序加载和调试接口，使模拟器可用于实际开发。

**退出标准** (可测量):

- 可通过 GDB 单步调试
- DiffTest 能检测差异
- 可加载并运行 ELF 程序

---

### 步骤 5.1: UART 外设

**文件**: `src/peripheral/uart.rs`

**任务**:

- [ ] 实现 NS16550A 兼容 UART
- [ ] 实现 THR (发送保持) 寄存器
- [ ] 实现 RBR (接收缓冲) 寄存器
- [ ] 实现 LSR (线路状态) 寄存器
- [ ] 实现 LCR (线路控制) 寄存器
- [ ] 实现中断使能 (IER)
- [ ] 添加测试

**验证**:

```bash
cargo test peripheral::uart::tests
```

**退出标准**: UART 寄存器读写测试通过

---

### 步骤 5.2: Timer 外设

**文件**: `src/peripheral/timer.rs`

**任务**:

- [ ] 实现 mtime 寄存器 (64 位，高/低分开)
- [ ] 实现 mtimecmp 寄存器 (64 位)
- [ ] 实现 tick 计数器
- [ ] 实现时钟中断触发
- [ ] 添加测试

**验证**:

```bash
cargo test peripheral::timer::tests
```

**退出标准**: Timer 测试通过，中断正确触发

---

### 步骤 5.3: ELF 加载器

**文件**: `src/loader/elf.rs`

**依赖**: 添加 goblin crate

**任务**:

- [ ] 更新 Cargo.toml 添加 goblin 依赖
- [ ] 实现 ELF 头解析
- [ ] 实现 Program Header 遍历
- [ ] 实现可加载段加载
- [ ] 设置入口点 PC
- [ ] 添加测试 (使用测试 ELF)

**验证**:

```bash
# 验证依赖添加
grep goblin Cargo.toml

cargo test loader::elf::tests

# 运行实际 ELF (如果存在)
if [ -f tests/hello.elf ]; then
    cargo run -- tests/hello.elf
fi
```

**退出标准**:

- ELF 加载测试通过
- 能正确加载并跳转到入口点

---

### 步骤 5.4: GDB Remote Protocol - 基础

**文件**: `src/debug/gdb.rs`, `src/debug/gdb/commands.rs`

**任务**:

- [ ] 添加 tokio 依赖 (异步 TCP)
- [ ] 实现 TCP Server 监听
- [ ] 实现协议编解码 (RLE, hex)
- [ ] 实现基础命令: `?`, `g`, `G`, `m`, `M`
- [ ] 添加测试

**验证**:

```bash
cargo test debug::gdb::protocol::tests

# 手动测试 (后台运行)
cargo run -- --gdb 1234 &
GDB_PID=$!
sleep 2
echo "Testing GDB connection..."
echo $? > test_result.txt
kill $GDB_PID
```

**退出标准**: GDB 协议测试通过

---

### 步骤 5.5: GDB Remote Protocol - 执行控制

**文件**: `src/debug/gdb/execute.rs`

**任务**:

- [ ] 实现 `c` (continue) 命令
- [ ] 实现 `s` (step) 命令
- [ ] 实现断点支持: `Z0`, `z0`
- [ ] 实现信号处理
- [ ] 添加测试

**验证**:

```bash
cargo test debug::gdb::execute::tests
```

**退出标准**: 执行控制测试通过

---

### 步骤 5.6: GDB Remote Protocol - 查询

**文件**: `src/debug/gdb/query.rs`

**任务**:

- [ ] 实现 `qSupported` 命令
- [ ] 实现 `qAttached` 命令
- [ ] 实现 `qC` (当前线程)
- [ ] 实现 `qfThreadInfo`/`qsThreadInfo`
- [ ] 添加测试

**验证**:

```bash
cargo test debug::gdb::query::tests
```

**退出标准**: 查询命令测试通过

---

### 步骤 5.7: CLI 参数扩展

**文件**: `src/main.rs`

**任务**:

- [ ] 添加 `--gdb <port>` 参数
- [ ] 添加 `--difftest` 参数
- [ ] 添加 `--test-elf <path>` 参数
- [ ] 更新帮助信息
- [ ] 添加 CLI 测试

**验证**:

```bash
cargo run -- --help | grep -E "gdb|difftest"
cargo test cli::arguments::tests
```

**退出标准**: CLI 参数解析正确

---

### 步骤 5.8: DiffTest 框架

**文件**: `src/debug/difftest.rs`, `src/debug/qemu.rs`

**依赖**: QEMU 安装且可执行

**任务**:

- [ ] 实现 QEMU 启动和连接
- [ ] 实现状态对比逻辑
  - 通用寄存器 (x1-x31)
  - PC 寄存器
  - 关键 CSR (mstatus, mepc, mcause)
- [ ] 实现错误报告与日志
- [ ] 实现指令历史记录 (最近 100 条)
- [ ] 添加测试

**验证**:

```bash
# 验证 QEMU 可用
qemu-riscv32 --version || echo "QEMU not installed"

cargo test debug::difftest::tests

# 运行 DiffTest (需要 QEMU)
if command -v qemu-riscv32; then
    cargo run -- --difftest --test-elf tests/simple.elf
fi
```

**退出标准**:

- DiffTest 框架测试通过
- 能正确对比 CPU 状态
- 差异能正确报告

---

### 步骤 5.9: VSCode 集成

**文件**: `.vscode/launch.json`, `.vscode/tasks.json`

**任务**:

- [ ] 创建 VSCode 调试配置
- [ ] 创建构建任务
- [ ] 创建测试任务
- [ ] 添加 README 说明

**验证**:

```bash
test -f .vscode/launch.json && echo "OK" || echo "MISSING"
test -f .vscode/tasks.json && echo "OK" || echo "MISSING"
```

**退出标准**: VSCode 配置文件存在且格式正确

---

## 加分项 (可选)

### M 扩展 (乘除法)

**文件**: `src/instruction/m_type.rs`

**指令**: MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU

**退出标准**: 8 条指令测试通过

### C 扩展 (压缩指令)

**文件**: `src/instruction/c_type.rs`

**任务**: 16 位压缩指令解码

**退出标准**: 常用压缩指令测试通过

### Sv32 分页

**文件**: `src/mmu/mod.rs`, `src/mmu/tlb.rs`

**任务**: 页表结构、TLB 缓存、地址翻译

**退出标准**: 页翻译测试通过

---

## 测试策略

### 单元测试

每个模块包含 `#[cfg(test)]` 测试，目标覆盖率 80%+

**验证方法**:

```bash
cargo llvm-cov --html
```

### 集成测试

使用 riscv-tests 官方测试套件

**集成步骤**:

1. 下载 riscv-tests
2. 构建测试 ELF
3. 添加到 tests/ 目录
4. 创建测试运行脚本

### DiffTest

与 QEMU 逐指令对比验证

**启动命令**:

```bash
qemu-riscv32 -g 1234 test.elf &
cargo run -- --difftest --gdb-local 1234
```

---

## 提交策略

### Phase 2 提交计划

```
feat(tests): 添加指令测试基础设施
feat(instruction): 实现指令译码器
feat(instruction): 实现 R-type 算术逻辑指令
feat(instruction): 实现 I-type 立即数指令
feat(instruction): 实现 Load 指令
feat(instruction): 实现 Store 指令
feat(instruction): 实现分支指令
feat(instruction): 实现跳转指令
feat(instruction): 实现 System 和 U-type 指令
feat(instruction): 集成指令执行到 CPU
test(instruction): 添加指令集成测试
```

### Phase 3-5 提交计划

每个 Phase 完成后创建一个里程碑 PR:

- `feat(pipeline): 实现 5 级流水线`
- `feat(privilege): 实现 M/S/U 特权级`
- `feat(debug): 实现 GDB 调试和 DiffTest`

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
if [ -f docs/ROADMAP.md ]; then
    echo "5. Checking documentation..."
    grep -E "✅|❌" docs/ROADMAP.md || echo "No progress markers found"
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

---

*蓝图版本: 3.0*
*创建日期: 2026-03-24*
*最后更新: 2026-04-02 (同步 Phase 6 全链路完成状态)*
*备注: v2 细化实施步骤已转为历史归档，现行推进请参考 ROADMAP 与 MILESTONE_EXECUTION*
