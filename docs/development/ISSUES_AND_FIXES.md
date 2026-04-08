# 问题与修复记录（Issues & Fixes）

本文档用于集中记录工程中遇到的关键问题、定位过程、最终修复方案与回归验证要点，目标是为后续排查提供可复用的知识库。

目录
- 可视化（visualize）
- 构建与验收（build/acceptance）
- 指令/译码（instruction/decoder/execute）
- CSR/中断/特权（csr/interrupt）
- 存储/内存（memory/bus/ram）

使用说明（模板）
每个条目尽量包含下面字段：

- 标题：简短说明问题
- 现象：可复现的外观察（日志、前端截图、错误消息）
- 根因分析：定位到的代码/设计缺陷
- 修复方案：具体修改点（文件/函数），必要时附上补丁摘要
- 验证：如何回归验证（命令、测试、所需工件）
- 状态：已修复 / 部分修复 / 未修复
- 引用：相关 issue/PR/文件路径/测试用例

---------------------------------------------------------------------------

## 可视化（visualize）

### Reset 与 run_loop 的竞态导致 Reset 后 IF-stage PC 显示不正确

- 标题：Reset 后 IF-stage PC 显示已 advance（示例：0x001C 而非 0x0018），并且 perf.cycles 列不重置；未映射内存时 run_loop 报错并持续广播快照导致界面卡住。
- 现象：前端在 Reset 后可能显示错误的 IF 阶段地址；周期列（Cn）不从 C0 开始；若内存未映射，后端在 `run_loop` 中持续返回 MemoryOutOfBounds 并广播错误快照，UI 无法恢复。
- 根因：后端 `run_loop` 与外部命令（如 `reset`）存在竞态；Reset 路径广播的 snapshot 可能包含已 advance 的 IF 状态；低地址访问未映射直接错误导致 busy-loop。
- 修复方案：
  - 在 `src/visualize/server.rs` 内使用 `clock_lock` 序列化对 `cpu.clock()` 与命令处理的访问，避免并发。
  - Reset 时构造并广播 `modified_snapshot`：强制把 `pc` 与 `pipeline.if_stage.pc` 设为记录的 `initial_pc`，并把 `perf.cycles` / `instructions` 等计数器清零，以便前端列从 C0 开始并立即看到预期 IF 状态。
  - 在 `run_loop` 的 `cpu.clock()` 周围增加错误处理：对可自动修复的低地址 `MemoryOutOfBounds`，自动附加一段 NOP-filled RAM（demo 模式保护）；对不可恢复的错误，halt CPU 并广播最终 snapshot，避免 busy-loop。
  - 前端保留按 `perf.cycles` 去重/合并历史的策略，确保后端发布的 authoritative snapshot 被优先使用。
- 验证：
  - 启动可视化服务：
    ```bash
    cargo run -- visualize -w 8080 --pc 0x18
    ```
  - 打开前端，执行 Reset → Run，观察 Reset 后的第一帧 snapshot（检查 `pc`、`pipeline.if_stage.pc`、`perf.cycles` 是否为 expected initial values）。
  - 如果出现问题，收集后端日志（包含 `[visualize]` 与 `[visualize::run_loop]` 行）以及前端收到的 Reset 前后一条 snapshot JSON。
- 状态：已修复（见 commit），参考文件：`src/visualize/server.rs`，docs 参考：`docs/design/VISUALIZATION_DESIGN.md`（已加入摘要）。

---------------------------------------------------------------------------

（后续请在本文件中按上述模板补充条目，便于按问题域检索）

---------------------------------------------------------------------------

## 构建与验收（build/acceptance）

### Buildroot 工件构建链路不稳定（WSL/路径/引号）

- 标题：Buildroot 工件复制/定位不稳定导致验收流程断裂
- 现象：构建脚本在 WSL 输出目录解析、命令拼接与引号处理上反复失败，导致 `images` 目录无法稳定定位并阻断后续验收脚本。
- 根因：PowerShell 与 Bash 混合执行时参数展开与引号语义冲突，脚本对 WSL 返回值解析不够健壮。
- 修复方案：在 `scripts/build_phase3_buildroot_artifacts.ps1` 中引入统一的 WSL 脚本执行/捕获封装，并用脚本片段方式解析输出目录，避免直接拼接复杂命令与引号。
- 验证：运行构建脚本并确认 `artifacts/phase3` 下三件套（Image、rootfs.ext2、virt-qemu.dtb）能被稳定生成或复制。
- 状态：已修复（见 `scripts/build_phase3_buildroot_artifacts.ps1` 改动）。

---

### OpenSBI 启动早期崩溃（RV32C 跳转错误）

- 标题：RV32C 条目导致 OpenSBI 启动时异常跳转
- 现象：启动阶段出现异常跳转/越界导致早期崩溃。
- 根因：压缩指令 `C.J/C.JAL` 的立即数位映射实现错误，跳转目标计算不正确。
- 修复方案：修正 `decode_c_imm_j` 的位映射实现并添加单元测试覆盖。
- 验证：对相关 C 指令单测通过，OpenSBI 能正常继续启动至下一阶段。
- 状态：已修复（参见 `src/instruction/decoder.rs` 测试与修复提交）。

---

### 原子指令缺失导致 Linux/SBI 崩溃（AMO 覆盖不足）

- 标题：缺少 AMO 指令实现导致在运行操作系统或 SBI 时失败
- 现象：日志出现 `Unsupported AMO funct5=...`，导致启动失败。
- 根因：RV32A 指令集实现不完整，只实现了部分 AMO/LRSC 指令。
- 修复方案：在 `src/instruction/execute.rs` 的 `execute_amo` 中补齐 `lr.w/sc.w`、`amoadd.w/amoswap.w`、位逻辑与最小/最大等族指令，并添加相应单测。
- 验证：新增 AMO 单元测试通过；运行 OpenSBI 与用户态镜像时不再因 AMO 缺失崩溃。
- 状态：已修复。

---

### fw_jump 装载地址错配导致后续越界

- 标题：验收脚本默认装载地址与 payload 约定不一致
- 现象：OpenSBI Banner 虽正常显示，但后续出现越界/异常导致无法继续。
- 根因：验收脚本默认的 `PayloadAddr/DtbAddr` 与 `fw_jump` 的 Next Address/Arg1 约定不一致。
- 修复方案：将 `scripts/run_linux_phase3_acceptance.ps1` 的默认 `PayloadAddr` 与 `DtbAddr` 调整为 `0x80400000` / `0x82200000`；并在构建脚本中优先选择真实 DTB（`-AutoResolveArtifacts -StrictUserlandMarker`）。
- 验证：使用更新的脚本能稳定启动至 S-mode 并继续后续验证步骤。
- 状态：已修复（脚本已更新）。

---------------------------------------------------------------------------

## 指令/译码（instruction/decoder/execute）

（见上方关于 RV32C 与 AMO 的条目；后续如有新的指令实现问题请在此分类补充）

---------------------------------------------------------------------------

## CSR/中断/特权（csr/interrupt）

### CSR 兼容缺失导致启动期间 Invalid CSR 报错

- 标题：启动过程中出现 `Invalid CSR address` 报错
- 现象：系统在早期引导频繁遇到无法识别的 CSR 访问，影响后续启动路径。
- 根因：未实现部分可选的机器/特权 CSR（PMP、计数器别名等），以及对未知机器 CSR 无兼容存储。
- 修复方案：在 `src/cpu/csr/mod.rs` 中增加 PMP CSR 存根、`MCOUNTEREN`、机器态未知 CSR 的兼容 Map 存储；实现用户计数器别名（`cycle/time/instret` 及其高位、`hpmcounter3..31` 等）。并补充单测验证读写与别名行为。
- 验证：对应单测通过，启动日志中不再出现相关 `Invalid CSR address` 报错。
- 状态：已修复。

---

### S 态中断注入路径不完整（SIP/MIP 可写位）

- 标题：Supervisor pending interrupt 注入链路不完整导致系统停滞
- 现象：OpenSBI 切换到 S-mode 后长时间无用户态推进，怀疑 pending interrupt 未正确注入。
- 根因：`MIP/SIP` 写入覆盖与 `sync_interrupts()` 的映射逻辑不完整，导致 `mip` 的可委托 pending 位未映射至 `sip`。
- 修复方案：扩展 `src/cpu/csr/machine.rs`（`Mip::write()` 支持 `SSIP/STIP/SEIP`）、在 `src/cpu/csr/mod.rs` 中允许机器态写 `SIP` 的这些位；在 `src/cpu/core.rs` 的 `sync_interrupts()` 中将 `mip` 的 `SSIP/STIP/SEIP` 反映到 `sip`。
- 验证：中断同步测试与严格验收流程中观察到 `sip`/`mip` pending 位的正确传递。
- 状态：已修复（但 strict 验收仍有其他阻塞）。

---------------------------------------------------------------------------

## 存储/内存（memory/bus/ram）

### 运行期越界导致进程直接退出（未归一化为页故障）

- 标题：MemoryOutOfBounds 等低级错误直接导致模拟器退出
- 现象：数据访问越界直接终止模拟器，无法进入 trap 处理路径。
- 根因：越界错误在 fetch/load/store 路径被直接上抛为致命错误，未统一为页故障由内核处理。
- 修复方案：在 `src/cpu/core.rs` 中对 fetch/load/store 路径做错误归一化：将 `MemoryOutOfBounds/InvalidAddress/MemoryAlignment` 统一映射为 `PageFault`（按访问类型），以便上层 trap/异常机制处理。
- 验证：严格验收运行到指令上限时不再直接崩溃；相关单元测试通过。
- 状态：已修复。

---------------------------------------------------------------------------

（迁移注意）
- 已将 Phase3 严格验收文档中的关键问题迁移到本文件；后续若需把更多历史条目拆分为模块子文件（如 `docs/development/phase3_issues.md`），请回复我确认，我会继续拆分与重组织。
