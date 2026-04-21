# 课程汇报演示指南

## 演示结构 (10-15 分钟)

## 快速启动（Framebuffer 演示）

```bash
# 仓库根目录执行（Windows）
powershell -ExecutionPolicy Bypass -File .\scripts\run_framebuffer_demo.ps1
```

默认模式会：

- 启动 `visualize --linux-fb-demo --warmup 3500`
- 启动前执行 WebSocket 探针验收（`fb linux` 返回非零像素）
- 自动打开前端页面

如需使用旧的图案命令演示（`fb_demo`）：

```bash
powershell -ExecutionPolicy Bypass -File .\scripts\run_framebuffer_demo.ps1 -Mode pattern-demo
```

## 快速验收（Phase 4：输入 + 帧缓冲）

```bash
# host 演示链路验收（fb_game + input + framebuffer）
powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode host-demo

# guest 演示链路验收（guest binary + stepn + input/framebuffer）
powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode guest-binary
```

期望输出关键字：

- `PASS(host-demo)`
- `PASS(guest-binary)`

说明：guest 模式在未提供 `-GuestProgram` 时会自动生成最小 RV32 帧缓冲 demo 二进制用于验收。

## 快速验收（Phase 6：一键编排）

```bash
# 轻量烟测（仅验证脚本框架与协处理器回归阶段）
powershell -ExecutionPolicy Bypass -File .\scripts\run_phase6_showcase_pipeline.ps1 -SkipBuild -SkipXv6 -SkipPhase4 -SkipFrontendBuild

# 全量串联（在 Linux 工件准备齐全时启用）
powershell -ExecutionPolicy Bypass -File .\scripts\run_phase6_showcase_pipeline.ps1 -EnableLinux
```

已验证结果（2026-04-02）：

- `-EnableLinux` 全量模式 PASS（xv6、Linux、Phase4 host/guest、NPU/LPU〔含 LPU Language 迁移能力〕、GPU/TPU、frontend build 全通过）。

说明：

- 编排日志统一输出到 `target/phase6-demo-logs`。
- Linux 阶段默认关闭，避免在缺少工件时阻塞整条演示链路。

浏览器操作：

1. 进入 `Framebuffer` 标签页
2. 点击 `Game Flow` 区域的 `Init` 初始化游戏状态
3. 点击 `Run`（或 `Step`）驱动 `fb_game`，再观察帧变化
4. 使用 `Input Panel`（键盘 WASD/方向键 + J/K）验证输入响应
5. 观察画布左上角 Overlay：`FPS/IPC/Stalls/Tick/Score/InputBits`
6. 如需直接读取 Linux 预设地址，点击 `Linux Preset` + `Refresh`
7. （图案模式）选择 `pong/checker/gradient` 后点击 `Demo Frame`

### 5 分钟答辩推荐演示顺序（live + 录屏兜底）

1. `Pipeline` 标签页：
   - 顶部 **hazard 徽标**（`STALL / FLUSH / CONTROL HAZARD`）作为“当前周期状态总览”；
   - 时间轴观察 `Load-Use` 与 `Branch Data` 的 stall 标识；
   - 底部联动看 `Branch Predictor` 与 `Memory Hierarchy`。
2. `Pipeline` 底部 `CSR / Trap` 面板：
   - 展示 `mstatus/mtvec/mepc/mcause/satp/mie/mip`；
   - 讲解 `Latest Trap Summary`（`cause_label/cause_code/handler_mode/epc/tval`）。
3. 切换 `Framebuffer`：
   - 触发 `Init` + `Run`；
   - 用 `Input Panel` 操作并观察 Overlay（`FPS/IPC/Stalls/Tick/Score/InputBits`）。
4. 切换 `Coprocessor`：
   - 快速展示 NPU/LPU/GPU/TPU 状态与任务统计。

### 录屏兜底建议

- 若 live 网络/终端异常，按顺序切换预录片段：
  1) Pipeline hazard + timeline；
  2) CSR/Trap 面板；
  3) Framebuffer 输入交互；
  4) Coprocessor 状态页。
- 每段录屏时长建议 15-25 秒，切换时口播“该片段对应当前版本同一构建产物”。

说明：后端 `visualize` 支持不传程序文件，且可通过 `--linux-fb-demo` 预置 RV32I 帧缓冲写入程序。

### 时间分配

| 阶段     | 时长   | 内容            | 目的         |
| -------- | ------ | --------------- | ------------ |
| 开场     | 1 分钟 | 项目背景和目标  | 建立上下文   |
| 架构展示 | 2 分钟 | 5 级流水线动画  | 视觉冲击     |
| 功能演示 | 4 分钟 | 运行程序 + 调试 | 展示核心功能 |
| 技术亮点 | 3 分钟 | DiffTest + 前递 | 展示深度     |
| 创新点   | 2 分钟 | 可视化/扩展     | 差异化       |
| 总结     | 1 分钟 | 成果和收获      | 收尾         |

---

## 第一部分：开场 (1 分钟)

### 脚本模板

> "大家好，我今天要展示的是 myCPU —— 一个使用 Rust 实现的 RISC-V RV32I 模拟器。
>
> 这个项目实现了：
>
> - 完整的 RV32I 指令集（40 条指令）
> - 经典的 5 级流水线架构
> - M/S/U 三级特权模式
> - 完整的异常和中断处理
> - GDB 调试支持
> - 性能监控 (HPM CSR + 性能报告)
>
> 接下来我将通过实际演示来展示这些功能。"

### 关键数字

- **代码量**: ~6000 行 Rust 代码
- **测试覆盖**: 274 单元测试，全部通过（截至 2026-04-03）
- **课设排期**: Phase 1-6 统一对齐第4-8周

---

## 第二部分：架构展示 (2 分钟)

### 准备材料

1. **架构图**: 提前准备好 Mermaid 或 PPT 图
2. **流水线动画**: 如果有可视化，直接演示

### 架构图 (备用)

```text
┌─────────────────────────────────────────────────────────────────────┐
│                         myCPU Architecture                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│    ┌─────┐   ┌─────┐   ┌─────┐   ┌─────┐   ┌─────┐               │
│    │ IF  │──▶│ ID  │──▶│ EX  │──▶│ MEM │──▶│ WB  │  Pipeline    │
│    └─────┘   └─────┘   └─────┘   └─────┘   └─────┘               │
│       │         │         │         │         │                    │
│       ▼         ▼         ▼         ▼         ▼                    │
│    ┌─────────────────────────────────────────────┐                │
│    │              Register File (x0-x31)          │                │
│    └─────────────────────────────────────────────┘                │
│                        │                                            │
│    ┌───────────────────┼───────────────────┐                      │
│    │                   │                   │                      │
│    ▼                   ▼                   ▼                      │
│ ┌──────┐         ┌──────┐          ┌──────┐                     │
│ │ RAM  │         │ UART │          │ CLINT│   Peripherals       │
│ └──────┘         └──────┘          └──────┘                     │
│                                                                   │
└─────────────────────────────────────────────────────────────────────┘
```

### 讲解要点

1. **5 级流水线**: IF(取指) → ID(译码) → EX(执行) → MEM(内存) → WB(写回)
2. **数据前递**: 解决 RAW 数据冒险
3. **Load-Use 暂停**: 处理 Load 后立即使用的数据冒险
4. **分支预测**: 静态预测不跳转 + 冲刷

---

## 第三部分：功能演示 (4 分钟)

### 演示资产准备（必做）

在运行 Demo 1/2/3 之前，先构建演示程序：

```bash
# Windows（仓库根目录）
powershell -ExecutionPolicy Bypass -File .\scripts\build_demo_programs.ps1

# Unix / WSL
bash tests/programs/build.sh
```

说明：

- 会生成 `tests/programs/hello.bin`、`tests/programs/fib.elf`、`tests/programs/test.elf` 等演示产物。
- `--ecall-exit` 是 `run` 子命令参数，必须写在 `run` 后面：
    - 正确：`cargo run --release -- run --ecall-exit tests/programs/hello.bin`
    - 错误：`cargo run --release --ecall-exit -- run tests/programs/hello.bin`
- 若在 WSL 内执行 Rust 命令遇到 `lock file version '4'` 报错，说明 WSL 内 `cargo` 版本过旧；先升级后再测：

```bash
wsl bash -lc 'rustup update stable'
wsl bash -lc 'cargo --version && rustc --version'
```

网络受限无法升级时，推荐“WSL 构建 demo 资产 + Windows 侧运行 cargo 测试/演示”的组合流程。

### Demo 1: 运行 Hello World (1 分钟)

```bash
# 终端演示
cargo run --release -- run --ecall-exit tests/programs/hello.bin

# 预期输出
Hello
```

**讲解**: "这是一个简单的 Hello World 程序，通过 UART 输出字符串。可以看到程序正确执行并输出。"

### Demo 2: GDB 调试 (1.5 分钟)

```bash
# 终端 1: 启动调试服务器
cargo run --release -- debug tests/programs/fib.elf
GDB server listening on port 1234

# 终端 2: 连接 GDB
gdb-multiarch tests/programs/fib.elf
(gdb) set architecture riscv:rv32
(gdb) target remote :1234
(gdb) break main
(gdb) continue
(gdb) info registers
(gdb) stepi
(gdb) x/5i $pc
```

自动化测试：`tests/demo_gdb.rs`（默认跳过；运行前设置环境变量 `RUN_DEMO_TESTS=1`）

**讲解**: "我们支持标准的 GDB 调试协议。可以设置断点、单步执行、查看寄存器，和调试真实硬件一样。"

### Demo 3: DiffTest 能力验证 (1.5 分钟)

```bash
# 运行 DiffTest 模块相关测试（当前仓库可直接执行）
cargo test --lib difftest

# 输出 (如果通过)
running 2 tests
test difftest::tests::test_history_entry ... ok
test difftest::tests::test_difftest_error_display ... ok
```

**讲解**: "我们实现了 DiffTest 核心模块（连接、步进、状态比较与错误报告）。当前课堂演示采用模块级验证，工程化全链路由 `scripts/run_riscv_tests.sh` 承载。"

#### 现场逐指令对比示例（可复现步骤）

下面给出一个低风险、可在本地复现的演示路径（不修改 Cargo feature）：

1. 在 WSL/Unix 环境准备演示二进制（在仓库根目录）：

```bash
# 在 Unix/WSL 中构建 demo 程序
bash tests/programs/build.sh
```

2. 在 WSL 中后台启动 QEMU（开启 GDB stub :1234 并暂停）：

```bash
# 在 WSL 中（工作目录 /mnt/d/code/myCPU）
qemu-system-riscv32 -M virt -nographic -bios none -kernel tests/programs/test.elf -S -s &
```

3. 在同一 WSL 环境中运行示例程序（例子已加入 `examples/difftest_demo.rs`）：

```bash
cargo run --example difftest_demo -- tests/programs/test.elf
```

示例行为：示例会把 ELF 加载到 myCPU 模拟器中，每步执行 myCPU，然后通过 GDB 协议驱动 QEMU 单步并比较状态；发现不一致时会打印详细差异并退出。

注意与故障排查：
- 如果在 WSL 中执行 `cargo run` 遇到 `lock file version '4'` 等错误，请在 WSL 中更新 Rust/Cargo（`rustup update stable`）并重试；某些内网/离线环境需要先配置代理或使用离线工具链。  
- 如果 WSL 网络/包管理受限导致无法更新，请考虑在同一环境（Windows / WSL）内同时运行 QEMU 与示例，或者在另一台具备网络/工具链的机器上演示。  
- 若 QEMU 启动成功但示例无法连接，请检查防火墙或 /proc/net/tcp 中 1234 监听状态：`ss -ltnp | grep 1234`。

若你希望我在当前环境继续尝试，我已尝试在 WSL 中启动 QEMU 并运行示例，但遇到 WSL 中的 Cargo 版本与网络更新限制（无法完成 `rustup update`），导致无法本地运行示例。我可以继续帮助：

- 在你允许的情况下尝试在你的机器上执行 `rustup update` 后重试运行示例；或
- 在文档中再补充一份“仅演示/录屏”流程及输出样例以备现场使用（我可以生成示例输出）。

---

## 第四部分：技术亮点 (3 分钟)

### 亮点 1: 流水线前递 (1 分钟)

**展示代码**: `src/cpu/pipeline/forward.rs`

```rust
// 前递逻辑核心
fn resolve_forward(&self, rs: RegIdx) -> u32 {
    // EX/MEM 前递
    if self.ex_mem.rd == rs && self.ex_mem.rd != 0 {
        return self.ex_mem.alu_result;
    }
    // MEM/WB 前递
    if self.mem_wb.rd == rs && self.mem_wb.rd != 0 {
        return self.mem_wb.mem_result;
    }
    // 从寄存器文件读取
    self.regs.read(rs)
}
```

**讲解**: "前递是流水线的核心技术。当一条指令需要使用前一条指令的结果时，我们不需要等待写回，而是直接从流水线寄存器获取。"

### 亮点 2: 冒险检测 (1 分钟)

**展示代码**: `src/cpu/pipeline/hazard.rs`

```rust
// Load-Use 冒险检测
fn has_load_use_hazard(&self) -> bool {
    let id_ex = &self.id_ex;
    let if_id = &self.if_id;
  
    // ID 阶段指令需要使用 EX 阶段 Load 的目标寄存器
    id_ex.mem_read && 
    (id_ex.rd == if_id.rs1 || id_ex.rd == if_id.rs2)
}
```

**讲解**: "Load-Use 冒险是最难处理的数据冒险之一。当一条 Load 指令后紧跟使用其结果的指令时，我们必须暂停流水线一个周期。"

### 亮点 3: 特权级切换 (1 分钟)

**展示场景**: 系统调用处理

```rust
// ecall 处理
fn handle_ecall(&mut self) -> Result<()> {
    // 1. 保存当前状态
    let pc = self.pc;
    let mode = self.privilege;
  
    // 2. 切换到更高特权级
    self.privilege = PrivilegeMode::Machine;
  
    // 3. 跳转到异常处理程序
    self.pc = self.csr.read(mtvec)?;
  
    // 4. 保存上下文
    self.csr.write(mepc, pc)?;
    self.csr.write(mcause, CAUSE_ECALL)?;
  
    Ok(())
}
```

---

## 第五部分：创新点 (2 分钟)

### 创新点 1: Web 可视化 (如果有)

**演示**: 打开浏览器，展示流水线动画

> "我们开发了 Web 前端可视化界面，可以直观地看到指令如何在流水线中流动，以及数据前递的路径。"

### 创新点 2: 性能监控 ✅

**演示**: 运行程序，输出性能报告

```bash
cargo run --release -- run --ecall-exit --perf-report tests/programs/fib.elf

╔══════════════════════════════════════════════════════════════╗
║                    Performance Report                         ║
╠══════════════════════════════════════════════════════════════╣
║  Execution Summary                                            ║
╠══════════════════════════════════════════════════════════════╣
║  Cycles:                                          1,234,567  ║
║  Instructions:                                      987,654  ║
║  IPC:                                                 0.80   ║
║  CPI:                                                1.25   ║
║  Efficiency:                                        80.0%   ║
╠══════════════════════════════════════════════════════════════╣
║  Pipeline Hazards                                             ║
╠══════════════════════════════════════════════════════════════╣
║  Load-Use Stalls:                                    12,345  ║
║  Control Hazards:                                     8,765  ║
║  Total Stalls:                                       21,110  ║
║  Stall Rate:                                          1.7%   ║
╠══════════════════════════════════════════════════════════════╣
║  Branch Statistics                                            ║
╠══════════════════════════════════════════════════════════════╣
║  Total Branches:                                     45,678  ║
║  Prediction Accuracy:                                51.4%   ║
╚══════════════════════════════════════════════════════════════╝
```

**讲解**: "我们实现了符合 RISC-V HPM 规范的性能计数器，可以精确统计流水线效率、暂停率、分支预测准确率等关键指标。"

### 创新点 3: M 扩展 (如果有)

**演示**: 运行乘除法测试

```bash
cargo test mul_div

running 8 tests
test mul ... ok
test mulh ... ok
test div ... ok
test divu ... ok
...
test result: ok. 8 passed
```

---

## 第六部分：总结 (1 分钟)

### 总结模板

> "总结一下，myCPU 项目：
>
> 1. **完整实现了 RV32I 指令集**，包括 40 条基础指令
> 2. **实现了 5 级流水线**，包含前递和冒险处理
> 3. **支持完整的特权级架构**，可以运行真实的操作系统
> 4. **提供了完善的调试支持**，包括 GDB 和 DiffTest
> 5. **实现了性能监控功能**，符合 RISC-V HPM 规范
>
> 通过这个项目，我深入理解了：
>
> - CPU 流水线的工作原理
> - 数据冒险和控制冒险的处理方法
> - 特权级和异常处理机制
> - 性能计数器和流水线效率分析
> - 以及如何用 Rust 实现高性能的系统软件
>
> 谢谢大家！"

---

## 演示前检查清单

### 环境准备

- [ ] Rust 工具链已安装 (`rustc --version`)
- [ ] RISC-V 交叉编译器已安装 (`riscv32-unknown-elf-gcc --version`)
- [ ] QEMU RISC-V 已安装 (`qemu-system-riscv32 --version`)
- [ ] GDB 已安装 (`riscv32-unknown-elf-gdb --version`)

### 程序准备

- [ ] 已执行演示程序构建脚本（Windows: `scripts/build_demo_programs.ps1` / Unix: `tests/programs/build.sh`）
- [ ] Hello World 程序已编译 (`tests/programs/hello.bin`)
- [ ] 斐波那契程序已编译 (`tests/programs/fib.elf`)
- [ ] DiffTest 样例程序已编译 (`tests/programs/test.elf`)
- [ ] (如果有) CoreMark 已编译

### 演示材料

- [ ] PPT/幻灯片已准备
- [ ] 架构图已准备
- [ ] 代码片段已截图 (备用)
- [ ] 备用视频已录制 (防止 Live Demo 失败)

### 终端准备

- [ ] 字体大小合适 (建议 18pt+)
- [ ] 终端背景色适合投影
- [ ] 命令历史已清理
- [ ] 预先输入常用命令 (按需)

---

## 常见问题准备

### Q1: 为什么选择 RISC-V？

> "RISC-V 是开放标准的指令集，没有专利限制，文档公开。它的设计简洁，基础指令集只有 40 条，非常适合学习和实现。同时它也有完善的生态系统，可以运行真实的操作系统和程序。"

### Q2: 为什么用 Rust 而不是 C++？

> "Rust 提供了内存安全保证，在编译时就能捕获很多错误。它的模式匹配和类型系统非常适合处理指令解码和状态机。同时 Rust 的性能与 C++ 相当，没有运行时开销。"

### Q3: 流水线的性能如何？

> "我们实现了性能监控功能，可以精确测量 IPC、暂停率等指标。理想 IPC 是 1.0，但实际由于分支预测错误和数据冒险，IPC 通常在 0.7-0.9 之间。通过 `--perf-report` 选项可以输出详细的性能分析报告。"

### Q4: DiffTest 是什么？

> "DiffTest 是一种验证技术，我们将自己的模拟器与 QEMU（参考实现）逐指令对比。如果状态不一致，就说明我们的实现有问题。这种方法能快速发现错误，确保正确性。"

### Q5: 这个模拟器能运行什么程序？

> "目前可以运行简单的 C/Rust 程序，通过 UART 输出。理论上任何 RV32I 程序都可以运行，包括简单的操作系统内核。"

---

## 应急预案

### Live Demo 失败

1. **准备备用视频**: 提前录制演示视频
2. **准备截图**: 关键步骤的截图
3. **切换到代码讲解**: 直接讲解代码逻辑

### 时间不足

1. **跳过 GDB 演示**: 用截图代替
2. **简化技术讲解**: 只讲核心概念
3. **减少 Q&A**: "可以课后交流"

### 时间剩余

1. **深入讲解某个技术点**
2. **展示更多测试程序**
3. **讨论未来改进方向**

---

## 成功标准

演示成功的标志：

- [ ] 流水线架构清晰展示
- [ ] 至少一个程序成功运行
- [ ] 调试功能正常工作
- [ ] 技术亮点讲解到位
- [ ] 时间控制在 10-15 分钟
- [ ] 回答了至少一个问题
