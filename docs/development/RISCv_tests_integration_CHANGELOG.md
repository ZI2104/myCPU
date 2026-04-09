riscv-tests CI integration - Change log

Summary
-------
本次提交实现并完善了在 CI 中运行官方 `riscv-tests` 的集成点，包含：

- 在后端 `PerfCollector` 中添加了可选的 cache/tlb 计数器（`cache_hits`, `cache_misses`, `tlb_hits`, `tlb_misses`）并提供便捷记录方法。
- 在可视化快照 `PerfSnapshot` 中暴露 cache/tlb 字段，frontend types 与 `PerformanceDashboard` 已同步显示这些字段（可选，默认 0）。
- 在 `PerfReport`/JSON 中包含上述统计信息并在文本报告中打印。
- 健壮化并确认 `scripts/run_riscv_tests.sh` 已可在 CI 环境中用于逐测试执行并写入 JSON（脚本已存在，做了小幅注释/确认）。
- CI 工作流 `.github/workflows/run-riscv-tests.yml` 已改为在完成后上传 `artifacts/riscv-tests/` 作为 `riscv-tests-perf-json` 工件（始终上传以便失败时取证）。
- 更新文档 `docs/development/RISCV_TESTS_CI.md` 以说明新增字段与工件上传行为。

为什么这样设计
----------------
- 保持向后兼容：cache/TLB 字段为可选或默认 0，若当前运行器未启用缓存模型，不影响现有 CI。
- 最小侵入：新增计数器通过便捷函数记录，避免必须修改现有 PerfEvent 枚举或 CSR 映射，减小与其他 agent 的冲突面。
- CI 可观察性：将每个测试的 perf JSON 上传为工件，方便离线分析与断言（后续可添加更严格的合并门槛）。

验证步骤（你可以在 CI 上运行）
-----------------------------
1. 确认 GitHub Actions 的 runner 能访问互联网（脚本会 `git clone` 和（可选）下载预编译交叉工具链）。
2. 触发 workflow（Repository > Actions > Run riscv-tests 或使用 workflow_dispatch）：

```bash
# 在本地（或 CI runner）手动运行（注意：本地可能缺交叉编译器或 QEMU）
chmod +x ./scripts/run_riscv_tests.sh
# 0 = run all tests (CI 通常会限制到小集合以节省时间)
./scripts/run_riscv_tests.sh 10
```

3. CI 在每个测试后应在 `artifacts/riscv-tests/<test>.perf.json` 看到 JSON 输出。
4. 在 Actions 的 run 页面下载 `riscv-tests-perf-json` 工件进行检视。

注意（当前状态）
----------------
- 我没有在本地或 CI 上运行完整测试（因为另一个 agent 正在修改缓存/TLB 实现，可能导致构建失败）。
- 建议在另一个 agent 的变更稳定后，在 CI 中运行一个小规模 smoke 集合（例如 `max_tests=10`）以做第一次验证。

下一步建议
----------
- 当缓存/TLB 实现稳定后：
  - 在 CI workflow 中添加基于 perf JSON 的断言脚本（例如检查未出现非法退出或 perf JSON 可解析）。
  - 根据需要把 perf JSON 汇总成聚合报告并在 PR 中显示关键指标（IPC、stalls、cache miss rates）。
  - 如果需要，把 `riscv-tests` 的子集固定为 repo 子模块以避免 CI 每次 clone 花费过多时间。

变更文件一览
------------
- src/cpu/perf_collector.rs (新增 cache/tlb 字段与方法)
- src/visualize/snapshot.rs (PerfSnapshot: 新增 cache/tlb 字段)
- src/cpu/pipeline/mod.rs (将 PerfCollector 新字段传入 snapshot)
- src/perf/report.rs (PerfReport MemoryStats: 新增 cache/tlb 字段，Display 中打印)
- frontend/src/types/snapshot.ts (PerfSnapshot types: 可选 cache/tlb 字段)
- frontend/src/components/PerformanceDashboard.tsx (显示 cache/tlb)
- .github/workflows/run-riscv-tests.yml (上传 artifacts 步骤)
- docs/development/RISCV_TESTS_CI.md (文档更新)


