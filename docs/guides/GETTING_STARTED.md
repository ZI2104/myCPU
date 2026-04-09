# 快速上手（Getting Started）

此文档提供最常用的构建、运行与调试命令，适合新来贡献者或演示准备时参考。

1. 构建

```bash
# 构建发布二进制
cargo build --release

# 构建调试（默认）
cargo build
```

2. 运行与测试

```bash
# 运行模拟器（传入 ELF）
cargo run --release -- run program.elf

# 运行单元测试
cargo test

# 运行可选的 NPU ELF 集成测试（需显式开启）
# Unix/WSL:
RUN_ELF_INTEGRATION=1 MYCPU_NPU_DEBUG=1 cargo test --test npu_elf_integration -- --nocapture

# 运行 GPU/TPU 集成测试
cargo test --test gpu_tpu_integration
```

3. 调试与可视化

```bash
# 启动 GDB 调试服务器
cargo run --release -- debug program.elf

# 启动可视化（Demo 模式）
cargo run --release -- visualize

# Windows 一键帧缓冲演示
powershell -ExecutionPolicy Bypass -File .\scripts\run_framebuffer_demo.ps1
```

4. 可选 CI / 集成测试说明

- 项目提供了一个手动触发的 GitHub Actions 工作流：`.github/workflows/run-npu-elf-integration.yml`，用于按需在 CI 上运行 ELF 集成测试（需准备交叉工具链或自托管 runner）。
- 本地运行 ELF 集成测试需要交叉工具链或使用 `riscv64-unknown-elf-gcc` 驱动并传入 RV32 参数（参见 `docs/NPU_OVERVIEW.md`）。

5. 常见问题

- 如果缺少 riscv 交叉工具链，可用 `riscv64-unknown-elf-gcc -march=rv32i -mabi=ilp32` 进行替代编译。
- 若要在 CI 上稳定运行 ELF 集成测试，建议使用配置好的自托管 runner 并限制谁能触发该 workflow。

更多详细的演示指南请参见：`docs/guides/DEMO_GUIDE.md`。
