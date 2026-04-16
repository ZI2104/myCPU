# myCPU 架构设计文档

## 一、架构选型

### 目标架构: RISC-V RV32I

| 特性       | 说明                                  |
| ---------- | ------------------------------------- |
| 基础指令集 | RV32I (40 条指令)                     |
| 扩展指令集 | M (乘除法), C (压缩, 可选)            |
| 特权级     | M-mode + S-mode + U-mode              |
| 流水线     | 伪6 级流水线 (pre-IF/IF/ID/EX/MEM/WB) |
| 地址宽度   | 32 位                                 |
| 内存管理   | Sv32 分页 (可选)                      |

### 选型理由

1. **规范开放**: RISC-V 规范完全开放，文档详尽
2. **指令简洁**: 基础指令集仅 40 条，易于正确实现
3. **现代设计**: 无历史包袱，架构设计清晰
4. **生态完善**: 可运行真实的 RISC-V 程序（Rust/C 编译产物）
5. **OS 友好**: 完整特权级支持，为后续操作系统课程预留接口

---

## 二、系统架构总览

```mermaid
graph TB
    subgraph Frontend["前端接口"]
        Loader["Loader<br/>ELF/Binary"]
        Debugger["Debugger<br/>GDB Protocol"]
        CLI["CLI / Web UI"]
    end

    Bus["System Bus<br/>系统总线"]

    subgraph CPUCore["CPU Core"]
        Pipeline["5-Stage Pipeline"]
        Regs["Register File<br/>x0-x31"]
        CSR["CSR Bank"]
        MMU["MMU / TLB"]
    end

    subgraph MemSubsystem["Memory Subsystem"]
        RAM["RAM<br/>256MB"]
        ROM["ROM/Boot"]
        Cache["Cache<br/>(Optional)"]
    end

    subgraph Peripherals["Peripherals"]
        UART["UART<br/>NS16550A"]
        Timer["Timer<br/>CLINT"]
        PLIC["PLIC<br/>中断控制器"]
        VirtIO["VirtIO<br/>(Optional)"]
    end

    Loader --> Bus
    Debugger --> Bus
    CLI --> Bus

    Bus <--> Pipeline
    Bus <--> RAM
    Bus <--> UART

    Pipeline --> Regs
    Pipeline --> CSR
    Pipeline --> MMU
```

---

### 当前实现同步（2026-04-09）

- **MMU/TLB 路径**：已从“可选设计”推进到可运行实现，`src/cpu/tlb.rs` 提供 TLB 基础能力（命中/未命中、ASID 隔离、刷新）。
- **性能可观测性**：性能链路（`PerfCollector` → `PerfReport` → 可视化快照/前端）已打通 cache/TLB 指标。
- **回归工程化**：`riscv-tests` 已接入 CI（DiffTest + QEMU），并将每测例性能 JSON 上传为 artifacts，用于 ISA 正确性与性能趋势联合观测。

---

## 三、6 级流水线设计

### 流水线阶段

```mermaid
flowchart LR
    subgraph pre_IF["pre-IF - 预取指"]
        PC["PC"]
        NextPC["NextPC<br/>计算"]
        IMEM_REQ["指令内存<br/>读请求"]
    end

    subgraph IF["IF - 取指"]
        IMEM["Instruction<br/>Memory"]
        IR["Instruction<br/>Register<br/>InstrFetchLatch"]
    end

    subgraph ID["ID - 译码"]
        DEC["Decoder"]
        RF["Register<br/>File"]
        CTL["Control<br/>Signals"]
    end

    subgraph EX["EX - 执行"]
        ALU["ALU"]
        BR["Branch<br/>Unit"]
        FWD["Forwarding<br/>Unit"]
    end

    subgraph MEM["MEM - 内存访问"]
        DMEM["Data<br/>Memory"]
        LS["Load/Store<br/>Unit"]
        DL["Data Read<br/>Latch"]
    end

    subgraph WB["WB - 写回"]
        MUX["Result Mux"]
        WR["Write Back<br/>to RF"]
    end

    NextPC --> IMEM_REQ
    IMEM_REQ --> IMEM
    IMEM --> IR
    IR --> DEC
    CTL --> ALU
    ALU --> DMEM
    DMEM --> MUX
    DMEM --> DL
    FWD -.->|Forward Path| RF
```

### 流水线寄存器

| 寄存器          | 内容                           |
| --------------- | ------------------------------ |
| InstrFetchLatch | 指令 (从同步 RAM 输出)         |
| ID/EX           | PC, 操作数, 立即数, 控制信号   |
| EX/MEM          | ALU 结果, Store 数据, 控制信号 |
| DataReadLatch   | 内存读取数据 (从同步 RAM 输出) |
| MEM/WB          | 内存数据, ALU 结果, 写回目标   |

### 流水线冒险处理

```mermaid
flowchart TD
    subgraph Hazards["流水线冒险"]
        DH["Data Hazard<br/>数据冒险"]
        CH["Control Hazard<br/>控制冒险"]
        SH["Structural Hazard<br/>结构冒险"]
    end

    subgraph Solutions["解决方案"]
        FW["Forwarding<br/>前递"]
        ST["Stalling<br/>暂停"]
        BP["Branch Prediction<br/>分支预测"]
        FL["Flushing<br/>冲刷"]
    end

    DH --> FW
    DH --> ST
    CH --> BP
    CH --> FL
    SH --> ST
```

#### 数据冒险 - 前递策略

```
场景: ADD x1, x2, x3  →  SUB x4, x1, x5
      (x1 尚未写回，但 SUB 需要 x1)

解决方案: EX 阶段结果直接前递到 ID 阶段
```

#### 控制冒险 - 分支预测

| 策略             | 说明                            | 实现复杂度 |
| ---------------- | ------------------------------- | ---------- |
| Always Not Taken | 总是预测不跳转 (静态)           | 简单       |
| 1-Bit            | 记住上次结果                    | 简单       |
| 2-Bit Saturating | 4 状态 FSM，需连续 2 次错误翻转 | 中等       |
| Local (2-Level)  | 每分支 BHR + 共享 PHT           | 中等       |
| Global (gshare)  | GHR XOR PC 索引 PHT             | 中等       |
| BTB              | 分支目标缓存 (配合方向预测器)   | 复杂       |

**当前实现**: 5 种方向预测器 + BTB，支持运行时切换，通过前端可视化对比。

##### 预测器实现

所有预测器共享 `BranchPredictor` trait，方向预测 + BTB 目标预测组合：

```
PredictorManager
├── Box<dyn BranchPredictor>  // 方向预测 (taken/not-taken)
└── BranchTargetBuffer        // 目标地址预测 (256 项直接映射)
```

**方向预测器对比**：

| 预测器          | 存储开销                                 | 特点                                |
| --------------- | ---------------------------------------- | ----------------------------------- |
| Always Not      | 0                                        | 基线，无动态预测                    |
| 1-Bit           | 1024 × 1-bit (BHT)                       | 循环首尾各误预测一次                |
| 2-Bit           | 1024 × 2-bit (BHT)                       | 抵抗单次异常，需连续 2 次错误才翻转 |
| Local           | 1024 × 10-bit (BHR) + 1024 × 2-bit (PHT) | 捕获每分支行为模式                  |
| Global (gshare) | 10-bit GHR + 1024 × 2-bit (PHT)          | 捕获分支间相关性 (GHR XOR PC 索引)  |

**BTB 设计**：

- 256 项直接映射缓存 (PC[9:2] 索引，PC[31:10] 作为 tag)
- 每次 taken 分支/跳转更新 BTB 条目
- 预测 taken 时查询 BTB 获取目标地址

##### 预测流程

```
Cycle N:
  IF/ID: 指令 B → predictor.predict(B.pc) → PredictionResult
  pre-IF: 若预测 taken → 从 BTB 目标取指；否则 PC+4
  预测结果存入 IfIdRegister.prediction

Cycle N+1:
  ID: 解码指令 B → 实际分支结果 (branch_taken, branch_target)
  predictor.resolve(B.pc, actual_taken, actual_target) → 更新预测器
  若 prediction != actual → 冲刷 IF/ID (1 周期惩罚)
```

##### WebSocket 命令

| 命令                      | 说明                                           |
| ------------------------- | ---------------------------------------------- |
| `predictor_switch <type>` | 切换预测器 (none/one_bit/two_bit/local/global) |

##### 前端可视化

`PredictorPanel` 组件显示：

- 预测器类型选择器 (下拉框切换)
- 预测准确率 (带颜色编码: 绿>90%, 黄70-90%, 红<70%)
- BTB 命中率统计
- BTB 条目表 (tag, target, branch/jump)

### 新流水线架构 (6 级)

**关键设计变更**：

1. **新增 pre-IF 阶段**：从 5 级扩展到 6 级流水线
2. **同步 RAM 设计**：所有内存访问都经过同步寄存器
3. **内存访问延迟**：1 周期延迟（指令和数据内存）

#### pre-IF 阶段设计

pre-IF 是一个"伪阶段"（没有流水线寄存器），主要功能：

- 计算下一条指令地址（nextPC）
- 向指令内存发起读取请求（使用 nextPC 作为地址）
- 分支指令目标地址计算

```rust
// pre_fetch() - 组合逻辑，无状态
fn pre_fetch(&self, current_pc: u32, branch_target: Option<u32>) -> u32 {
    match branch_target {
        Some(target) => target,  // 分支跳转目标
        None => current_pc + 4, // 顺序执行
    }
}
```

#### IF 阶段设计

IF 阶段从 `InstrFetchLatch` 读取指令：

- `InstrFetchLatch` 是同步 RAM 的输出寄存器
- 数据在请求后 1 个周期可用
- pre-IF 发起的请求在 IF 阶段获得结果

```rust
// fetch() - 从同步 RAM 读取
fn fetch(&mut self, pc: u32) -> Option<u32> {
    // pre-IF 已经发起请求，这里读取 latch
    self.instr_latch.read()
}
```

#### 内存访问设计

1. **指令内存**：

   - pre-IF 发起请求 → IF 阶段从 `InstrFetchLatch` 读取
   - 1 周期延迟
2. **数据内存**：

   - EX 阶段计算地址 → MEM 阶段从 `DataReadLatch` 读取
   - Store 操作在 MEM 阶段直接写入总线
   - 1 周期延迟

#### 同步 RAM 读保持（Read-Hold）

同步 RAM 的 Q 端具有**读保持**特性：从采样到上一个有效读命令的时钟沿开始，Q 端将保持该次读操作对应的数据，直至采样到下一个有效读命令。

- **`InstrFetchLatch`**：stall 期间不更新 latch（pre-IF 不发起新请求），保持上一次有效读数据
- **`DataReadLatch`**：只在 EX 阶段发起 load 请求时（`new_data_latch.valid == true`）更新 latch，非 load 指令不覆盖，保持上一次有效读数据

#### 新增数据结构

```rust
// 指令读取 latch (pre-IF → IF)
pub struct InstrFetchLatch {
    data: Option<u32>,  // 指令数据
    valid: bool,       // 数据有效标志
}

// 数据读取 latch (EX → MEM)
pub struct DataReadLatch {
    data: Option<u32>,  // 数据内存读取结果
    valid: bool,       // 数据有效标志
}
```

#### 分支惩罚优化：ID 阶段早期分支解析

**分支/跳转指令在 ID（译码）阶段即完成条件判断和目标地址计算**，而非在 EX 阶段。这使 pre-IF 在同一周期内即可重定向取指。

- **原设计**（分支在 EX）：3 周期分支惩罚
- **新设计**（分支在 ID）：**1 周期**分支惩罚

关键设计决策：

1. **ID 阶段前递**：从 `ex_mem`（非 load）和 `mem_wb` 前递操作数到 ID 阶段，用于分支条件判断
2. **新增 stall 场景**：
   - `id_ex` 写入分支源寄存器 → stall 1 周期
   - `ex_mem` 是 load 且写入分支源寄存器 → stall 1 周期
3. **冲刷范围**：仅冲刷 `if_id`（分支指令本身需通过 EX 计算 JAL/JALR 链接地址）
4. **trap_return 不走 ID 分支解析**：目标来自 CSR，由 `mod.rs` 直接处理

时序对比：

```text
分支在 EX（3 周期惩罚）：
  T:   分支在 EX，计算 branch_taken → ex_mem 更新
  T+1: pre-IF 重定向到目标
  T+2: IF 读到目标指令
  T+3: 目标到达 ID → 浪费 3 周期

分支在 ID（1 周期惩罚）：
  T:   分支在 ID，决定 branch_taken + pre-IF 同周期重定向
  T+1: IF 读到目标指令；分支通过 EX
  T+2: 目标到达 ID → 浪费 1 周期
```

#### 流水线方法变更

- `FetchStage` 拆分为 `pre_fetch()` 和 `fetch()` 两个方法
- `MemoryStage` 新增 `execute_with_latch()` 方法处理数据读取 latch

---

## 四、特权级架构

### M/S/U 三级特权模式

```mermaid
graph TB
    subgraph Privilege["特权级层次"]
        M["Machine Mode<br/>M-mode<br/>最高权限"]
        S["Supervisor Mode<br/>S-mode<br/>操作系统内核"]
        U["User Mode<br/>U-mode<br/>用户程序"]
    end

    M -->|ecall| S
    S -->|ecall| U

    M -.->|mret| S
    S -.->|sret| U

    style M fill:#ff6b6b
    style S fill:#4ecdc4
    style U fill:#95e1d3
```

### 各模式权限

| 特权级     | 权限             | 用途                           |
| ---------- | ---------------- | ------------------------------ |
| **M-mode** | 完全访问所有资源 | Bootloader, 固件, 底层异常处理 |
| **S-mode** | 访问 S/U 级资源  | 操作系统内核                   |
| **U-mode** | 受限访问         | 用户应用程序                   |

### CSR 寄存器分布

```mermaid
graph LR
    subgraph MMode["Machine Mode CSRs"]
        mstatus["mstatus"]
        mtvec["mtvec"]
        mepc["mepc"]
        mcause["mcause"]
        mie["mie"]
        mip["mip"]
        satp["satp<br/>(MMU)"]
    end

    subgraph SMode["Supervisor Mode CSRs"]
        sstatus["sstatus"]
        stvec["stvec"]
        sepc["sepc"]
        scause["scause"]
        sie["sie"]
        sip["sip"]
    end

    subgraph UMode["User Mode CSRs"]
        ustatus["ustatus"]
        utvec["utvec"]
        uepc["uepc"]
        ucause["ucause"]
    end
```

### 异常/中断处理流程

```mermaid
sequenceDiagram
    participant U as U-mode
    participant S as S-mode
    participant M as M-mode
    participant HW as Hardware

    Note over U: 执行用户程序

    U->>HW: 触发异常/中断
    HW->>HW: 保存 PC → xepc
    HW->>HW: 保存原因 → xcause
    HW->>HW: 设置 xstatus
    HW->>M: 跳转 mtvec (M-mode)

    M->>M: 执行异常处理程序
    M->>S: mret (委托给 S-mode)

    S->>S: 执行异常处理程序
    S->>U: sret (返回用户态)

    Note over U: 恢复执行
```

---

## 五、模块详细设计

### 1. CPU Core 模块

```
src/cpu/
├── mod.rs           # CPU 主结构
├── registers.rs     # 寄存器组 (x0-x31 + PC)
├── csr.rs           # 控制状态寄存器
├── pipeline/
│   ├── mod.rs       # 流水线控制
│   ├── fetch.rs     # IF 阶段
│   ├── decode.rs    # ID 阶段
│   ├── execute.rs   # EX 阶段
│   ├── memory.rs    # MEM 阶段
│   ├── writeback.rs # WB 阶段
│   ├── hazard.rs    # 冒险检测与处理
│   └── forward.rs   # 前递逻辑
├── alu.rs           # 算术逻辑单元
├── branch.rs        # 分支单元
└── privilege.rs     # 特权级管理
```

### 2. 指令集模块

```
src/instruction/
├── mod.rs           # 指令定义
├── decoder.rs       # 译码器
├── opcode.rs        # 操作码枚举
├── format/
│   ├── r_type.rs    # R-type: ADD, SUB, AND, OR, XOR...
│   ├── i_type.rs    # I-type: ADDI, ANDI, LB, LW...
│   ├── s_type.rs    # S-type: SB, SH, SW
│   ├── b_type.rs    # B-type: BEQ, BNE, BLT...
│   ├── u_type.rs    # U-type: LUI, AUIPC
│   └── j_type.rs    # J-type: JAL, JALR
└── extension/
    ├── m_ext.rs     # M 扩展: MUL, DIV...
    └── c_ext.rs     # C 扩展: 压缩指令
```

### 3. 内存模块

```
src/memory/
├── mod.rs           # 内存抽象接口
├── bus.rs           # 系统总线
├── ram.rs           # RAM 实现
├── rom.rs           # ROM/Bootloader
├── mmu/
│   ├── mod.rs       # MMU 接口
│   ├── tlb.rs       # TLB 缓存
│   ├── page_table.rs # 页表管理
│   └── sv32.rs      # Sv32 分页实现
└── address.rs       # 地址空间定义

> 2026-03-31 进展说明：当前仓库已在 `src/cpu/mmu.rs` 实现 Sv32 软件页表遍历（两级 walk），并在**单周期 CPU**的取指与 Load/Store 路径接入翻译入口；页故障已接入 trap 流程。
```

### 4. 中断模块

```
src/interrupt/
├── mod.rs           # 中断抽象
├── plic.rs          # 平台级中断控制器
├── clint.rs         # 核心本地中断器
├── exception.rs     # 异常处理
└── trap.rs          # 陷阱处理
```

### 4.1 NPU/LPU MMIO 协处理器

```
src/peripheral/
├── npu.rs           # NPU: Add/Mul/Max/Relu
└── lpu.rs           # LPU: Language Processing Unit（纯语言 opcode）
```

- NPU 基地址：`0x2001_0000`
- LPU 基地址：`0x2001_1000`
- 统一寄存器风格：`CONTROL/STATUS/OP_A/OP_B/RESULT/OPCODE/CYCLES`
- 中断模型：计算完成后置位 `IRQ_PENDING`，CPU 可通过总线轮询并确认
- LPU 当前能力：
  - `ByteTokenize`（opcode `0x10`，支持单次与 descriptor 批处理）
  - `EmbeddingBag`（opcode `0x11`，固定 embedding 表 + descriptor sum pooling）
  - `GreedyDecode`（opcode `0x12`，argmax 解码，支持单次与 descriptor）
  - `TopKSampleDecode`（opcode `0x13`，top-k + temperature 采样，支持单次与 descriptor）
  - `TopPSampleDecode`（opcode `0x14`，top-p (nucleus) + temperature 采样，支持单次与 descriptor）

### 4.2 GPU/TPU 模拟加速器

```
src/peripheral/
├── gpu.rs           # GPU: MatMul/Conv2d/Pool2d/激活函数 (FP32+INT8)
├── tpu.rs           # TPU: INT8 量化矩阵乘 + 量化/反量化
src/traits/
└── accelerator.rs   # Accelerator trait + KernelType/Precision/TensorDescriptor
```

- GPU 基地址：`0x2001_2000`（4 KB MMIO），IRQ 源：PLIC #13
- TPU 基地址：`0x2001_3000`（4 KB MMIO），IRQ 源：PLIC #14
- GPU 支持 15 种内核：MatMul、Conv2d、Pool2dMax/Avg、VectorAdd/Mul/Dot/Scale、Relu/Relu6/LeakyRelu/Sigmoid/Tanh/Softmax
- TPU 专注 INT8 量化矩阵乘，支持 per-tensor 量化参数
- 共享内存模型：加速器通过 DMA 风格直接访问 guest RAM（无独立 VRAM）
- `pending_start` 机制：寄存器写入 START 位 → Bus 检测 → 调用 `execute_with_memory` 并传入 RAM 区域
- 详细 API 见 `docs/guides/GPU_TPU_API.md`

### 4.3 协处理器优化设计（V2 落地进展）

> 说明：控制面/事件面与 Doorbell 汇聚已落地，当前已收敛到纯 V2 拓扑。

#### 挂载方式重构（控制面 / 数据面 / 事件面分离）

1. **控制面（MMIO）**
   - 每个加速器保留独立控制寄存器页（配置、启动、状态、统计）。
   - 增加 `ACC_CTRL_ROOT` 统一能力发现（版本、特性位、engine mask）。
2. **数据面（共享内存 + 描述符队列）**
   - 统一 descriptor ring 结构，按 engine 使用不同 opcode 子空间。
   - 大数据仅走 guest RAM（DMA 风格），MMIO 仅传控制参数与指针。
3. **事件面（中断/门铃）**
   - 为 NPU/LPU/GPU/TPU 分配稳定 PLIC 中断源，新增 Doorbell/Completion 汇总寄存器。
   - 支持“每 engine 细粒度中断 + 全局摘要中断”双模式。

#### 地址映射重构（V2 目标）

```text
0x2000_0000 ─ 0x2000_0FFF  ACC_CTRL_ROOT（能力发现/版本/全局状态）
0x2000_1000 ─ 0x2000_1FFF  ACC_DOORBELL（统一任务提交/完成队列）
0x2001_0000 ─ 0x2001_0FFF  NPU_CTRL
0x2001_1000 ─ 0x2001_1FFF  LPU_CTRL
0x2001_2000 ─ 0x2001_2FFF  GPU_CTRL
0x2001_3000 ─ 0x2001_3FFF  TPU_CTRL
0x2002_0000 ─ 0x2002_FFFF  Accelerator Reserved（后续 VPU/ISP/NIC Offload）
```

#### 当前实现状态（2026-04-16-R3）

- `ACC_CTRL_ROOT` / `ACC_DOORBELL` 已在总线层实现统一控制面寄存器窗口。
- `ACC_CTRL_ROOT.MODE` 固定为 V2 启用态（`0x2001_xxxx` 四个 engine 窗口始终生效）。
- Doorbell 统一提交 ABI 已落地：`engine + desc_addr + desc_len + notify`。
- Root 汇总寄存器已落地：`engine_mask`、`irq_summary`、`doorbell_submits/completes/errors`。
- V1 alias 已移除，`ACC_CTRL_ROOT/ACC_DOORBELL` 低 `0x100` 子窗口不再透传 legacy 引擎寄存器。

#### 迁移策略（执行结果）

- **阶段 A（文档与接口冻结）**：已完成。
- **阶段 B（双地址窗口）**：已完成（迁移期过渡）。
- **阶段 C（收敛）**：已完成（仅保留 V2 拓扑）。
- **LPU 语义约束**：LPU 仅保留语言 opcode（`0x10~0x14`），不再接受历史逻辑 opcode（`0~5`）。

### 5. 外设模块

```
src/peripheral/
├── mod.rs           # Peripheral trait
├── uart.rs          # NS16550A 兼容串口
├── timer.rs         # 定时器
└── virtio/
    ├── mod.rs       # VirtIO 抽象
    ├── blk.rs       # VirtIO Block
    └── net.rs       # VirtIO Net
```

---

## 六、内存地址映射

### 当前实现（纯 V2 拓扑）

```
地址空间布局 (Sv32 物理地址):

0x0000_0000 ─ 0x0FFF_FFFF  RAM (256 MB)
0x1000_0000 ─ 0x1000_FFFF  ROM / Bootloader (64 KB)
0x0200_0000 ─ 0x0200_FFFF  CLINT (Core Local Interruptor)
0x0C00_0000 ─ 0x0FFF_FFFF  PLIC (Platform Level Interrupt Controller)
0x1000_1000 ─ 0x1000_1FFF  UART (Serial Port)
0x1000_2000 ─ 0x1000_2FFF  Input Device (MMIO)
0x2000_0000 ─ 0x2000_0FFF  ACC_CTRL_ROOT
0x2000_1000 ─ 0x2000_1FFF  ACC_DOORBELL
0x2001_0000 ─ 0x2001_0FFF  NPU_CTRL
0x2001_1000 ─ 0x2001_1FFF  LPU_CTRL
0x2001_2000 ─ 0x2001_2FFF  GPU_CTRL (Simulated Accelerator, PLIC #13)
0x2001_3000 ─ 0x2001_3FFF  TPU_CTRL (Simulated Accelerator, PLIC #14)
0x3000_0000 ─ 0x3FFF_FFFF  VirtIO Devices
0x8000_0000 ─ 0xFFFF_FFFF  Reserved / Expansion
```

### V2 协处理器地址映射（当前生效拓扑）

```text
0x2000_0000 ─ 0x2000_0FFF  ACC_CTRL_ROOT
0x2000_1000 ─ 0x2000_1FFF  ACC_DOORBELL
0x2001_0000 ─ 0x2001_0FFF  NPU_CTRL
0x2001_1000 ─ 0x2001_1FFF  LPU_CTRL
0x2001_2000 ─ 0x2001_2FFF  GPU_CTRL
0x2001_3000 ─ 0x2001_3FFF  TPU_CTRL
```

> 当前策略：V2 拓扑始终启用；V1 alias 已移除。

### Sv32 当前实现边界（里程碑）

- 已完成：
  - Bare/Sv32 模式分流
  - 两级页表遍历（根表 + 次级表）
  - Instruction/Load/Store 权限位检查（X/R/W）
  - 页故障触发 `InstructionPageFault/LoadPageFault/StorePageFault`
- 暂未完成：
  - TLB / ASID 相关优化
  - A/D 位硬件更新语义
  - 细粒度权限语义（SUM/MXR）与缺页性能优化

---

## 七、开发阶段规划

```mermaid
gantt
    title myCPU 开发路线图（第4-8周）
    dateFormat  YYYY-MM-DD
    section 第4周（Phase 1-2）
    基础框架 + RV32I       :a1, 2026-03-30, 7d

    section 第5周（Phase 3）
    流水线 + 冒险处理      :b1, after a1, 7d

    section 第6周（Phase 4）
    特权级 + 异常中断      :c1, after b1, 7d

    section 第7周（Phase 5）
    外设 + 调试 + DiffTest :d1, after c1, 7d

    section 第8周（Phase 6）
    演示封装 + 回归矩阵    :e1, after d1, 7d
```

---

## 八、测试策略

### 核心方案: QEMU 差分测试 (DiffTest)

采用与 QEMU 逐指令对比的差分测试策略，确保模拟器行为与参考实现完全一致。

```mermaid
flowchart LR
    TestProgram["测试程序<br/>.elf"]

    subgraph MyCPU["myCPU"]
        MyStep["单步执行"]
        MyState["CPU 状态"]
    end

    subgraph QEMU["QEMU 参考"]
        QemuStep["单步执行"]
        QemuState["CPU 状态"]
    end

    Compare["状态对比"]
    Result["Pass / Fail"]

    TestProgram --> MyStep
    TestProgram --> QemuStep
    MyStep --> MyState
    QemuStep --> QemuState
    MyState --> Compare
    QemuState --> Compare
    Compare --> Result
```

### 差分测试架构

```rust
// 核心数据结构示意
struct DiffTestRunner {
    my_cpu: MyCpu,           // 待测模拟器
    qemu: QemuInstance,      // QEMU 参考实例
    compare_mask: CompareMask, // 需要对比的状态
}

struct CpuState {
    regs: [u32; 32],         // 通用寄存器
    pc: u32,                 // 程序计数器
    mode: PrivilegeMode,     // 特权级
    csrs: HashMap<u16, u32>, // CSR 寄存器
}

impl DiffTestRunner {
    fn step_and_compare(&mut self) -> Result<(), DiffError> {
        // 1. 单步执行 myCPU
        let my_state = self.my_cpu.step();

        // 2. 单步执行 QEMU
        let qemu_state = self.qemu.step();

        // 3. 对比状态
        self.compare_states(&my_state, &qemu_state)?;

        Ok(())
    }

    fn compare_states(&self, a: &CpuState, b: &CpuState) -> Result<(), DiffError> {
        // 对比 PC
        if a.pc != b.pc {
            return Err(DiffError::PcMismatch { my: a.pc, qemu: b.pc });
        }

        // 对比通用寄存器 (跳过 x0)
        for i in 1..32 {
            if a.regs[i] != b.regs[i] {
                return Err(DiffError::RegMismatch {
                    reg: i, my: a.regs[i], qemu: b.regs[i]
                });
            }
        }

        // 对比关键 CSR
        for csr in &[0x300, 0x305, 0x341] { // mstatus, mtvec, mepc
            if a.csrs.get(csr) != b.csrs.get(csr) {
                return Err(DiffError::CsrMismatch {
                    csr: *csr,
                    my: a.csrs.get(csr).copied(),
                    qemu: b.csrs.get(csr).copied()
                });
            }
        }

        Ok(())
    }
}
```

### QEMU 集成方式

**方案 A: GDB Stub (推荐)**

```mermaid
sequenceDiagram
    participant Runner as DiffTest Runner
    participant MyCPU as myCPU
    participant GDB as GDB Client
    participant QEMU as QEMU (gdb stub)

    Runner->>MyCPU: step()
    MyCPU-->>Runner: state

    Runner->>GDB: step command
    GDB->>QEMU: $s#73
    QEMU->>QEMU: execute one instruction
    QEMU-->>GDB: registers state
    GDB-->>Runner: state

    Runner->>Runner: compare states
```

```bash
# 启动 QEMU GDB Stub
qemu-system-riscv32 -M virt -nographic -kernel test.elf -s -S

# -s: 开启 GDB server (端口 1234)
# -S: 启动时暂停
```

**方案 B: QEMU Plugin API**

```c
// QEMU 插件: 每条指令后导出状态
#include <qemu-plugin.h>

static void vcpu_insn_exec(unsigned int cpu_index, void *udata)
{
    // 导出寄存器状态到共享内存
    export_cpu_state(cpu_index);
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id)
{
    qemu_plugin_register_vcpu_insn_exec_cb(id, vcpu_insn_exec,
                                           QEMU_PLUGIN_CB_NO_REGS, NULL);
    return 0;
}
```

### 测试层次

```mermaid
graph TB
    subgraph Tests["测试金字塔"]
        E2E["E2E Tests<br/>运行完整程序"]
        Int["Integration Tests<br/>流水线/异常处理"]
        Unit["Unit Tests<br/>单指令 DiffTest"]
    end

    Unit --> Int
    Int --> E2E
```

### 测试程序来源

| 来源                 | 用途                 | 优先级 |
| -------------------- | -------------------- | ------ |
| **riscv-tests**      | ISA 指令正确性验证   | P0     |
| **自定义单元测试**   | 边界情况、异常场景   | P0     |
| **CoreMark**         | 性能基准、复杂流水线 | P1     |
| **自编译 Rust 程序** | 真实程序运行验证     | P1     |
| **riscv-torture**    | 压力测试、随机指令   | P2     |

### CI/CD 集成

```yaml
# .github/workflows/test.yml
name: DiffTest

on: [push, pull_request]

jobs:
  difftest:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install QEMU
        run: sudo apt install qemu-system-riscv32

      - name: Build myCPU
        run: cargo build --release

      - name: Run DiffTest
        run: cargo test --release --features difftest

      - name: Run riscv-tests
        run: ./scripts/run_riscv_tests.sh
```

---

## 九、调试接口

### 核心方案: GDB Remote Serial Protocol (RSP)

实现 GDB Remote Protocol，支持与 GDB、VSCode、CLion 等 IDE 联调。

```mermaid
flowchart LR
    subgraph Frontend["调试前端"]
        GDB["GDB CLI"]
        VSCode["VSCode<br/>Cortex-Debug"]
        CLion["CLion<br/>Embedded GDB"]
    end

    subgraph Protocol["GDB RSP"]
        TCP["TCP:1234"]
        Packets["RSP Packets<br/>$packet#checksum"]
    end

    subgraph MyCPU["myCPU Debug Stub"]
        Server["GDB Server"]
        Debug["Debug Controller"]
        CPU["CPU Core"]
    end

    GDB --> TCP
    VSCode --> TCP
    CLion --> TCP
    TCP --> Packets
    Packets --> Server
    Server --> Debug
    Debug --> CPU
```

### RSP 协议实现

**数据包格式:**

```
发送: $<packet-data>#<checksum>
应答: + (确认) 或 - (重传)

示例: $g#67
      请求读取所有寄存器，校验和 0x67
```

**核心命令实现:**

| 命令              | 功能           | 实现说明             |
| ----------------- | -------------- | -------------------- |
| `?`               | 查询停止原因   | 返回 `S05` (SIGTRAP) |
| `g`               | 读取所有寄存器 | 返回 32 个 GPR + PC  |
| `G<data>`         | 写入所有寄存器 | 设置寄存器值         |
| `m addr,len`      | 读取内存       | 十六进制返回         |
| `M addr,len:data` | 写入内存       | 修改内存             |
| `c [addr]`        | 继续执行       | 可选指定地址         |
| `s [addr]`        | 单步执行       | 执行一条指令         |
| `Z0,addr,kind`    | 设置软件断点   | kind=2 或 4          |
| `z0,addr,kind`    | 清除断点       | 移除断点             |
| `Z1,addr,kind`    | 设置硬件断点   | 可选实现             |
| `qSupported`      | 查询支持特性   | 返回能力列表         |
| `qAttached`       | 查询附加状态   | 返回是否已附加       |

**Rust 实现结构:**

```rust
pub struct GdbServer {
    listener: TcpListener,
    cpu: Cpu,
    breakpoints: HashSet<u32>,
    running: bool,
}

impl GdbServer {
    pub fn run(&mut self, port: u16) -> Result<()> {
        self.listener = TcpListener::bind(("127.0.0.1", port))?;

        loop {
            let (mut stream, _) = self.listener.accept()?;
            self.handle_connection(&mut stream)?;
        }
    }

    fn handle_packet(&mut self, packet: &str) -> Option<String> {
        let (cmd, args) = Self::parse_packet(packet);

        match cmd {
            "?" => Some(self.query_stop_reason()),
            "g" => Some(self.read_registers()),
            "G" => self.write_registers(args),
            "m" => self.read_memory(args),
            "M" => self.write_memory(args),
            "c" => self.continue_exec(args),
            "s" => self.step(args),
            "Z" => self.set_breakpoint(args),
            "z" => self.clear_breakpoint(args),
            "q" => self.handle_query(args),
            _ => Some(String::new()), // 空响应表示不支持
        }
    }

    fn read_registers(&self) -> String {
        let mut result = String::new();
        // 32 个通用寄存器
        for i in 0..32 {
            result.push_str(&format!("{:08x}", self.cpu.regs[i].to_le()));
        }
        // PC
        result.push_str(&format!("{:08x}", self.cpu.pc.to_le()));
        result
    }

    fn set_breakpoint(&mut self, args: &str) -> Option<String> {
        // Z0,addr,kind
        let parts: Vec<&str> = args.split(',').collect();
        if parts.len() >= 2 {
            let addr = u32::from_str_radix(parts[1], 16).ok()?;
            self.breakpoints.insert(addr);
            return Some("OK".to_string());
        }
        None
    }
}
```

### VSCode 集成配置

```json
// .vscode/launch.json
{
    "version": "0.2.0",
    "configurations": [
        {
            "name": "Debug myCPU",
            "type": "gdbtarget",
            "request": "attach",
            "gdbpath": "riscv32-unknown-elf-gdb",
            "executable": "${workspaceFolder}/target.elf",
            "target": {
                "host": "localhost",
                "port": 1234
            },
            "autorun": [
                "monitor reset",
                "load"
            ]
        }
    ]
}
```

### CLI 调试命令 (GDB)

```bash
# 连接 myCPU
riscv32-unknown-elf-gdb target.elf
(gdb) target remote localhost:1234

# 基本调试命令
(gdb) info registers          # 查看寄存器
(gdb) x/10i $pc              # 反汇编当前位置
(gdb) x/10xw 0x80000000      # 查看内存
(gdb) break *0x80000100      # 设置断点
(gdb) continue               # 继续执行
(gdb) stepi                  # 单步执行
(gdb) backtrace              # 调用栈
```

### 调试功能规划

| 阶段             | 功能       | 命令                         |
| ---------------- | ---------- | ---------------------------- |
| **Phase 5 基础** | 基础命令   | `g`, `G`, `m`, `M`, `c`, `s` |
| **Phase 5 基础** | 软件断点   | `Z0`, `z0`                   |
| **扩展**         | 查询命令   | `qSupported`, `qAttached`    |
| **扩展**         | 多线程支持 | `H`, `g`, `T`                |
| **高级**         | 观察点     | `Z2`, `z2` (数据断点)        |
| **高级**         | 指令追踪   | 自定义命令                   |

```mermaid
flowchart LR
    P5基础["Phase 5 基础<br/>g/G/m/M/c/s + 断点"]
    扩展["扩展功能<br/>qSupported + 多线程"]
    高级["高级功能<br/>观察点 + 追踪"]

    P5基础 --> 扩展 --> 高级
```

---

## 十一、关键设计决策

| 决策项       | 选择                       | 理由                             |
| ------------ | -------------------------- | -------------------------------- |
| 目标架构     | **RISC-V RV32I**           | 规范开放、指令简洁、生态完善     |
| 流水线深度   | **5 级** (IF/ID/EX/MEM/WB) | 经典设计，平衡复杂度与性能       |
| 特权级       | **M + S + U**              | 完整支持，便于后续 OS 开发       |
| 分支预测     | 静态预测 + 暂停            | 初期简化，后续可升级             |
| MMU          | Sv32 分页                  | RV32 标准，支持虚拟内存          |
| 外设接口     | Memory-Mapped I/O          | RISC-V 标准，易于扩展            |
| **测试策略** | **QEMU 差分测试**          | 与参考实现逐指令对比，确保正确性 |
| **调试接口** | **GDB Remote Protocol**    | 工业标准，支持 GDB/IDE 联调      |

---

## 十二、可扩展性设计

为确保架构在后期迭代中不需要大规模重构，采用以下抽象层设计。

### 1. 执行模型抽象

支持单周期执行与 5 级流水线之间的切换，便于调试和性能对比。

```mermaid
classDiagram
    class ExecutionModel {
        <<trait>>
        +step() CpuState
        +reset()
        +state() CpuState
    }

    class SingleCycle {
        -cpu: CpuCore
        +step() CpuState
    }

    class FiveStagePipeline {
        -stages: PipelineStage
        -hazard_unit: HazardUnit
        +step() CpuState
    }

    class CpuCore {
        -model: ExecutionModel
        +run()
        +switch_model(ModelType)
    }

    ExecutionModel <|.. SingleCycle
    ExecutionModel <|.. FiveStagePipeline
    CpuCore --> ExecutionModel
```

**Rust 接口定义：**

```rust
/// 执行模型抽象
pub trait ExecutionModel {
    /// 执行一条指令，返回执行后的状态
    fn step(&mut self) -> CpuState;

    /// 重置到初始状态
    fn reset(&mut self);

    /// 获取当前状态快照
    fn state(&self) -> CpuState;

    /// 当前正在执行的指令 (用于调试)
    fn current_instruction(&self) -> Option<u32>;
}

/// CPU 核心，可切换执行模型
pub struct CpuCore {
    model: Box<dyn ExecutionModel>,
    memory: Arc<RwLock<dyn Memory>>,
}

impl CpuCore {
    /// 切换执行模型 (运行时)
    pub fn switch_model(&mut self, model_type: ModelType) -> Result<()> {
        let state = self.model.state();
        self.model = match model_type {
            ModelType::SingleCycle => Box::new(SingleCycle::from_state(state, &self.memory)),
            ModelType::Pipeline => Box::new(FiveStagePipeline::from_state(state, &self.memory)),
        };
        Ok(())
    }
}

pub enum ModelType {
    SingleCycle,
    Pipeline,
}
```

**使用场景：**

| 场景          | 推荐模型    | 原因               |
| ------------- | ----------- | ------------------ |
| DiffTest 调试 | SingleCycle | 状态简单，易于对比 |
| 功能验证      | SingleCycle | 排除流水线干扰     |
| 性能测试      | Pipeline    | 真实性能指标       |
| 运行 OS       | Pipeline    | 完整功能           |

---

### 2. CSR 注册表模式

统一管理所有 CSR 寄存器，新增 CSR 只需实现 trait 并注册。

```mermaid
classDiagram
    class CsrRegister {
        <<trait>>
        +address() u16
        +read(PrivilegeMode) Result
        +write(u32, PrivilegeMode) Result
        +privilege() PrivilegeMode
    }

    class CsrBank {
        -registers: HashMap
        +register(CsrRegister)
        +read(u16) Result
        +write(u16, u32) Result
    }

    class MstatusCsr {
        -value: u32
    }
    class MtvecCsr {
        -value: u32
    }
    class SatpCsr {
        -value: u32
    }

    CsrRegister <|.. MstatusCsr
    CsrRegister <|.. MtvecCsr
    CsrRegister <|.. SatpCsr
    CsrBank --> CsrRegister
```

**Rust 接口定义：**

```rust
/// CSR 寄存器抽象
pub trait CsrRegister: Send + Sync {
    /// CSR 地址 (如 0x300 = mstatus)
    fn address(&self) -> u16;

    /// 读取值 (带权限检查)
    fn read(&self, mode: PrivilegeMode) -> Result<u32, CsrError>;

    /// 写入值 (带权限检查)
    fn write(&mut self, value: u32, mode: PrivilegeMode) -> Result<(), CsrError>;

    /// 所属特权级
    fn privilege(&self) -> PrivilegeMode;

    /// 是否可写 (某些 CSR 只读)
    fn writable(&self) -> bool { true }

    /// 字段掩码 (哪些位可写)
    fn write_mask(&self) -> u32 { 0xFFFF_FFFF }
}

/// CSR 注册表
pub struct CsrBank {
    registers: HashMap<u16, Box<dyn CsrRegister>>,
}

impl CsrBank {
    pub fn new() -> Self {
        let mut bank = Self { registers: HashMap::new() };

        // 注册标准 CSR
        bank.register(Box::new(MstatusCsr::new()));
        bank.register(Box::new(MtvecCsr::new()));
        bank.register(Box::new(MepcCsr::new()));
        bank.register(Box::new(McauseCsr::new()));
        bank.register(Box::new(SatpCsr::new()));
        // ... 更多

        bank
    }

    /// 注册新的 CSR (支持扩展)
    pub fn register(&mut self, csr: Box<dyn CsrRegister>) {
        self.registers.insert(csr.address(), csr);
    }

    pub fn read(&self, addr: u16, mode: PrivilegeMode) -> Result<u32, CsrError> {
        self.registers
            .get(&addr)
            .ok_or(CsrError::NotFound(addr))
            .and_then(|r| r.read(mode))
    }

    pub fn write(&mut self, addr: u16, value: u32, mode: PrivilegeMode) -> Result<(), CsrError> {
        let csr = self.registers
            .get_mut(&addr)
            .ok_or(CsrError::NotFound(addr))?;

        // 检查权限
        if mode < csr.privilege() {
            return Err(CsrError::PrivilegeViolation);
        }

        // 应用写掩码
        let masked_value = value & csr.write_mask();
        csr.write(masked_value, mode)
    }
}
```

**扩展新 CSR 示例：**

```rust
// 添加自定义 CSR 只需实现 trait
pub struct MyCustomCsr {
    value: u32,
}

impl CsrRegister for MyCustomCsr {
    fn address(&self) -> u16 { 0x7C0 } // 自定义地址范围
    fn read(&self, mode: PrivilegeMode) -> Result<u32, CsrError> {
        if mode < PrivilegeMode::Machine {
            return Err(CsrError::PrivilegeViolation);
        }
        Ok(self.value)
    }
    fn write(&mut self, value: u32, mode: PrivilegeMode) -> Result<(), CsrError> {
        if mode < PrivilegeMode::Machine {
            return Err(CsrError::PrivilegeViolation);
        }
        self.value = value;
        Ok(())
    }
    fn privilege(&self) -> PrivilegeMode { PrivilegeMode::Machine }
}

// 注册
csr_bank.register(Box::new(MyCustomCsr::new()));
```

---

### 3. 外设中断接口扩展

完善 `Peripheral` trait，支持中断声明和查询。

```mermaid
classDiagram
    class Peripheral {
        <<trait>>
        +base_address() u32
        +size() u32
        +read(u32) u32
        +write(u32, u32)
        +interrupt_lines() Vec
        +pending_interrupts() Vec
        +acknowledge_interrupt(u32)
    }

    class Uart {
        -irq_line: u32
        -pending: bool
    }
    class Timer {
        -irq_line: u32
        -mtimecmp: u64
    }
    class Plic {
        -irq_lines: Vec
    }

    Peripheral <|.. Uart
    Peripheral <|.. Timer
    Peripheral <|.. Plic

    class Bus {
        -peripherals: Vec
        +route_interrupt(u32) Peripheral
    }

    Bus --> Peripheral
```

**Rust 接口定义：**

```rust
/// 外设抽象 (扩展版)
pub trait Peripheral: Send + Sync {
    /// 基地址
    fn base_address(&self) -> u32;

    /// 地址空间大小
    fn size(&self) -> u32;

    /// 相对地址读取
    fn read(&self, offset: u32) -> u32;

    /// 相对地址写入
    fn write(&mut self, offset: u32, value: u32);

    /// 声明的中断线 (PLIC IRQ 编号)
    fn interrupt_lines(&self) -> Vec<u32> { vec![] }

    /// 当前待处理的中断
    fn pending_interrupts(&self) -> Vec<u32> { vec![] }

    /// 确认中断 (中断处理完成后调用)
    fn acknowledge_interrupt(&mut self, _irq: u32) {}

    /// 外设名称 (调试用)
    fn name(&self) -> &str { "unknown" }
}

/// UART 实现示例
pub struct Uart {
    base: u32,
    irq_line: u32,        // PLIC IRQ 编号
    pending_tx: bool,
    pending_rx: bool,
    // ... 寄存器
}

impl Peripheral for Uart {
    fn base_address(&self) -> u32 { self.base }
    fn size(&self) -> u32 { 0x1000 }
    fn name(&self) -> &str { "UART0" }

    fn interrupt_lines(&self) -> Vec<u32> {
        vec![self.irq_line]
    }

    fn pending_interrupts(&self) -> Vec<u32> {
        if self.pending_tx || self.pending_rx {
            vec![self.irq_line]
        } else {
            vec![]
        }
    }

    fn acknowledge_interrupt(&mut self, irq: u32) {
        if irq == self.irq_line {
            self.pending_tx = false;
            self.pending_rx = false;
        }
    }

    fn read(&self, offset: u32) -> u32 { /* ... */ }
    fn write(&mut self, offset: u32, value: u32) { /* ... */ }
}
```

---

### 4. DiffTest 快照接口

支持多层次状态对比，可扩展对比粒度。

```mermaid
classDiagram
    class StateSnapshot {
        <<trait>>
        +gpr_snapshot() u32
        +pc_snapshot() u32
        +csr_snapshot() HashMap
        +memory_hash() u64
        +pipeline_snapshot() PipelineState
    }

    class MinimalSnapshot {
        -gpr: u32
        -pc: u32
    }

    class FullSnapshot {
        -gpr: u32
        -pc: u32
        -csrs: HashMap
        -mem_hash: u64
    }

    class VerboseSnapshot {
        -gpr: u32
        -pc: u32
        -csrs: HashMap
        -pipeline: PipelineState
        -mem_changes: Vec
    }

    StateSnapshot <|.. MinimalSnapshot
    StateSnapshot <|.. FullSnapshot
    StateSnapshot <|.. VerboseSnapshot

    class DiffTestRunner {
        -snapshot_type: SnapshotType
        +compare() DiffResult
    }

    DiffTestRunner --> StateSnapshot
```

**Rust 接口定义：**

```rust
/// 状态快照抽象
pub trait StateSnapshot {
    /// 通用寄存器快照
    fn gpr_snapshot(&self) -> [u32; 32];

    /// PC 快照
    fn pc_snapshot(&self) -> u32;

    /// CSR 快照 (可选)
    fn csr_snapshot(&self) -> Option<HashMap<u16, u32>> { None }

    /// 内存哈希 (可选，用于快速检测内存变化)
    fn memory_hash(&self) -> Option<u64> { None }

    /// 流水线状态 (可选，用于深度调试)
    fn pipeline_snapshot(&self) -> Option<PipelineState> { None }

    /// 特权模式
    fn privilege_mode(&self) -> PrivilegeMode;
}

/// 流水线状态 (用于深度对比)
#[derive(Debug, Clone)]
pub struct PipelineState {
    pub if_stage: IfStageState,
    pub id_stage: IdStageState,
    pub ex_stage: ExStageState,
    pub mem_stage: MemStageState,
    pub wb_stage: WbStageState,
}

/// DiffTest 对比配置
pub struct DiffTestConfig {
    /// 对比层级
    pub snapshot_level: SnapshotLevel,
    /// 需要对比的 CSR 列表
    pub csr_whitelist: Vec<u16>,
    /// 忽略的寄存器 (如 x0)
    pub gpr_ignore: Vec<usize>,
    /// 内存对比范围 (可选)
    pub memory_regions: Vec<(u32, u32)>,
}

pub enum SnapshotLevel {
    /// 最小: 仅 GPR + PC
    Minimal,
    /// 标准: GPR + PC + 关键 CSR
    Standard,
    /// 完整: GPR + PC + 所有 CSR + 内存哈希
    Full,
    /// 详细: 包括流水线内部状态
    Verbose,
}

/// DiffTest 运行器
pub struct DiffTestRunner<M: ExecutionModel> {
    my_cpu: M,
    qemu: QemuInstance,
    config: DiffTestConfig,
}

impl<M: ExecutionModel> DiffTestRunner<M> {
    pub fn step_and_compare(&mut self) -> Result<(), DiffError> {
        // 1. 执行一步
        let my_state = self.my_cpu.step();
        let qemu_state = self.qemu.step()?;

        // 2. 根据配置层级对比
        match self.config.snapshot_level {
            SnapshotLevel::Minimal => {
                self.compare_minimal(&my_state, &qemu_state)?;
            }
            SnapshotLevel::Standard => {
                self.compare_standard(&my_state, &qemu_state)?;
            }
            SnapshotLevel::Full => {
                self.compare_full(&my_state, &qemu_state)?;
            }
            SnapshotLevel::Verbose => {
                self.compare_verbose(&my_state, &qemu_state)?;
            }
        }

        Ok(())
    }

    fn compare_minimal(&self, a: &dyn StateSnapshot, b: &dyn StateSnapshot) -> Result<(), DiffError> {
        // PC 对比
        if a.pc_snapshot() != b.pc_snapshot() {
            return Err(DiffError::PcMismatch {
                my: a.pc_snapshot(),
                qemu: b.pc_snapshot(),
            });
        }

        // GPR 对比 (跳过 x0 和忽略列表)
        for i in 0..32 {
            if self.config.gpr_ignore.contains(&i) {
                continue;
            }
            if a.gpr_snapshot()[i] != b.gpr_snapshot()[i] {
                return Err(DiffError::RegMismatch {
                    reg: i,
                    my: a.gpr_snapshot()[i],
                    qemu: b.gpr_snapshot()[i],
                });
            }
        }

        Ok(())
    }
}
```

---

### 5. 模块依赖关系

```mermaid
graph TB
    subgraph Core["核心抽象层"]
        ExecModel["ExecutionModel<br/>执行模型"]
        StateSnap["StateSnapshot<br/>状态快照"]
        CsrReg["CsrRegister<br/>CSR 抽象"]
        Periph["Peripheral<br/>外设抽象"]
    end

    subgraph Implementations["实现层"]
        SingleCycle["SingleCycle"]
        Pipeline["FiveStagePipeline"]
        CsrBank["CsrBank"]
        Uart["UART"]
        Timer["Timer"]
        GpuDev["GPU"]
        TpuDev["TPU"]
    end

    subgraph Infrastructure["基础设施"]
        Bus["System Bus"]
        DiffTest["DiffTestRunner"]
        GdbServer["GdbServer"]
    end

    ExecModel --> SingleCycle
    ExecModel --> Pipeline
    CsrReg --> CsrBank
    Periph --> Uart
    Periph --> Timer
    Periph --> GpuDev
    Periph --> TpuDev

    Bus --> Periph
    DiffTest --> ExecModel
    DiffTest --> StateSnap
    GdbServer --> ExecModel
```

---

### 6. 扩展点总结

| 扩展点     | 抽象接口            | 扩展方式          | 典型场景               |
| ---------- | ------------------- | ----------------- | ---------------------- |
| 执行模型   | `ExecutionModel`    | 实现 trait        | 单周期调试、流水线优化 |
| CSR 寄存器 | `CsrRegister`       | 实现 trait + 注册 | 自定义 CSR、新扩展     |
| 外设       | `Peripheral`        | 实现 trait + 挂载 | VirtIO、自定义外设     |
| 状态对比   | `StateSnapshot`     | 实现 trait        | DiffTest 不同粒度      |
| 指令扩展   | `Instruction` trait | 实现 trait + 注册 | M/F/D/A 扩展           |

---

## 十三、性能监控

### RISC-V HPM 规范实现

myCPU 实现了符合 RISC-V 硬件性能监控 (HPM) 规范的 CSR 寄存器。

#### 性能计数器 CSR

| CSR 地址    | 名称             | 说明                    |
| ----------- | ---------------- | ----------------------- |
| 0xB00       | mcycle           | 周期计数器低 32 位      |
| 0xB80       | mcycleh          | 周期计数器高 32 位      |
| 0xB02       | minstret         | 指令计数器低 32 位      |
| 0xB82       | minstreth        | 指令计数器高 32 位      |
| 0xB03-0xB1F | mhpmcounter3-31  | 可编程计数器 (低 32 位) |
| 0xB83-0xB9F | mhpmcounter3-31h | 可编程计数器 (高 32 位) |
| 0x323-0x33F | mhpmevent3-31    | 事件选择器              |
| 0x320       | mcountinhibit    | 计数器禁止寄存器        |

#### 支持的性能事件

| 事件 ID | 事件名称             | 说明                    |
| ------- | -------------------- | ----------------------- |
| 0       | None                 | 禁用计数                |
| 1       | Cycles               | CPU 周期                |
| 2       | InstructionsRetired  | 已完成指令              |
| 3       | LoadUseStalls        | Load-Use 暂停周期       |
| 4       | ControlHazards       | 控制冒险 (分支预测错误) |
| 5       | BranchExecuted       | 执行的分支指令          |
| 6       | BranchTaken          | 跳转的分支              |
| 7       | BranchNotTaken       | 未跳转的分支            |
| 15      | BranchMispredictions | 动态预测器误预测次数    |
| 8       | MemoryReads          | 内存读取次数            |
| 9       | MemoryWrites         | 内存写入次数            |
| 10      | AluOperations        | ALU 操作次数            |
| 11      | CsrAccesses          | CSR 访问次数            |
| 12      | InterruptsTaken      | 已处理中断数            |
| 13      | PipelineFlushes      | 流水线冲刷次数          |

### 性能收集器架构

```mermaid
flowchart LR
    subgraph Pipeline["流水线"]
        IF["IF"] --> ID["ID"] --> EX["EX"] --> MEM["MEM"] --> WB["WB"]
    end

    subgraph PerfCollector["PerfCollector"]
        Events["事件记录"]
        Counters["计数器"]
    end

    subgraph CSR["CSR HPM"]
        Mcycle["mcycle"]
        Minstret["minstret"]
        Mhpm["mhpmcounter3-31"]
        Mhpmevent["mhpmevent3-31"]
    end

    Pipeline -->|记录事件| PerfCollector
    PerfCollector -->|更新计数| CSR
```

### 性能报告

通过 `--perf-report` CLI 选项生成性能报告：

```bash
cargo run --release -- run --perf-report program.elf
```

输出示例：

```
╔══════════════════════════════════════════════════════════════╗
║                    Performance Report                         ║
╠══════════════════════════════════════════════════════════════╣
║  Execution Summary                                            ║
╠══════════════════════════════════════════════════════════════╣
║  Cycles:                                          1,234,567  ║
║  Instructions:                                      987,654  ║
║  IPC:                                                 0.80   ║
║  CPI:                                                1.25   ║
║  Efficiency:                                        80.0%   ║
╠══════════════════════════════════════════════════════════════╣
║  Pipeline Hazards                                             ║
╠══════════════════════════════════════════════════════════════╣
║  Load-Use Stalls:                                    12,345  ║
║  Control Hazards:                                     8,765  ║
║  Total Stalls:                                       21,110  ║
║  Stall Rate:                                          1.7%   ║
╠══════════════════════════════════════════════════════════════╣
║  Branch Statistics                                            ║
╠══════════════════════════════════════════════════════════════╣
║  Total Branches:                                     45,678  ║
║  Taken:                                              23,456  ║
║  Not Taken:                                          22,222  ║
║  Prediction Acc:                                     51.4%   ║
╠══════════════════════════════════════════════════════════════╣
║  Memory Statistics                                            ║
╠══════════════════════════════════════════════════════════════╣
║  Memory Reads:                                      123,456  ║
║  Memory Writes:                                      45,678  ║
║  Total Mem Ops:                                     169,134  ║
║  Mem Ops/Instr:                                      0.17   ║
╚══════════════════════════════════════════════════════════════╝
```

### 关键实现文件

| 文件                        | 说明                                                    |
| --------------------------- | ------------------------------------------------------- |
| `src/cpu/csr/perf.rs`       | HPM CSR 实现 (Counter64, Mcycle, Minstret, Mhpmcounter) |
| `src/cpu/perf_collector.rs` | 性能事件收集器                                          |
| `src/perf/report.rs`        | 性能报告格式化输出                                      |

---

## 十四、Linux + Framebuffer 可视化链路（已落地）

为降低 Linux + SDL 演示接入成本，当前已完成“程序写内存帧缓冲 → WebSocket → 前端 Canvas”的闭环：

- 后端命令：`framebuffer <addr> <width> <height> [format]`
- Linux 快捷预设：`fb linux` / `framebuffer linux`
- 预设参数：`addr=0x80E00000`, `width=320`, `height=240`, `format=rgb565`
- 支持格式：`gray8` / `rgb565` / `rgb888`
- 后端统一转换为 `RGBA8888` 字节流，前端直接 `ImageData` 渲染
- 内置 RV32I 帧缓冲写入程序：`visualize --linux-fb-demo --warmup <N>`
- 一键脚本验收：`scripts/run_framebuffer_demo.ps1`（内置 WebSocket 非零像素探针）

这条链路可在不引入复杂 GPU/显示控制器模型的前提下，快速验证 Linux 图形输出路径。

## 十五、后续扩展

- [ ] Zicsr 扩展 (CSR 指令)
- [ ] Zifencei 扩展 (指令缓存刷新)
- [ ] F/D 扩展 (浮点运算)
- [ ] A 扩展 (原子操作)
- [ ] 多核 SMP 支持
- [ ] JTAG 调试接口
- [ ] ~~性能计数器 (PMU)~~ ✅ 已完成
