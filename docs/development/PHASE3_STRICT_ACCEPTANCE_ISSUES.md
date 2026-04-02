# Phase 3 严格验收问题与解决记录

> 目标：执行 `scripts/run_linux_phase3_acceptance.ps1 -AutoResolveArtifacts -StrictUserlandMarker`，并达到用户态 marker：`Run /init as init process`。

## 当前结论

- **基础链路验收（非 strict）**：可通过。
- **严格验收（strict）**：当前仍未通过，卡在 S-mode 指令页故障循环，未进入用户态 marker。
- **本轮已解决的关键阻塞**：构建脚本稳定性、RV32C 跳转、AMO 指令覆盖、CSR 兼容缺失、默认装载地址错配导致的早期崩溃。
- **本轮新增定位信息**：心跳输出已包含 `satp/stval` 与 Sv32 页表快照（`sv32.root/pte1/pte0`）。

## legacy 简要要点（已并入）

为提升检索召回，这里保留历史归档中的精简结论（已并入当前文档）：

- 已修复：构建脚本 WSL 参数与引号问题；
- 已修复：RV32C 跳转立即数位映射导致的早期崩溃；
- 已修复：AMO 指令覆盖不足；
- 已修复：CSR 兼容缺失（PMP、计数器别名等）；
- 已修复：越界错误已归一化为页故障，避免直接崩溃；
- 尚阻塞：用户态进入前仍有 Instruction Page Fault fault-loop，需要对 `sepc` 附近反汇编与页表做定点比对。

## 本轮执行环境与命令

- 仓库：`d:\code\myCPU`
- 工件：`artifacts/phase3/fw_jump.elf`、`Image`、`rootfs.ext2`、`virt-qemu.dtb`
- 关键命令（严格验收）：
  - `powershell -ExecutionPolicy Bypass -File .\scripts\run_linux_phase3_acceptance.ps1 -AutoResolveArtifacts -StrictUserlandMarker`

## 问题时间线（按出现顺序）

### 1) Buildroot 工件构建链路不稳定（WSL/路径/引号）

#### 现象

- 构建脚本在 WSL 输出目录解析、命令拼接与引号处理上反复失败，导致 `images` 目录无法稳定定位。

#### 根因

- PowerShell 与 Bash 混合执行时参数展开与引号语义冲突。

#### 修复

- 在 `scripts/build_phase3_buildroot_artifacts.ps1` 中引入统一的 WSL 脚本执行/捕获封装，并用脚本片段方式解析输出目录。
- 最终可稳定复制三件套到 `artifacts/phase3`。

---

### 2) OpenSBI 启动早期崩溃（RV32C 跳转错误）

#### 现象

- 早期出现异常跳转/越界。

#### 根因

- `C.J/C.JAL` 立即数位映射错误，导致跳转目标错误。

#### 修复

- 修正 `decode_c_imm_j` 位映射，并补充单测。

---

### 3) 原子指令缺失导致 Linux/SBI 继续崩溃

#### 现象

- 出现 `Unsupported AMO funct5=xxxxx`。

#### 根因

- RV32A 覆盖不足（仅部分 AMO/LRSC）。

#### 修复

- 扩展 `src/instruction/execute.rs` 的 `execute_amo`：

  - `lr.w / sc.w`
  - `amoadd.w / amoswap.w`
  - `amoxor.w / amoand.w / amoor.w`
  - `amomin.w / amomax.w / amominu.w / amomaxu.w`

- 新增对应单测（AMOOR/AMOMAXU 等）。

---

### 4) CSR 兼容缺失（0x3a0/0x3c0/0x306/0xda0/0xc01 等）

#### 现象

- 启动过程中多次报 `Invalid CSR address`。

#### 根因

- PMP/计数器/可选机器 CSR 与用户计数器别名支持不足。

#### 修复

- `src/cpu/csr/mod.rs` 增加/扩展：

  - PMP CSR 存根（`pmpcfg*`/`pmpaddr*`）
  - `MCOUNTEREN`
  - 机器态未知 CSR 兼容 Map 存储
  - 用户计数器别名 CSR：`cycle/time/instret` 及高位、`hpmcounter3..31` 及高位

- 新增单测验证读写与别名行为。

---

### 5) `fw_jump` 装载地址错配导致更深阶段错误

#### 现象

- OpenSBI Banner 正常，但后续出现越界/异常。

#### 根因

- 验收脚本默认 `PayloadAddr/DtbAddr` 与 `fw_jump` 实际 Next Address/Arg1 约定不一致。

#### 修复

- `scripts/run_linux_phase3_acceptance.ps1` 默认改为：

  - `PayloadAddr = 0x80400000`
  - `DtbAddr = 0x82200000`

- `scripts/build_phase3_buildroot_artifacts.ps1` 末尾建议命令改为优先 `-AutoResolveArtifacts -StrictUserlandMarker`（优先真实 DTB，不再默认 `-AutoDtb`）。

---

### 6) 运行期越界导致进程直接退出（无法进入 trap 路径）

#### 现象

- 数据访问越界会直接终止模拟器。

#### 根因

- 越界错误直接上抛为致命错误，没有归一化为可由内核处理的 fault。

#### 修复

- 在 `src/cpu/core.rs` 对 fetch/load/store 路径做错误归一化：

  - 将 `MemoryOutOfBounds/InvalidAddress/MemoryAlignment` 统一映射为 `PageFault`（按访问类型）。

- 结果：strict 跑到指令上限时不再直接崩溃。

---

### 7) S 态中断注入路径不完整（SIP/MIP 可写位）

#### 现象

- OpenSBI 进入 S-mode 后，系统长时间无用户态推进，怀疑 supervisor pending interrupt 注入链路不完整。

#### 根因

- 早期实现中，`MIP/SIP` 对 `STIP/SEIP` 的机器态写入覆盖不足，`sync_interrupts()` 也未将 `mip` 的可委托 pending 位映射到 `sip` 视图。

#### 修复

- 扩展 `src/cpu/csr/machine.rs`：
  - `Mip::write()` 支持 `SSIP/STIP/SEIP` 可写位。
  - 新增 `set_ssip/set_stip/set_seip`。
- 扩展 `src/cpu/csr/mod.rs`：
  - 机器态写 `SIP` 时支持 `SSIP/STIP/SEIP`。
- 扩展 `src/cpu/core.rs`：
  - `sync_interrupts()` 中将 `mip` 的 `SSIP/STIP/SEIP` 反映至 `sip`。

> 说明：该修复提升了中断模型完整性，但当前 strict 阶段仍未达到用户态 marker。

## 当前未解决阻塞（strict 未通过的直接原因）

### 症状

严格验收可以稳定进入 OpenSBI 并切换至 S-mode，但一直未出现用户态 marker，日志呈现：

- `scause=0x0000000c`（Instruction Page Fault）
- `sepc=0xc0006c70`
- PC 在 `0xc0723dxx` 附近循环
- 最终达到指令上限后失败：`userland marker not found`

并且从新增页表观测可见：

- `satp=0x80081cbd`（Sv32 已开启）
- `sv32.root=0x81cbd000`
- `sv32.pte1=0x201000eb`（叶子 PTE，具备 `V/R/X/A/D`）

这表明 fault-loop 已不再是“MMU 未开启”级别问题，而是更深层的内核启动兼容问题。

### 解释

这说明当前仍存在“内核早期执行路径/页表映射/指令语义”层面的深层兼容问题，虽然已从“立即崩溃”推进到“可稳定运行并复现 fault-loop”，但尚未进入 `Run /init as init process` 阶段。

## 本轮代码改动清单

- `src/instruction/execute.rs`
  - 扩展 AMO 指令实现与测试。
- `src/cpu/csr/mod.rs`
  - 扩展 CSR 兼容覆盖（含用户计数器别名）与测试。
- `src/cpu/csr/machine.rs`
  - 补齐 MIP 的 supervisor pending 位写入与测试。
- `src/cpu/core.rs`
  - 内存越界错误归一化为页故障，避免直接退出；同步中断时映射 `mip -> sip`。
- `src/main.rs`
  - 心跳新增 `satp/stval/sv32.root/sv32.pte1/sv32.pte0` 调试字段。
- `scripts/run_linux_phase3_acceptance.ps1`
  - 调整默认 `PayloadAddr/DtbAddr` 为 `fw_jump` 友好值。
- `scripts/build_phase3_buildroot_artifacts.ps1`
  - 更新下一步 strict 验收建议命令。

## 验证结果

- `cargo test --lib -q`：**263 passed / 0 failed**。
- strict 验收：
  - 命令可稳定执行到上限，OpenSBI 信息完整输出；
  - 仍未达到用户态 marker，当前结论为 **未完成 strict 通过**。

## 下一步建议（面向 strict 最终收敛）

1. 增加 fault-loop 定位信息（`stval/satp/stvec`、fault 前 N 条 PC 环形缓冲）。
2. 对 `sepc=0xc0006c70` 做定点反汇编与页表项核对（比对预期 PTE 权限与映射）。
3. 重点复核 RV32C 其余已实现指令语义（特别是分支/栈相关路径）与 trap return 细节。
4. 若需要，将 Linux 首屏前阶段纳入 DiffTest/trace 对齐（先定位第一处分歧再修）。
