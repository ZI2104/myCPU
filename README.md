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

# Windows 执行 Phase 4 帧缓冲+输入自动验收（host 模式）
powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode host-demo

# Windows 执行 Phase 4 帧缓冲+输入自动验收（guest 模式，含 stepn）
powershell -ExecutionPolicy Bypass -File .\scripts\run_phase4_input_framebuffer_acceptance.ps1 -Mode guest-binary

# Windows 执行 guest CPU 行为验收（输入脚本注入）
powershell -ExecutionPolicy Bypass -File .\scripts\run_mario_cpu_validation.ps1 -GuestBinary .\third_party\xv6-rv32\kernel\kernel -VirtioDisk .\third_party\xv6-rv32\fs.img

# Windows 准备 Phase 3 Linux 工件目录（自动下载 OpenSBI）
powershell -ExecutionPolicy Bypass -File .\scripts\setup_phase3_artifacts.ps1

# Windows 构建 Phase 3 Buildroot 工件（产出 fw_jump.elf/Image/rootfs.ext2）
powershell -ExecutionPolicy Bypass -File .\scripts\build_phase3_buildroot_artifacts.ps1

# Windows 执行 Phase 3 Linux 启动链路验收（自动探测工件）
powershell -ExecutionPolicy Bypass -File .\scripts\run_linux_phase3_acceptance.ps1 -AutoResolveArtifacts -AutoDtb

# Windows 清理临时工作区文件（推荐定期执行）
powershell -ExecutionPolicy Bypass -File .\scripts\cleanup_workspace.ps1
```

## 文档

- [架构设计](docs/ARCHITECTURE.md) - 系统架构详细说明
- [开发计划](docs/ROADMAP.md) - 开发路线图
- [里程碑执行记录](docs/MILESTONE_EXECUTION.md) - 分阶段验收与上下文压缩
- [课程演示指南](docs/DEMO_GUIDE.md) - 可视化与演示流程
- [目录结构说明](docs/PROJECT_STRUCTURE.md) - 目录职责、产物落位与清理建议

## 项目结构

```text
myCPU/
├── src/                 # Rust 核心实现（CPU/指令/内存/外设/可视化）
├── tests/               # 集成测试与测试程序
├── frontend/            # 可视化前端（Vite + React + TS）
├── scripts/             # 一键验收与构建脚本（Phase3~Phase6）
├── docs/                # 架构、路线图、里程碑、演示文档
│   └── presentations/   # 课程汇报与开题资料（已从根目录收拢）
├── plans/               # 施工蓝图与规划文档
├── artifacts/           # 外部工件（如 phase3 Linux 工件）
├── third_party/         # 外部源码依赖（xv6/buildroot 等）
├── target/              # Rust 构建输出（可清理）
└── tmp/                 # 临时文件目录（可清理）
```

> 说明：`target/`、`tmp/` 与脚本生成日志均属于运行产物；建议定期清理，避免污染工作区视图。

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
