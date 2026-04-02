# NPU 概览（NPU Overview）

> 更新日期：2026-04-03
>
> 角色定位：本文件是 NPU 文档入口（快速查阅）；规范细节和调试复盘请跳转到下方链接文档。

## 文档分工（建议阅读顺序）

1. `docs/npu/NPU_OVERVIEW.md`（本文件）
   - 快速了解 ABI、测试入口、CI 策略与 Hybrid Offload 方向。
2. `docs/npu/NPU_SPEC.md`
   - 16B descriptor ABI、寄存器语义、错误语义、算子与软异步行为定义。
3. `docs/npu/NPU_INTEGRATION_SUMMARY.md`
   - 集成问题复盘、根因、修复措施与复现步骤。

## 快速事实（Quick Facts）

- Descriptor ABI：与 `NPU_SPEC v1` 保持兼容，每个 descriptor 固定 16 字节。
- 对齐约束：关键地址按 4 字节对齐，否则会触发对齐错误并计入 `REG_TASKS_ERROR`。
- 集成测试：`tests/npu_elf_integration.rs` 为可选测试，默认跳过。
- 启用条件：需显式设置 `RUN_ELF_INTEGRATION=1`。
- 调试开关：可选设置 `MYCPU_NPU_DEBUG=1` 打印调试日志。

## 本地运行（可选集成测试）

### Unix / WSL

```bash
RUN_ELF_INTEGRATION=1 MYCPU_NPU_DEBUG=1 cargo test --test npu_elf_integration -- --nocapture
```

### Windows PowerShell

```powershell
Set-Item -Path Env:RUN_ELF_INTEGRATION -Value 1
Set-Item -Path Env:MYCPU_NPU_DEBUG -Value 1
cargo test --test npu_elf_integration -- --nocapture
```

## Hybrid Offload 摘要

- 调度策略：根据算子类型、数据规模、对齐/布局、延迟预算在 CPU 与 NPU 之间选择执行路径。
- 兼容原则：保持默认同步路径语义不变；异步路径通过新增标志位渐进启用。
- 错误与回退：NPU 对对齐/越界错误通过状态寄存器或 IRQ 上报；上层需支持 CPU fallback。

## CI / Workflow 约定

- NPU ELF 集成测试通过手动工作流触发：`.github/workflows/run-npu-elf-integration.yml`。
- 该测试保持"受控执行"策略，避免对常规 CI 稳定性造成干扰。

## 相关文档

- `docs/npu/NPU_SPEC.md`（规范定义）
- `docs/npu/NPU_INTEGRATION_SUMMARY.md`（集成复盘）
