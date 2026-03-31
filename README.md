# myCPU: RISC-V 指令集模拟器

> 从零实现一个支持完整特权级的 5 级流水线 RISC-V 虚拟机

## 项目概述

myCPU 是一个使用 Rust 实现的 RISC-V (RV32I) 指令集模拟器，面向"硬件设计者"视角，以指令集模拟器为核心目标，完成 CPU、指令、内存、中断、外设的模拟实现。

### 核心特性

- **5 级流水线**: IF → ID → EX → MEM → WB
- **完整特权级**: Machine + Supervisor + User 模式
- **可扩展外设**: UART、Timer、VirtIO 等
- **操作系统支持**: 为后续 OS 课程预留完整接口

### 当前进度

- ✅ **Phase 1 完成**: 基础框架已实现
  - Memory trait + RAM/ROM 实现
  - 32 个通用寄存器 (x0-x31)
  - 程序计数器 (PC)
  - CPU 核心结构和主循环框架
  - 系统总线 (Bus) 和内存映射
- ✅ **Phase 2 完成**: RV32I 指令集 (40 条)
- ✅ **Phase 3 完成**: 5 级流水线 + 冒险处理
- ✅ **Phase 4 完成**: M/S/U 特权级 + 异常中断
- ✅ **Phase 5 完成**: UART + ELF 加载 + GDB 调试 + DiffTest
- ✅ **Phase 6 P0 完成**: 性能监控 (HPM CSR + PerfCollector)

## 项目目标

1. 理解指令集架构（ISA）与 CPU 执行模型
2. 掌握取指–译码–执行的流水线思想
3. 理解内存、寄存器、异常、中断、外设接口原理
4. 能够用高级语言实现一个可运行裸机程序的模拟器

## 快速开始

```bash
# 构建项目
cargo build --release

# 运行测试
cargo test

# 运行模拟器 (ELF 文件)
cargo run --release -- run program.elf

# 带详细输出运行
cargo run --release -- run --verbose program.elf

# 生成性能报告
cargo run --release -- run --perf-report program.elf

# 启动 GDB 调试服务器
cargo run --release -- debug program.elf

# 启动可视化（可不传程序文件，进入 Demo 模式）
cargo run --release -- visualize

# 启动可视化（预载 Linux 帧缓冲写入程序并预热执行）
cargo run --release -- visualize --linux-fb-demo --warmup 3500

# Windows 一键启动帧缓冲演示
powershell -ExecutionPolicy Bypass -File .\scripts\run_framebuffer_demo.ps1

# Windows 一键启动图案演示（旧模式）
powershell -ExecutionPolicy Bypass -File .\scripts\run_framebuffer_demo.ps1 -Mode pattern-demo
```

## 文档

- [架构设计](docs/ARCHITECTURE.md) - 系统架构详细说明
- [指令集实现](docs/INSTRUCTIONS.md) - RV32I 指令集实现状态
- [开发计划](docs/ROADMAP.md) - 开发路线图

## 项目结构

```
myCPU/
├── src/
│   ├── lib.rs           # 库入口
│   ├── main.rs          # CLI 入口
│   ├── types.rs         # 基础类型 (Addr, Word, Byte 等)
│   ├── error.rs         # 错误类型定义
│   ├── perf_report.rs   # 性能报告生成
│   ├── traits/          # 核心 trait 定义
│   │   ├── memory.rs    # Memory trait
│   │   └── peripheral.rs # Peripheral trait
│   ├── memory/          # 内存系统
│   │   ├── ram.rs       # RAM 实现
│   │   ├── rom.rs       # ROM 实现
│   │   └── bus.rs       # 系统总线
│   ├── cpu/             # CPU 核心
│   │   ├── mod.rs       # CPU 模块导出
│   │   ├── core.rs      # 单周期 CPU 实现
│   │   ├── registers.rs # 通用寄存器
│   │   ├── pc.rs        # 程序计数器
│   │   ├── state.rs     # CPU 状态快照
│   │   ├── perf_collector.rs # 性能事件收集器
│   │   ├── csr/         # CSR 寄存器
│   │   │   ├── mod.rs   # CSR 模块
│   │   │   ├── perf.rs  # 性能计数器 CSR
│   │   │   ├── machine.rs # M-mode CSR
│   │   │   └── ...
│   │   ├── pipeline/    # 5 级流水线
│   │   │   ├── mod.rs   # 流水线控制
│   │   │   ├── stages/  # 各阶段实现
│   │   │   ├── hazard.rs # 冒险检测
│   │   │   └── forward.rs # 前递逻辑
│   │   └── ...
│   ├── instruction/     # 指令译码和执行
│   ├── interrupt/       # 中断控制器 (CLINT/PLIC)
│   ├── peripheral/      # 外设 (UART)
│   ├── loader/          # ELF 加载器
│   ├── debug/           # GDB 调试接口
│   └── difftest/        # QEMU DiffTest
├── docs/                # 设计文档
├── tests/               # 集成测试
└── firmware/            # 测试固件
```

## 技术栈

| 组件     | 选择                         | 说明                           |
| -------- | ---------------------------- | ------------------------------ |
| 语言     | Rust                         | 内存安全、模式匹配、零成本抽象 |
| 测试     | built-in + pretty_assertions | 单元测试                       |
| CLI      | clap                         | 命令行框架                     |
| 日志     | log + env_logger             | 日志系统                       |
| 错误处理 | thiserror + anyhow           | 错误类型                       |

## 参考资料

- [RISC-V 规范](https://riscv.org/technical/specifications/)
- [RISC-V Reader](https://riscvbook.com/)
- [SiFive SDK](https://www.sifive.com/software)

## License

MIT
