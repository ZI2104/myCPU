# myCPU Agent 入口

> 目的：给 Agent 一个**稳定、小巧、可执行**的起点；细节按需拉取，不在本文件堆上下文。

## 1) 项目一句话

`myCPU` 是 Rust 实现的 RISC-V RV32I 模拟器，核心能力包含：伪 6 级流水线、M/S/U 特权、MMU/TLB、外设、GDB、DiffTest 与性能监控。

## 2) 先做什么（固定流程）

1. 明确任务类型：`bug` / `feature` / `refactor` / `docs`。
2. 只读取**最小必要文件**（见下方“按需拉取地图”）。
3. 先改最小闭环，再运行对应验证。
4. 输出“改了什么 + 如何验证 + 风险点/后续”。

## 3) 按需拉取地图（缺什么读什么）

### A. 架构/路线

- 总览：`README.md`
- 架构设计：`docs/design/ARCHITECTURE.md`
- 路线与阶段：`docs/design/ROADMAP.md`

### B. CPU 执行与流水线

- 入口：`src/cpu/core.rs`
- 流水线主控：`src/cpu/pipeline/mod.rs`
- 阶段实现：`src/cpu/pipeline/stages/`
- 指令译码执行：`src/instruction/`

### C. 内存 / MMU / Cache / 总线

- 总线与内存：`src/memory/bus.rs` `src/memory/ram.rs` `src/memory/rom.rs`
- MMU/TLB：`src/cpu/mmu.rs`
- Cache：`src/cpu/cache.rs`

### D. 特权 / 异常 / 中断

- CSR：`src/cpu/csr/`
- 异常：`src/cpu/exception/`
- 中断控制器：`src/interrupt/clint.rs` `src/interrupt/plic.rs`

### E. 调试 / 验证 / 可视化

- GDB：`src/debug/`
- DiffTest：`src/difftest/`
- 性能采集：`src/cpu/perf_collector.rs` `src/perf/`
- 可视化后端：`src/visualize/`
- 前端：`frontend/src/`

## 4) 必守约束（高优先级）

- 不硬编码指令魔数；优先复用已定义常量。
- 访存接口保持类型一致（`Word/Byte/...` 包装类型）。
- 多字节访问必须做对齐检查。
- 流水线访存遵循 latch 时序（指令与数据读延迟语义不能破坏）。
- 错误使用 `Result`/`SimError`，不要静默吞错。

## 5) 最小验证矩阵（改完必跑）

- Rust 库测试：`cargo test --lib`
- 构建检查：`cargo build`
- 涉及前端时：在 `frontend/` 下执行 `npm run build`

> 若变更触及流水线/MMU/中断/调试协议，补充对应模块测试或回归用例。

## 6) Agent 拉取策略（“教”你怎么取上下文）

- **默认不要全量读仓库**；先读入口文件，再沿调用链下钻。
- 每次只回答一个问题：
   - “我要改哪里？”
   - “这段逻辑依赖谁？”
   - “改完如何证伪？”
- 当出现以下信号再扩展读取范围：
   - 行为跨模块（如 pipeline + mmu + bus）
   - 测试失败指向其他子系统
   - 接口契约不明确

## 7) 常用任务 → 入口文件

- 改指令行为：`src/instruction/execute.rs` + `src/cpu/core.rs`
- 查流水线停顿/前递：`src/cpu/pipeline/` + `src/cpu/pipeline/registers.rs`
- 查 TLB 命中/旁路：`src/cpu/mmu.rs` + `src/cpu/pipeline/mod.rs`
- 查 Cache 策略与统计：`src/cpu/cache.rs` + `src/cpu/perf_collector.rs`
- 改可视化指标：`src/visualize/snapshot.rs` + `frontend/src/types/snapshot.ts`

## 8) 完成定义（DoD）

- 改动最小且聚焦。
- 相关测试/构建通过。
- 文档或注释已同步（若接口/行为变化）。
- 交付说明包含：变更点、验证结果、已知限制。
