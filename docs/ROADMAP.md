# myCPU 开发路线图

## 开发阶段总览

| 阶段 | 内容 | 产出 | 预计周期 |
|------|------|------|----------|
| Phase 1 | 基础框架 | 可编译的项目骨架 | 1-2 周 |
| Phase 2 | 指令集 | 可运行简单程序 | 2-3 周 |
| Phase 3 | 流水线 | 5 级流水线 + 冒险处理 | 2 周 |
| Phase 4 | 特权级 | M/S/U 模式 + 异常中断 | 2 周 |
| Phase 5 | 外设 | UART + 调试器 | 1-2 周 |
| Phase 6 | 加分项 | MMU + 扩展指令 | 可选 |

---

## Phase 1: 基础框架 ✅ 完成

### 目标
搭建项目骨架，实现内存和寄存器基础模块。

### 任务清单

- [x] 项目初始化 (Cargo 配置)
- [x] 基础 trait 定义
  - [x] `Memory` trait (read/write)
  - [x] `Peripheral` trait
- [x] 内存模块
  - [x] RAM 实现
  - [x] ROM 实现
  - [x] 总线 (Bus) 实现
- [x] CPU 寄存器组
  - [x] 通用寄存器 x0-x31
  - [x] PC 寄存器
- [x] 主循环框架
  - [x] 单周期执行循环
  - [x] 时钟计数
- [x] 错误处理
  - [x] SimError 类型定义
  - [x] Result 类型别名

### 产出

- ✅ 可编译运行的空模拟器
- ✅ 基础内存读写测试通过 (35 个测试全部通过)

---

## Phase 2: 指令集实现

### 目标
实现 RV32I 基础指令集 (40 条)，可运行简单程序。

### RV32I 指令清单

#### R-type (10 条)
- [ ] ADD  - 加法
- [ ] SUB  - 减法
- [ ] AND  - 与
- [ ] OR   - 或
- [ ] XOR  - 异或
- [ ] SLL  - 逻辑左移
- [ ] SRL  - 逻辑右移
- [ ] SRA  - 算术右移
- [ ] SLT  - 有符号小于比较
- [ ] SLTU - 无符号小于比较

#### I-type (14 条)
- [ ] ADDI  - 加立即数
- [ ] ANDI  - 与立即数
- [ ] ORI   - 或立即数
- [ ] XORI  - 异或立即数
- [ ] SLTI  - 有符号小于比较立即数
- [ ] SLTIU - 无符号小于比较立即数
- [ ] SLLI  - 逻辑左移立即数
- [ ] SRLI  - 逻辑右移立即数
- [ ] SRAI  - 算术右移立即数
- [ ] LB    - 加载字节
- [ ] LH    - 加载半字
- [ ] LW    - 加载字
- [ ] LBU   - 加载无符号字节
- [ ] LHU   - 加载无符号半字

#### S-type (3 条)
- [ ] SB - 存储字节
- [ ] SH - 存储半字
- [ ] SW - 存储字

#### B-type (6 条)
- [ ] BEQ  - 相等跳转
- [ ] BNE  - 不等跳转
- [ ] BLT  - 有符号小于跳转
- [ ] BGE  - 有符号大于等于跳转
- [ ] BLTU - 无符号小于跳转
- [ ] BGEU - 无符号大于等于跳转

#### U-type (2 条)
- [ ] LUI   - 加载高位立即数
- [ ] AUIPC - PC 加高位立即数

#### J-type (2 条)
- [ ] JAL  - 跳转并链接
- [ ] JALR - 跳转并链接寄存器

#### System (3 条)
- [ ] ECALL - 环境调用
- [ ] EBREAK - 断点
- [ ] FENCE - 内存屏障

### 产出
- 所有 RV32I 指令测试通过
- 可运行简单算术程序

---

## Phase 3: 流水线实现

### 目标
实现 5 级流水线，处理数据冒险和控制冒险。

### 任务清单

- [ ] 流水线寄存器
  - [ ] IF/ID 寄存器
  - [ ] ID/EX 寄存器
  - [ ] EX/MEM 寄存器
  - [ ] MEM/WB 寄存器
- [ ] 各阶段实现
  - [ ] IF (Instruction Fetch)
  - [ ] ID (Instruction Decode)
  - [ ] EX (Execute)
  - [ ] MEM (Memory Access)
  - [ ] WB (Write Back)
- [ ] 冒险处理
  - [ ] 数据冒险检测
  - [ ] 前递逻辑 (EX/EX, MEM/EX)
  - [ ] Load-Use 暂停
  - [ ] 控制冒险 - 静态预测
  - [ ] 分支冲刷

### 产出
- 流水线测试通过
- 性能对比单周期提升

---

## Phase 4: 特权级与异常

### 目标
实现 M/S/U 三级特权模式，支持异常和中断处理。

### 任务清单

- [ ] CSR 寄存器
  - [ ] M-mode: mstatus, mtvec, mepc, mcause, mie, mip, mscratch
  - [ ] S-mode: sstatus, stvec, sepc, scause, sie, sip, sscratch
  - [ ] U-mode: ustatus, utvec, uepc, ucause
- [ ] 特权级切换
  - [ ] ecall 指令
  - [ ] mret/sret/uret 指令
  - [ ] 特权级检查
- [ ] 异常处理
  - [ ] 异常入口 (xtvec)
  - [ ] 上下文保存/恢复
  - [ ] 异常返回
- [ ] 中断系统
  - [ ] CLINT (时钟中断)
  - [ ] PLIC (外部中断)
  - [ ] 中断委托 (M → S)

### 产出
- 特权级切换测试通过
- 可处理异常和中断

---

## Phase 5: 外设与调试

### 目标
实现 UART 串口输出，支持程序加载，实现 GDB 调试接口。

### 任务清单

- [ ] UART (NS16550A 兼容)
  - [ ] 发送/接收寄存器
  - [ ] 状态寄存器
  - [ ] 中断支持
- [ ] Timer
  - [ ] mtime 寄存器
  - [ ] mtimecmp 寄存器
  - [ ] 时钟中断
- [ ] ELF 加载器
  - [ ] 解析 ELF 头 (使用 goblin crate)
  - [ ] 加载程序段
  - [ ] 设置入口点
- [ ] GDB Remote Protocol (核心)
  - [ ] TCP Server 基础框架
  - [ ] 基础命令: ?, g, G, m, M, c, s
  - [ ] 断点支持: Z0, z0
  - [ ] 查询命令: qSupported, qAttached
  - [ ] VSCode 集成配置
- [ ] DiffTest 框架
  - [ ] QEMU GDB Stub 集成
  - [ ] 状态对比逻辑
  - [ ] 错误报告与日志

### 产出
- 可串口输出 "Hello World"
- 可加载并运行 ELF 程序
- 可通过 GDB/VSCode 调试
- DiffTest 自动化测试可用

---

## Phase 6: 加分项 (可选)

### M 扩展 (乘除法)
- [ ] MUL, MULH, MULHSU, MULHU
- [ ] DIV, DIVU, REM, REMU

### C 扩展 (压缩指令)
- [ ] 16 位压缩指令解码
- [ ] 常用指令的压缩形式

### Sv32 分页
- [ ] 页表结构
- [ ] TLB 缓存
- [ ] 地址翻译
- [ ] 页错误异常

### 多核支持
- [ ] 多个 Hart (硬件线程)
- [ ] 核间中断 (IPI)
- [ ] 共享内存

---

## 测试计划

### DiffTest 差分测试 (核心)

与 QEMU 逐指令对比，确保行为一致：

- [ ] QEMU GDB Stub 集成
  - [ ] 启动 QEMU 并连接 GDB 端口
  - [ ] 同步加载测试程序到两个模拟器
- [ ] 状态对比
  - [ ] 通用寄存器 (x1-x31)
  - [ ] PC 寄存器
  - [ ] 关键 CSR (mstatus, mepc, mcause 等)
- [ ] 错误报告
  - [ ] 差异发生时的详细日志
  - [ ] 最近执行的指令历史
  - [ ] 寄存器/内存状态快照

### 单元测试

- 每条指令独立测试 (配合 DiffTest)
- 边界值测试 (溢出、符号扩展等)
- 异常情况测试 (非法指令、地址错误等)

### 集成测试

- riscv-tests 官方测试套件
- 流水线正确性测试
- 特权级切换测试

### 端到端测试

- 运行真实程序 (CoreMark, Rust hello world)
- 性能基准测试 (IPC 统计)
