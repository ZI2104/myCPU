# myCPU - RISC-V 模拟器项目指令

## 项目概述

myCPU 是一个 Rust 实现的 RISC-V RV32I 指令集模拟器，目标是实现：
- 6 级流水线 (pre-IF/IF/ID/EX/MEM/WB)
- M/S/U 三级特权模式
- 完整外设支持 (UART, Timer, PLIC)
- GDB 调试接口
- QEMU DiffTest

## 当前进度

- [x] Phase 1: 基础框架 ✅
- [ ] Phase 2: RV32I 指令集 (40 条)
- [ ] Phase 3: 5 级流水线
- [ ] Phase 4: 特权级与异常
- [ ] Phase 5: 外设与调试

## 项目结构

```
src/
├── lib.rs           # 库入口 + re-exports
├── main.rs          # CLI 入口 (clap)
├── types.rs         # 基础类型 (Addr, Word, Byte, RegIdx)
├── error.rs         # 错误类型 (SimError)
├── traits/          # 核心 trait
│   ├── memory.rs    # Memory trait (read/write)
│   └── peripheral.rs # Peripheral trait
├── memory/          # 内存子系统
│   ├── ram.rs       # RAM
│   ├── rom.rs       # ROM
│   └── bus.rs       # 系统总线 + 地址映射
├── cpu/             # CPU 核心
│   ├── core.rs      # Cpu 结构 + step()/run()
│   ├── registers.rs # 32 通用寄存器
│   ├── pc.rs        # 程序计数器
│   └── state.rs     # CpuState 快照 (DiffTest)
├── instruction/     # 指令译码和执行 (Phase 2)
├── pipeline/        # 流水线实现 (Phase 3)
│   ├── pre_if.rs    # pre-IF 阶段 (预取指)
│   ├── fetch.rs     # IF 阶段 (取指)
│   ├── decode.rs    # ID 阶段 (译码)
│   ├── execute.rs   # EX 阶段 (执行)
│   ├── memory.rs    # MEM 阶段 (内存访问)
│   ├── writeback.rs # WB 阶段 (写回)
│   ├── hazard.rs    # 冒险检测与处理
│   └── forward.rs   # 前递逻辑
├── csr/             # CSR 寄存器 (Phase 4)
├── exception/       # 异常处理 (Phase 4)
├── peripheral/      # 外设 (Phase 5)
├── loader/          # ELF 加载器 (Phase 5)
└── debug/           # 调试接口 (Phase 5)
```

## 流水线架构

### 6 级流水线设计 (pre-IF/IF/ID/EX/MEM/WB)

**关键特性**：
- pre-IF 阶段：伪阶段（组合逻辑），计算 nextPC，发起指令内存读请求
- 同步 RAM 设计：所有内存访问通过 latch，1 周期延迟，支持读保持（Q 端保持上次有效读数据直到新请求）
- pre-IF → IF：指令通过 `InstrFetchLatch` 传递
- EX → MEM：数据通过 `DataReadLatch` 传递
- ID 阶段早期分支解析：分支/跳转在 ID 阶段完成条件判断和目标计算，pre-IF 同周期重定向
- 分支惩罚：1 周期（分支在 ID 解析，仅冲刷 IF 中的错路指令）

#### 流水线寄存器

| 寄存器 | 内容说明 |
| ------ | -------- |
| `InstrFetchLatch` | 指令内存输出寄存器（1 周期延迟，读保持） |
| `ID/EX` | PC, 操作数, 立即数, 控制信号, branch_taken, branch_target |
| `EX/MEM` | ALU 结果, Store 数据, 控制信号 |
| `DataReadLatch` | 数据内存读取结果（1 周期延迟，读保持） |
| `MEM/WB` | 内存数据, ALU 结果, 写回目标 |

#### 内存访问时序

```text
指令内存访问：
pre-IF: PC计算 → 发起读请求
   IF: 从InstrFetchLatch读取指令（延迟1周期）

数据内存访问：
  EX: 计算地址 → 发起读请求
 MEM: 从DataReadLatch读取数据（延迟1周期）
     Store直接写入总线
```

#### 重置行为

- `reset()` 或 `set_pc()` 时，`instr_latch` 预填充
- 模拟硬件复位状态（RAM 已在读）
- 确保第一条指令立即可用

#### HazardUnit 与前递

- Load-Use 检测逻辑不变
- **新增**：ID 阶段前递（`apply_forwarding_for_decode`）从 ex_mem/mem_wb 前递操作数到 ID
- **新增**：分支数据冒险 stall（id_ex 写入分支源寄存器 / ex_mem load 写入分支源寄存器）
- 控制冒险冲刷移到 `clock()` 中（使用 `new_id_ex.branch_taken`）

## 编码规范

### 不可变性
- **始终创建新对象，绝不修改现有对象**
- 使用 `update(obj, field, value)` 模式，而非 `modify(obj, field, value)`

### 文件组织
- 单个文件 200-400 行，最多 800 行
- 按功能/领域组织，而非按类型

### 错误处理
- 使用 `SimError` 枚举定义所有错误
- 使用 `Result<T>` 作为返回类型
- 在系统边界验证输入

### 指令常量定义
- **禁止硬编码指令值**，必须使用命名常量
- 常量定义在 `src/cpu/core.rs` 的 `instr` 模块中
- RISC-V 标准 NOP 是 `0x00000013` (`addi x0, x0, 0`)，**不是**全 0
```rust
mod instr {
    pub const NOP: u32 = 0x00000013;           // addi x0, x0, 0
    pub const INVALID_PATTERN: u32 = 0xFFFFFFFF; // 未初始化内存标记
}
```

### PC 增量时机
- **PC 在指令执行成功后才递增**，而非执行前
- 异常发生时 PC 指向问题指令本身，无需回退
- 分支/跳转指令直接设置 PC 目标值
```rust
fn execute(&mut self, instruction: u32) -> Result<()> {
    // 先检查有效性（失败时 PC 不变）
    if is_invalid(instruction) {
        return Err(...); // PC 指向当前指令
    }
    // 执行指令逻辑...
    // 最后递增 PC
    self.pc.increment();
    Ok(())
}
```

### 接口类型一致性
- **所有内存读取方法返回包装类型** (`Word`, `Byte`, `Half`)，而非原始类型
- 保持 API 风格统一，减少使用者的认知负担
```rust
// 正确：返回包装类型
pub fn read_word(&self, addr: Addr) -> Result<Word>
pub fn read_byte(&self, addr: Addr) -> Result<Byte>
pub fn read_half(&self, addr: Addr) -> Result<Half>

// 错误：混合返回原始类型
pub fn read_byte(&self, addr: Addr) -> Result<u8>  // ❌
```

### 内存对齐检查
- **多字节访问必须检查对齐**：half-word 需要 2 字节对齐，word 需要 4 字节对齐
- 对齐错误返回 `SimError::MemoryAlignment`
- 为 Phase 4 异常处理（Load/Store Address Misaligned）做准备
```rust
pub fn read_word(&self, addr: Addr) -> Result<Word> {
    if !addr.is_aligned(4) {
        return Err(SimError::MemoryAlignment { addr, size: 4, alignment: 4 });
    }
    // ...
}
```

### 同步内存访问
- **所有内存访问必须通过 latch**：实现 1 周期延迟
- **指令内存**：pre-IF 发起请求，IF 阶段从 `InstrFetchLatch` 读取
- **数据内存**：EX 阶段发起请求，MEM 阶段从 `DataReadLatch` 读取
- **Store 操作**：MEM 阶段直接写入总线（无需 latch）
- **读保持**：latch 在无新读请求时保持上次有效数据，模拟同步 RAM Q 端保持行为

```rust
// 指令内存访问模式
pub fn fetch_instruction(&mut self, addr: Addr) -> Option<u32> {
    // pre-IF 阶段：发起请求
    self.memory.request_read(addr);
    // IF 阶段：从 latch 读取
    self.instr_latch.read()
}

// 数据内存访问模式
pub fn read_data(&mut self, addr: Addr) -> Result<Word> {
    // EX 阶段：发起请求
    self.memory.request_read(addr);
    // MEM 阶段：从 latch 读取
    self.data_latch.read().ok_or(SimError::MemoryNotReady)
}
```

### 测试
- 每个模块包含 `#[cfg(test)]` 单元测试
- 目标覆盖率: 80%+

## 常用命令

```bash
# 构建
cargo build --release

# 测试
cargo test

# 运行
cargo run -- program.bin

# 带日志运行
RUST_LOG=debug cargo run -- program.bin

# 检查
cargo clippy

# 格式化
cargo fmt
```

## 关键类型速查

| 类型               | 说明              |
| ------------------ | ----------------- |
| `Addr(u32)`        | 32 位地址         |
| `Word(u32)`        | 32 位数据字       |
| `Byte(u8)`         | 8 位字节          |
| `RegIdx(u8)`       | 寄存器索引 (0-31) |
| `Memory` trait     | 内存访问接口      |
| `Peripheral` trait | 外设接口          |
| `Accelerator` trait | 加速器接口 (GPU/TPU) |
| `KernelType` enum  | GPU/TPU 内核类型 |
| `Cpu`              | CPU 核心结构      |
| `CpuState`         | CPU 状态快照      |
| `InstrFetchLatch`  | 指令读取 latch    |
| `DataReadLatch`    | 数据读取 latch    |

## 文档位置

- 架构设计: `docs/design/ARCHITECTURE.md`
- 开发路线: `docs/design/ROADMAP.md`
- GPU/TPU API: `docs/guides/GPU_TPU_API.md`
- 项目说明: `README.md`

## 下一步

当前 Phase 1-6 已完成，GPU/TPU 模拟加速器（Phase 5.2）已实现。可选方向：

1. 在模拟器上运行 miniOS / Linux 最小系统
2. 形成: 自己写 CPU → 自己写 OS → 自己跑 Linux 的完整闭环
3. Hybrid Offload 路径（异步队列 + CPU 回退）
