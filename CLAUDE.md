# myCPU - RISC-V 模拟器项目指令

## 项目概述

myCPU 是一个 Rust 实现的 RISC-V RV32I 指令集模拟器，目标是实现：
- 5 级流水线 (IF/ID/EX/MEM/WB)
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
├── csr/             # CSR 寄存器 (Phase 4)
├── exception/       # 异常处理 (Phase 4)
├── peripheral/      # 外设 (Phase 5)
├── loader/          # ELF 加载器 (Phase 5)
└── debug/           # 调试接口 (Phase 5)
```

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
| `Cpu`              | CPU 核心结构      |
| `CpuState`         | CPU 状态快照      |

## 文档位置

- 架构设计: `docs/ARCHITECTURE.md`
- 开发路线: `docs/ROADMAP.md`
- 项目说明: `README.md`

## 下一步

当前 Phase 1 已完成，下一步是实现 Phase 2 的 RV32I 指令集：

1. 创建 `src/instruction/` 目录
2. 实现指令译码器 (`decoder.rs`)
3. 实现 R/I/S/B/U/J/System 类型指令
4. 在 `Cpu::execute()` 中调用指令执行
