# riscv-tests CI integration

本文档说明当前仓库如何将官方 `riscv-tests` ISA 套件接入 CI，并通过 DiffTest（myCPU vs QEMU）进行行为对比。

## 核心组件

- `scripts/run_riscv_tests.sh`
	- 克隆并构建 `riscv-tests`（目录：`third_party/riscv-tests/`）；
	- 逐测试启动 QEMU GDB stub（`-S -s`）；
	- 调用 myCPU 的 DiffTest 路径执行比对；
	- 为每个测试写出 `artifacts/riscv-tests/<test>.perf.json`。

- `.github/workflows/run-riscv-tests.yml`
	- 使用 `dtolnay/rust-toolchain@stable` 安装 Rust；
	- 通过 apt 安装 `qemu-system-riscv` 与 `gcc-riscv64-unknown-elf`；
	- `workflow_dispatch` 支持 `max_tests` 控制测试规模；
	- 结束后始终上传 `artifacts/riscv-tests/`。

- 性能 JSON 导出
	- `mycpu` 在设置 `MYCPU_PERF_JSON` 时写出 `PerfReport` JSON；
	- CI 脚本为每个测试设置独立输出路径，便于后续聚合分析。

## cache/TLB 指标联动

`PerfCollector` / `PerfReport` / 可视化快照已支持以下字段：

- `cache_hits`
- `cache_misses`
- `tlb_hits`
- `tlb_misses`

说明：若当前运行模式未启用对应模型，字段可保持 0，不影响 CI 兼容性。

## CI 执行流程（简版）

1. 安装 QEMU 与 RISC-V 交叉工具链（apt）。
2. 构建 myCPU（release）。
3. 执行 `scripts/run_riscv_tests.sh $max_tests`。
4. 每个测试生成 perf JSON 到 `artifacts/riscv-tests/`。
5. `actions/upload-artifact@v4` 以 `riscv-tests-perf-json` 名称上传产物（`if: always()`）。

## 使用建议

- PR 场景建议用 `max_tests=10` 做快速回归。
- 定时任务或发布前可将 `max_tests` 设为 `0` 跑全量。
- 如需做趋势分析，可对 `artifacts/riscv-tests/*.perf.json` 做二次聚合并生成摘要报告。
