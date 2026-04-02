# 性能监控设计

## 目标

为 myCPU 添加硬件性能监控 (HPM) 功能，支持：
- RISC-V 标准性能计数器
- IPC (Instructions Per Cycle) 计算
- 流水线效率分析
- 基准测试程序运行

## RISC-V HPM 规范

### 性能计数器 CSR

| CSR 地址 | 名称 | 说明 |
|----------|------|------|
| 0xB00 | mcycle | 机器模式周期计数器 |
| 0xB02 | minstret | 机器模式已完成指令计数器 |
| 0xB03-B1F | mhpmcounter3-31 | 机器模式可编程事件计数器 |
| 0x323-33F | mhpmevent3-31 | 事件选择寄存器 |

### 实现计划

#### 第一阶段：基础计数器

```rust
// src/cpu/csr/performance.rs

/// 性能计数器
pub struct PerformanceCounters {
    /// 周期计数
    pub mcycle: u64,
    /// 指令计数
    pub minstret: u64,
    /// 自定义事件计数器
    pub mhpmcounters: [u64; 29],
    /// 事件选择器
    pub mhpmevents: [u32; 29],
}

/// 可监控的事件类型
#[derive(Debug, Clone, Copy)]
pub enum PerfEvent {
    /// 周期数
    Cycles = 0,
    /// 已完成指令数
    InstructionsRetired = 1,
    /// Load-Use 暂停
    LoadUseStalls = 2,
    /// 分支预测错误
    BranchMispredictions = 3,
    /// 数据冒险
    DataHazards = 4,
    /// 控制冒险
    ControlHazards = 5,
    /// 内存访问
    MemoryAccesses = 6,
    /// 缓存命中 (如果有缓存)
    CacheHits = 7,
    /// 缓存未命中
    CacheMisses = 8,
}

impl PerformanceCounters {
    pub fn new() -> Self {
        Self {
            mcycle: 0,
            minstret: 0,
            mhpmcounters: [0; 29],
            mhpmevents: [0; 29],
        }
    }
    
    /// 每个时钟周期调用
    pub fn tick(&mut self) {
        self.mcycle = self.mcycle.wrapping_add(1);
        
        // 检查所有可编程计数器
        for i in 0..29 {
            if self.mhpmevents[i] & 1 != 0 {
                // 检查事件是否发生
                // 这里需要根据事件类型递增计数器
            }
        }
    }
    
    /// 指令完成时调用
    pub fn instruction_retired(&mut self) {
        self.minstret = self.minstret.wrapping_add(1);
    }
    
    /// 记录事件
    pub fn record_event(&mut self, event: PerfEvent) {
        match event {
            PerfEvent::Cycles => {}, // mcycle 在 tick() 中处理
            PerfEvent::InstructionsRetired => self.instruction_retired(),
            PerfEvent::LoadUseStalls => self.increment_counter(0), // mhpmcounter3
            PerfEvent::BranchMispredictions => self.increment_counter(1),
            PerfEvent::DataHazards => self.increment_counter(2),
            PerfEvent::ControlHazards => self.increment_counter(3),
            _ => {}
        }
    }
    
    fn increment_counter(&mut self, index: usize) {
        if index < 29 {
            self.mhpmcounters[index] = self.mhpmcounters[index].wrapping_add(1);
        }
    }
    
    /// 计算 IPC
    pub fn ipc(&self) -> f64 {
        if self.mcycle == 0 {
            0.0
        } else {
            self.minstret as f64 / self.mcycle as f64
        }
    }
    
    /// 获取分支预测准确率 (如果已配置)
    pub fn branch_accuracy(&self) -> Option<f64> {
        let branches = self.mhpmcounters[3]; // 假设用 counter4 记录总分支数
        let mispredictions = self.mhpmcounters[1];
        
        if branches == 0 {
            None
        } else {
            let correct = branches.saturating_sub(mispredictions);
            Some(correct as f64 / branches as f64 * 100.0)
        }
    }
}
```

#### 第二阶段：流水线集成

```rust
// 在流水线各阶段添加事件记录

// src/cpu/pipeline/mod.rs

impl FiveStagePipeline {
    pub fn step(&mut self) -> Result<CpuState> {
        // 1. 记录周期
        self.counters.tick();
        
        // 2. WB 阶段 - 记录完成指令
        if let Some(_wb) = &self.stages.wb {
            self.counters.instruction_retired();
        }
        
        // 3. 检测冒险并记录
        if self.hazard_unit.has_load_use_hazard() {
            self.counters.record_event(PerfEvent::LoadUseStalls);
        }
        
        if self.hazard_unit.has_data_hazard() {
            self.counters.record_event(PerfEvent::DataHazards);
        }
        
        // 4. 分支预测结果
        if self.branch_mispredicted {
            self.counters.record_event(PerfEvent::BranchMispredictions);
        }
        
        // ... 执行流水线各阶段
    }
}
```

#### 第三阶段：CSR 集成

```rust
// src/cpu/csr/performance.rs

impl CsrRegister for McycleCsr {
    fn address(&self) -> u16 { 0xB00 }
    
    fn read(&self, mode: PrivilegeMode) -> Result<u32, CsrError> {
        // 返回低 32 位
        Ok((self.counters.mcycle & 0xFFFFFFFF) as u32)
    }
    
    fn write(&mut self, value: u32, _mode: PrivilegeMode) -> Result<(), CsrError> {
        // 只更新低 32 位
        self.counters.mcycle = (self.counters.mcycle & 0xFFFFFFFF00000000) | value as u64;
        Ok(())
    }
}

// mcycleh (高 32 位)
impl CsrRegister for McyclehCsr {
    fn address(&self) -> u16 { 0xB80 }
    
    fn read(&self, _mode: PrivilegeMode) -> Result<u32, CsrError> {
        Ok((self.counters.mcycle >> 32) as u32)
    }
}
```

## 关键性能指标

### IPC (Instructions Per Cycle)

```
IPC = 已完成指令数 / 总周期数

理想值 (无暂停):
- 单周期 CPU: IPC = 1.0
- 5 级流水线: IPC ≈ 1.0 (稳定状态)

实际值:
- 简单程序: IPC ≈ 0.8-0.95
- 复杂程序: IPC ≈ 0.5-0.8
- 大量分支: IPC ≈ 0.3-0.5
```

### 分支预测准确率

```
准确率 = 正确预测 / 总分支数 × 100%

静态预测 (Predict Not Taken):
- 预期准确率: 50-70%

BTFN (Backward Taken, Forward Not Taken):
- 预期准确率: 70-85%

2-bit 动态预测:
- 预期准确率: 85-95%
```

### 流水线效率

```
效率 = (总周期 - 暂停周期) / 总周期 × 100%

暂停来源:
1. Load-Use 冒险: 约 10-20% 周期
2. 控制冒险: 约 5-15% 周期
3. 结构冒险: 取决于内存架构
```

## 基准测试程序

### 1. CoreMark

```bash
# 下载 CoreMark
git clone https://github.com/eembc/coremark.git

# 交叉编译
riscv32-unknown-elf-gcc -O2 -march=rv32im -o coremark.elf \
    coremark_main.c core_list_join.c core_matrix.c \
    core_state.c core_util.c

# 运行
mycpu run coremark.elf
```

### 2. Dhrystone

```c
// dhry_1.c - 简化版 Dhrystone
#include <stdio.h>

#define LOOPS 100000

typedef struct {
    int Int_1;
    int Int_2;
} Rec_Type;

int main(void) {
    Rec_Type Rec_1, Rec_2;
    int Int_1_Ref, Int_2_Ref;
    
    for (int i = 0; i < LOOPS; i++) {
        // ... Dhrystone 操作
    }
    
    return 0;
}
```

### 3. 自定义测试

```asm
# test_performance.s

.section .text
.globl _start

_start:
    li x1, 0          # 初始化计数器
    li x2, 1000       # 循环次数
    
loop:
    addi x1, x1, 1    # 递增
    add x3, x1, x2    # 简单算术
    beq x1, x2, done  # 条件分支
    j loop            # 无条件分支
    
done:
    # 输出结果
    li x10, 1         # print syscall
    ecall
    
    # 退出
    li x10, 10        # exit syscall
    ecall
```

## 性能报告格式

```rust
// src/performance/report.rs

pub struct PerformanceReport {
    pub program: String,
    pub duration_ms: u64,
    pub cycles: u64,
    pub instructions: u64,
    pub ipc: f64,
    pub stalls: u64,
    pub branch_predictions: BranchPredictionStats,
    pub memory_accesses: MemoryAccessStats,
}

impl PerformanceReport {
    pub fn format(&self) -> String {
        format!(
            r#"
╔══════════════════════════════════════════════════════════════╗
║                    Performance Report                        ║
╠══════════════════════════════════════════════════════════════╣
║ Program: {:<50}          ║
║ Duration: {} ms                                              ║
╠══════════════════════════════════════════════════════════════╣
║ Core Metrics                                                 ║
║   Cycles: {:>15}                                            ║
║   Instructions: {:>15}                                      ║
║   IPC: {:>20.3}                                             ║
║   Stalls: {:>15} ({:.1}%)                                   ║
╠══════════════════════════════════════════════════════════════╣
║ Branch Prediction                                            ║
║   Total: {:>15}                                             ║
║   Correct: {:>15}                                           ║
║   Accuracy: {:>14.1}%                                        ║
╠══════════════════════════════════════════════════════════════╣
║ Memory Access                                                ║
║   Reads: {:>15}                                             ║
║   Writes: {:>15}                                            ║
╚══════════════════════════════════════════════════════════════╝
"#,
            self.program,
            self.duration_ms,
            format_number(self.cycles),
            format_number(self.instructions),
            self.ipc,
            format_number(self.stalls),
            self.stalls as f64 / self.cycles as f64 * 100.0,
            format_number(self.branch_predictions.total),
            format_number(self.branch_predictions.correct),
            self.branch_predictions.accuracy(),
            format_number(self.memory_accesses.reads),
            format_number(self.memory_accesses.writes),
        )
    }
}
```

## CLI 命令

```bash
# 运行并输出性能报告
mycpu run --perf-report program.elf

# 只输出 IPC
mycpu run --ipc program.elf

# 运行基准测试
mycpu benchmark coremark.elf

# 比较两个配置的性能
mycpu compare --config single-cycle,pipeline program.elf
```

## 目录结构

```
src/
├── cpu/
│   └── csr/
│       └── performance.rs    # 性能计数器 CSR
├── performance/
│   ├── mod.rs               # 性能模块入口
│   ├── counters.rs          # 计数器实现
│   ├── report.rs            # 报告生成
│   └── benchmark.rs         # 基准测试框架
└── tests/
    └── performance/
        ├── test_counters.rs
        └── benchmarks/
            ├── coremark/
            └── dhrystone/
```

## 验证方法

1. **单元测试**: 测试计数器正确递增
2. **集成测试**: 运行简单程序，验证 IPC
3. **对比测试**: 与 QEMU 对比计数器值
4. **手动验证**: 运行已知结果的程序

```bash
# 运行性能测试
cargo test --features perf-counters

# 运行基准测试
cargo run --release -- benchmark tests/benchmarks/coremark.elf
```
