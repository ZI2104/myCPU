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

# Windows 深度瘦身（删除可重建缓存：target/buildroot output/node_modules）
powershell -ExecutionPolicy Bypass -File .\scripts\cleanup_workspace.ps1 -PruneBuildCaches
```

## 可选：运行 ELF 集成测试（本地）

项目包含一个依赖 guest ELF 的端到端集成测试 `tests/npu_elf_integration.rs`。出于可重现性和 CI 稳定性考虑，该测试默认是跳过的（需显式开启）。

运行步骤（大多数场景在 WSL/Unix 更方便）：

- 构建 tests 程序（脚本会优先使用汇编示例）：

```bash
cd tests/programs
./build.sh    # 生成 npu_vector_example.elf / npu_vector_example.bin
```

- 在项目根目录启用并运行集成测试：

PowerShell:
```powershell
# 在 PowerShell 中：
Set-Item -Path Env:RUN_ELF_INTEGRATION -Value 1
Set-Item -Path Env:MYCPU_NPU_DEBUG -Value 1    # 可选：启用 NPU 调试输出
cargo test --test npu_elf_integration -- --nocapture
```

Unix / WSL:
```bash
RUN_ELF_INTEGRATION=1 MYCPU_NPU_DEBUG=1 cargo test --test npu_elf_integration -- --nocapture
```

提示：如果本地没有 `riscv32-unknown-elf-*` 工具链，可以使用 `riscv64-unknown-elf-gcc` 驱动并传入 RV32 编译选项：

```bash
riscv64-unknown-elf-gcc -march=rv32i -mabi=ilp32 -nostdlib -T link.ld npu_vector_example.s -o npu_vector_example.elf
```

测试默认关闭可以避免在没有交叉工具链或构建工件时导致 CI 失败；若需要在 CI 上运行该测试，建议创建单独的手动触发（workflow_dispatch）job 并准备相应的交叉工具链镜像。

## 在 CI（GitHub Actions）上运行（可选 workflow）

本仓库包含一个示例的手动触发 GitHub Actions workflow：`.github/workflows/run-npu-elf-integration.yml`，用于按需在 CI 上构建 guest ELF 并运行 `tests/npu_elf_integration`。该 workflow 旨在由维护者或有权限的人员通过 Actions 页面手动触发。

快速使用：在 GitHub 页面选择 Actions -> run-npu-elf-integration -> Run workflow，或使用 gh CLI：

```bash
gh workflow run run-npu-elf-integration.yml -f run_integration=true
```

重要说明与注意事项：
- workflow 默认尝试在 GitHub-hosted runner 上作“best-effort”安装 riscv 工具链，可靠性受限于 runner 环境；推荐在受控环境使用自托管 runner（例如带标签 `riscv` 的机器）以确保工具链可用与加速构建。
- 自托管 runner 优点：可预装交叉编译工具链、缓存构建产物、访问内部镜像或私有资源；缺点：需维护、存在安全风险（请不要让外部未审查的 PR 自动在有 secrets 或内网访问的 self-hosted runner 上运行）。
- 若采用自托管 runner，请在 workflow 中将 `runs-on` 改为：

```yaml
runs-on: [self-hosted, linux, riscv]
```

- 安全实践：限制谁可以 dispatch workflow（仓库设置）、避免在未审查的 fork PR 上自动运行该 job、以容器或短生命周期 VM 方式运行 runner 以减小持久风险。

本流程为可选，课设日常开发通常不需要常驻使用；当需要在 CI 上复现 ELF 集成情形时，手动触发该 workflow 即可。

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
