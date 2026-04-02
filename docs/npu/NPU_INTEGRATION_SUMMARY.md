# NPU 集成调试总结（NPU Integration Debug Summary）

> 日期：2026-04-02
>
> 目标：记录 NPU 向量模式集成时的关键问题、修复路径与可复现实验步骤。

## 结论摘要

- 工具链建议：优先使用 `riscv64-unknown-elf-gcc`，并显式指定 RV32 参数。
- 集成测试策略：`tests/npu_elf_integration.rs` 保持可选，需通过环境变量显式启用。
- 调试输出策略：默认安静，按需通过 `MYCPU_NPU_DEBUG=1` 开启详细日志。

## 工具链策略

- 使用 `riscv64-unknown-elf-gcc` 驱动配合 RV32 参数构建 RV32 ELF。
- 典型参数：`-march=rv32i -mabi=ilp32 -nostdlib -T link.ld`。
- 原因：可避免汇编器/链接器 ABI 不一致问题，也暂不强依赖独立的 `riscv32-unknown-elf-*` 工具链安装。

## 遇到的问题与根因

### 现象

- 初始集成测试失败：ELF 执行阶段出现 instruction page fault，PC 回落到 `0`。

### 根因

1. C 版本 guest 示例引入了 C runtime / 链接差异，增加了工具链组合复杂度。
2. 若 `riscv64` 驱动未显式指定 `march/mabi` 或混用前缀工具，可能触发 ELF/ABI 不匹配。

## 修复与缓解措施

1. 新增最小汇编 guest：`tests/programs/npu_vector_example.s`，直接写 descriptor 并触发 notify。
2. 更新 `tests/programs/build.sh`：优先编译 `.s`，无 `.s` 时再回退到 C 编译流程。
3. 将 `tests/npu_elf_integration.rs` 的调试输出改为环境变量控制（`MYCPU_NPU_DEBUG`）。

## 复现与验收

> 注意：该测试为可选测试，默认跳过。运行前必须设置 `RUN_ELF_INTEGRATION=1`。

### WSL / Unix

```bash
cd /mnt/d/code/myCPU/tests/programs
riscv64-unknown-elf-gcc -march=rv32i -mabi=ilp32 -nostdlib -T link.ld npu_vector_example.s -o npu_vector_example.elf
riscv64-unknown-elf-objcopy -O binary npu_vector_example.elf npu_vector_example.bin

cd /mnt/d/code/myCPU
RUN_ELF_INTEGRATION=1 MYCPU_NPU_DEBUG=1 cargo test --test npu_elf_integration -- --nocapture
```

### Windows PowerShell

```powershell
Set-Item -Path Env:RUN_ELF_INTEGRATION -Value 1
Set-Item -Path Env:MYCPU_NPU_DEBUG -Value 1
cargo test --test npu_elf_integration -- --nocapture
```

## 成功日志样例

```text
[NPU] REG_DESC_ADDR_LOW <- 0x00000090
[NPU] REG_DESC_ADDR_LOW <- 0x80000090
[NPU] REG_DESC_NOTIFY <- 1 (pending_desc_notify set)
```

- 验收退出示例：`test test_npu_elf_end_to_end ... ok`

## CI 与后续建议

- 该集成测试建议保持手动触发或受控环境运行，避免影响常规 CI 稳定性。
- 后续若引入 RV64 全链路支持，可补充 `riscv32` / `riscv64` 双工具链矩阵，并统一 `build.sh` 的自动探测策略。
- 如需编译期控制日志，可评估将 `MYCPU_NPU_DEBUG` 迁移为 feature flag。

## 相关文档

- `docs/npu/NPU_OVERVIEW.md`（总览入口）
- `docs/npu/NPU_SPEC.md`（规范定义）
