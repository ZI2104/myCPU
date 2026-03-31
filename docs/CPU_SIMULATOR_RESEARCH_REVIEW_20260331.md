# myCPU CPU 模拟器调研、评估与改进建议（2026-03-31）

## 1. 任务范围与方法

本次工作覆盖以下内容：

- 对标项目调研：NEMU、QEMU、Spike（riscv-isa-sim）
- 当前仓库实现盘点（架构、模块、功能）
- 代码审查（可维护性、正确性、边界行为、工程化）
- 构建与测试验证
- 改进建议与优先级路线图
- 任务内自由优化（已实施）

---

## 2. 对标调研结论（NEMU / QEMU / Spike）

### 2.1 NEMU（教学导向全系统模拟器）

调研要点：

- 多 ISA 支持（x86/mips32/riscv32/riscv64）
- 内置 monitor + 简易调试器（单步、寄存器/内存查看、表达式、watchpoint）
- 差分测试（与参考设计如 QEMU 对比）
- snapshot 能力
- 基础设备模型（串口、计时器、键盘、VGA、音频）

对 myCPU 的参考价值：

1. “可验证性优先”工程路线（DiffTest + 调试器 + 快照）
2. 教学项目中，工具链完整性与 ISA 覆盖率同等重要

### 2.2 QEMU（工业级基准）

调研要点：

- 文档体系完备（system/user/interop/devel）
- 监控与管理接口成熟（Monitor/QMP/GDB stub）
- 测试与互操作能力强（协议、设备、镜像、record/replay）

对 myCPU 的参考价值：

1. 把调试接口从“可用”推进到“可集成”（GDB/RSP 完整性）
2. 把性能统计从“展示”推进到“可归因”（事件级拆解）

### 2.3 Spike（ISA 语义黄金参考）

调研要点：

- 功能模型严谨，ISA/特权级支持丰富
- 交互式调试与 GDB/OpenOCD 流程清晰
- 明确 API 与版本边界

对 myCPU 的参考价值：

1. 强化“语义正确性”回归路径（分层对比、失败复盘）
2. 提升调试路径的标准兼容性（更贴近工具链）

---

## 3. 当前项目实现评估

### 3.1 综合评价

总体处于“课程项目中的高完成度阶段”：

- RV32I、5 级流水线、CSR/异常中断、UART、ELF、性能统计均已具备
- 单元测试覆盖较广（`cargo test --lib` 共 185 项）
- 已形成可视化与 DiffTest 框架

### 3.2 功能完成度（基于代码与文档交叉）

| 能力域               | 现状                 | 评估                          |
| -------------------- | -------------------- | ----------------------------- |
| RV32I 指令执行       | 已实现               | 稳定，测试覆盖较完整          |
| 5 级流水线与冒险处理 | 已实现               | 结构清晰，前递/暂停/冲刷可见  |
| 特权级与 CSR         | 已实现（M/S/U 框架） | 可用，后续可加强边界一致性    |
| CLINT/PLIC 中断      | 已实现               | 功能具备，细节语义仍可收敛    |
| UART 与总线系统      | 已实现               | 基础良好                      |
| ELF 加载             | 已实现               | 本次已加固安全性（见第 6 节） |
| GDB 调试             | 框架已实现           | 关键命令仍偏占位实现          |
| DiffTest             | 框架已实现           | 可比较状态，仍需增强实战流程  |
| 可视化               | 已实现               | 功能丰富，局部性能点可优化    |
| 性能报告             | 已实现               | 展示完备，归因深度可继续提升  |

---

## 4. 代码 Review 发现（含优先级）

### 4.1 高优先级

1. **ELF Loader 存在不安全生命周期转换（已修复）**  
   - 位置：`src/loader/mod.rs`  
   - 原问题：`unsafe transmute::<Elf<'_>, Elf<'static>>`  
   - 风险：潜在生命周期不一致与维护风险  
   - 状态：**已在本次任务中移除 unsafe**，改为“解析后缓存元数据 + 段信息”

2. **GDB RSP 的核心读写命令仍偏占位实现**  
   - 位置：`src/debug/mod.rs`  
   - 现象：`cmd_read_registers` 返回全 0 占位，`cmd_read_memory` 返回占位数据，尚未真实连接 CPU/内存状态  
   - 影响：与 VSCode/GDB 的真实联调能力受限  
   - 建议：把 `GdbServer` 与 `DebugSession`/CPU 实例打通，优先完善 `g/m/s/c/Z0/z0/qSupported`

3. **ELF 入口点未驱动实际启动 PC**  
   - 位置：`src/main.rs` (`load_file` / `run_program`)  
   - 现象：ELF 解析后会打印 entry，但 `Cpu::with_pc` 使用的是命令行 `--pc` 值（默认 `0x80000000`）  
   - 影响：当 ELF entry 与默认 PC 不一致时可能运行错误  
   - 建议：`load_file` 返回 entry 并在 ELF 路径下覆盖启动 PC

### 4.2 中优先级

1. **CLINT 非对齐寄存器读语义过于宽松**  
   - 位置：`src/interrupt/clint.rs`  
   - 现象：部分非对齐读取返回 `0`  
   - 建议：统一返回对齐/地址异常（与系统其他内存访问语义一致）

2. **可视化历史队列头删为 O(n)**  
   - 位置：`src/visualize/server.rs` (`history.remove(0)`)  
   - 影响：长时间运行时历史维护有额外开销  
   - 建议：改为 `VecDeque` 环形缓冲

### 4.3 低优先级

1. **Bus 地址路由当前为线性扫描**  
   - 位置：`src/memory/bus.rs`  
   - 说明：当前规模可接受；当设备数量增多可考虑区间索引

2. **警告与文档 lint 噪音较多**  
   - 说明：不阻断功能，但影响长期维护体验

---

## 5. 构建与测试结论

### 5.1 本次执行结果

- 构建：`cargo build` ✅
- 测试：`cargo test --lib` ✅
- 结果：**185 passed; 0 failed**

### 5.2 观察

- 本轮优化前后，测试均保持 185/185，说明改动未破坏既有行为
- 已清理 `src/memory/rom.rs` 中一个未使用导入告警

---

## 6. 本次已实施优化（自由发挥）

### 6.1 安全性优化：移除 `ElfLoader` 中的 unsafe

已修改文件：`src/loader/mod.rs`

改动概要：

- 移除 `Elf<'static>` 存储与 `unsafe transmute`
- 改为存储：
  - `ElfHeaderInfo`（头信息缓存）
  - `LoadableSegment`（段元数据 + 文件偏移）
  - 原始 `bytes`
- 在解析阶段加入段边界检查：
  - `offset + filesz` 溢出检查
  - `end <= bytes.len()` 越界检查

收益：

- 降低生命周期相关风险
- 增强 ELF 输入鲁棒性
- 保持现有外部 API 与测试行为

### 6.2 代码洁净度优化

已修改文件：`src/memory/rom.rs`

- 删除测试模块中未使用的 `Word` 导入，消除对应告警

---

## 7. 改进路线建议（可执行）

### P0（1-2 周，优先）

1. **修复 ELF entry 启动语义**（高优先级正确性）
2. **补齐 GDB RSP 关键命令的真实数据路径**（可调试性）
3. **CLINT 非对齐访问语义收敛**（一致性）

### P1（2-4 周，能力增强）

1. **DiffTest 增强**：失败时自动输出最近 N 条指令、关键 CSR 对比
2. **可视化历史容器优化**：`Vec` -> `VecDeque`
3. **性能报告增强**：分离结构性停顿占比（load-use/branch/memory）

### P2（中期）

1. **M 扩展 / C 扩展** 的增量引入
2. **riscv-tests 与真实程序回归集** 常态化
3. **文档与 lint 规则统一治理**（降低长期维护成本）

---

## 8. 阶段性“上下文压缩”记录

- **压缩 #1（外部调研后）**：确认对标核心能力 = *difftest + debug + snapshot + 文档化测试体系*。  
- **压缩 #2（仓库盘点后）**：myCPU 已具备“课程级完整链路”，短板集中在 *调试协议完整性* 与 *边界语义一致性*。  
- **压缩 #3（review + 测试后）**：质量现状稳定（185/185），主要风险来自 *高优先级工程细节* 而非基础功能缺失。  
- **压缩 #4（优化落地后）**：已完成一项安全加固（移除 loader unsafe），系统稳定性与可维护性提升，下一步建议攻克 GDB/ELF entry 语义问题。

---

## 9. 参考链接

- NEMU: https://github.com/NJU-ProjectN/nemu  
- NEMU README: https://raw.githubusercontent.com/NJU-ProjectN/nemu/master/README.md  
- Abstract Machine: https://github.com/NJU-ProjectN/abstract-machine  
- QEMU: https://github.com/qemu/qemu  
- QEMU Docs: https://www.qemu.org/docs/master/  
- Spike: https://github.com/riscv-software-src/riscv-isa-sim

---

## 10. 第二轮改进落地进展（实现“改进路线建议”）

> 说明：本节对应“实现所有改进路线建议 + 完善前端功能 + 测试验证后记录文档”的执行进度。

### 10.1 P0 路线落地

1. **ELF entry 启动语义已修复**（`src/main.rs`）

- `load_file` 改为返回 `Option<Addr>`。
- ELF 路径下统一使用 ELF entry 覆盖启动 PC；raw binary 仍使用 CLI 传入地址。
- `run/debug/visualize` 三条路径行为已对齐。

2. **GDB RSP 关键命令接入真实 CPU/内存路径**（`src/debug/mod.rs`）

- `GdbServer` 新增 `with_cpu` 构造，内部持有真实 `Cpu`。
- `g/G/m/M/p/P/s/c` 命令由占位实现切换为真实寄存器、内存读写与执行路径。
- 新增回归测试：
   - `test_gdb_read_write_registers`
   - `test_gdb_read_write_memory`

3. **CLINT 非对齐访问语义收敛**（`src/interrupt/clint.rs`）

- 对非对齐 word 访问返回对齐错误，不再返回宽松默认值。
- 新增测试：`test_clint_unaligned_word_access_errors`。

### 10.2 P1 路线落地

1. **DiffTest 失败上下文增强**（`src/difftest/mod.rs`）

- 错误结构增加 `recent_history`。
- mismatch 报告增加 PC/寄存器/特权级/计数器差异摘要，便于快速定位。

2. **可视化历史容器性能优化**（`src/visualize/server.rs`）

- 历史结构由 `Vec` 改为 `VecDeque`。
- 头部淘汰由 O(n) 改为 O(1)。

3. **性能报告停顿分解增强（后端 + 前端）**

- 后端：`src/perf_report.rs`, `src/visualize/snapshot.rs`, `src/cpu/pipeline/mod.rs`
   - 新增 load-use / control 的 cycle rate 与 stall share 指标。
- 前端：`frontend/src/types/snapshot.ts`, `frontend/src/components/PerformanceDashboard.tsx`
   - 新增指标字段与可视化展示。

### 10.3 P2 路线增量推进

1. **RV32M 扩展已完成首批落地**

- `src/instruction/opcode.rs`：新增 `funct7::M_EXT` 常量。
- `src/instruction/execute.rs`：R-type 路径支持：
   - `MUL/MULH/MULHSU/MULHU`
   - `DIV/DIVU/REM/REMU`
   - 覆盖除零与溢出边界语义。
- 新增测试：
   - `test_rv32m_mul_div_rem`
   - `test_rv32m_div_by_zero_behavior`
- `src/visualize/snapshot.rs`：反汇编新增 RV32M 指令显示（mul/div/rem 全族）。

2. **C 扩展已完成增量起步（最小可运行支撑）**

- `src/cpu/core.rs`：取指改为 halfword-aware，可在 C/非 C 混合流中正确拼装 32 位指令。
- 新增压缩指令入口：
   - 已支持 `C.NOP`（PC 正确 +2）。
   - 其他压缩指令返回清晰 `UnsupportedInstruction`，便于后续按优先级扩展。
- 新增测试：
   - `test_compressed_nop_progresses_pc_by_2`
   - `test_unimplemented_compressed_instruction_reports_clear_error`

3. **文档/规范治理增量**

- 新增 `.markdownlint.json`，统一文档 lint 基线，降低仓库内文档规则噪音。

### 10.4 前端功能完善进展

1. **新增调试面板**（`frontend/src/components/DebugInspector.tsx`）

- 支持断点增删查。
- 支持按地址/长度请求反汇编。
- 支持按 offset/limit 请求历史，并展示执行轨迹。

2. **应用主界面接入 Debug 视图**（`frontend/src/App.tsx`, `frontend/src/App.css`）

- 新增 `debug` tab。
- WebSocket 消息分发新增：`breakpoints/disassembly/history`。
- 调试命令与 UI 操作链路贯通。

3. **控制面板可选 Unlimited 速度模式**（`frontend/src/components/ControlPanel.tsx`）

- 速度滑块支持 `0`（Unlimited）。

### 10.5 回归验证结果

- Rust 构建：`cargo build` ✅
- Rust 测试：`cargo test --lib` ✅（**193 passed; 0 failed**）
- 前端构建：`npm run build`（`frontend/`）✅

结论：第二轮增量改造与前端增强已通过本地构建与测试验证，未引入新增编译/类型错误。

---

## 11. 第二轮“上下文压缩”记录

- **压缩 #5（P0 启动语义）**：ELF entry 已从“仅展示”升级为“驱动实际启动 PC”，run/debug/visualize 入口行为一致。  
- **压缩 #6（GDB 通路）**：RSP 关键命令已接入真实 CPU/内存，不再是占位数据；调试可用性显著提升。  
- **压缩 #7（一致性收敛）**：CLINT 非对齐访问语义与系统内存对齐策略统一，异常边界更可预期。  
- **压缩 #8（可观测与性能）**：DiffTest 失败信息含最近执行上下文；可视化历史结构改为 `VecDeque` 降低运行开销。  
- **压缩 #9（前后端联动）**：stall 分解指标已从 pipeline 统计贯通到前端展示，性能瓶颈定位更直接。  
- **压缩 #10（前端调试增强）**：新增 Debug Inspector（断点/反汇编/历史），并完成主界面集成与打包验证。  
- **压缩 #11（P2 增量）**：RV32M 第一批指令（mul/div/rem）已落地并覆盖关键边界测试，进入可持续扩展状态。  
- **压缩 #12（C 扩展起步）**：完成 halfword-aware 取指与 `C.NOP` 最小支持，建立 C 扩展渐进实现的稳定落点。
