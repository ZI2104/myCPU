# 前端可视化技术方案

## 目标

为 myCPU RISC-V 模拟器创建 Web 前端可视化界面，直观展示：
- 5 级流水线指令流动
- 寄存器状态变化
- 内存访问模式
- 性能指标实时更新

## 技术选型

### 推荐技术栈

```
┌─────────────────────────────────────────────────────────┐
│                    Frontend (Browser)                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │   React     │  │   D3.js     │  │  WebSocket  │     │
│  │   UI 框架   │  │  可视化库   │  │  实时通信   │     │
│  └─────────────┘  └─────────────┘  └─────────────┘     │
└─────────────────────────────────────────────────────────┘
                          ▲
                          │ WebSocket
                          ▼
┌─────────────────────────────────────────────────────────┐
│                    Backend (Rust)                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │  Simulator  │  │  State      │  │  WebSocket  │     │
│  │    Core     │  │  Snapshot   │  │   Server    │     │
│  └─────────────┘  └─────────────┘  └─────────────┘     │
└─────────────────────────────────────────────────────────┘
```

### 技术选择理由

| 技术     | 选择                     | 理由                     |
| -------- | ------------------------ | ------------------------ |
| 前端框架 | React                    | 生态成熟，组件丰富       |
| 可视化库 | D3.js                    | SVG 动画灵活，适合硬件图 |
| 实时通信 | WebSocket                | 双向通信，低延迟         |
| 后端     | Rust + tokio-tungstenite | 复用现有代码，高性能     |

### 替代方案

如果时间有限，可以选择更简单的方案：

1. **纯 HTTP 轮询**: 前端定时请求 `/api/state`
2. **静态页面生成**: 预先生成 HTML，JavaScript 直接渲染
3. **使用现成工具**: 集成 QtRVSim 或 RISC-V Visualizer

## 架构设计

### 1. 状态导出 API

```rust
// src/visualize/mod.rs

/// CPU 状态快照 (用于前端可视化)
#[derive(Debug, Clone, Serialize)]
pub struct CpuSnapshot {
    /// 通用寄存器 x0-x31
    pub registers: [u32; 32],
    /// PC 寄存器
    pub pc: u32,
    /// 当前特权级
    pub privilege: String,
    /// 流水线状态
    pub pipeline: PipelineSnapshot,
    /// 性能计数器
    pub counters: PerformanceCounters,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineSnapshot {
    /// IF 阶段
    pub if_stage: Option<IfStageInfo>,
    /// ID 阶段
    pub id_stage: Option<IdStageInfo>,
    /// EX 阶段
    pub ex_stage: Option<ExStageInfo>,
    /// MEM 阶段
    pub mem_stage: Option<MemStageInfo>,
    /// WB 阶段
    pub wb_stage: Option<WbStageInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerformanceCounters {
    /// 总周期数
    pub cycles: u64,
    /// 已完成指令数
    pub instructions: u64,
    /// IPC (Instructions Per Cycle)
    pub ipc: f64,
    /// 分支预测准确率
    pub branch_accuracy: Option<f64>,
    /// 流水线暂停次数
    pub stalls: u64,
}
```

### 2. WebSocket 服务器

```rust
// src/visualize/server.rs

use tokio::net::TcpListener;
use tokio_tungstenite::WebSocketStream;

pub struct VisualizeServer {
    cpu: Arc<Mutex<Cpu>>,
    clients: Vec<WebSocketStream<TcpStream>>,
}

impl VisualizeServer {
    pub async fn start(port: u16) -> Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        
        loop {
            let (stream, _) = listener.accept().await?;
            let ws = accept_async(stream).await?;
            // 处理新客户端连接
            self.handle_client(ws).await;
        }
    }
    
    /// 每次执行后广播状态更新
    pub async fn broadcast_snapshot(&mut self, snapshot: &CpuSnapshot) {
        let json = serde_json::to_string(snapshot).unwrap();
        for client in &mut self.clients {
            client.send(Message::Text(json.clone())).await.ok();
        }
    }
}
```

### 3. 前端组件设计

```typescript
// frontend/src/components/PipelineVisualizer.tsx

interface PipelineVisualizerProps {
  snapshot: CpuSnapshot;
}

const PipelineVisualizer: React.FC<PipelineVisualizerProps> = ({ snapshot }) => {
  return (
    <div className="pipeline-container">
      <PipelineStage 
        name="IF" 
        instruction={snapshot.pipeline.if_stage?.instruction}
        active={snapshot.pipeline.if_stage !== null}
      />
      <Arrow direction="right" animated={true} />
      <PipelineStage 
        name="ID" 
        instruction={snapshot.pipeline.id_stage?.instruction}
        active={snapshot.pipeline.id_stage !== null}
      />
      {/* ... 其他阶段 */}
    </div>
  );
};
```

## 可视化内容

### 1. 流水线动画

```
┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐
│    IF   │───▶│    ID   │───▶│    EX   │───▶│   MEM   │───▶│    WB   │
│  取指   │    │  译码   │    │  执行   │    │ 内存访问 │    │  写回   │
└─────────┘    └─────────┘    └─────────┘    └─────────┘    └─────────┘
     │              │              │              │              │
     ▼              ▼              ▼              ▼              ▼
  PC=0x8000      rd=x1         ALU out      Mem[addr]      rf[rd]
  instr=ADD     rs1=x2        =0x1234     =0x5678      ←0x1234
```

**动画效果**:
- 指令从左到右流动
- 暂停时冒泡 (bubble)
- 前递路径高亮显示
- 数据冒险红色警告

### 2. 寄存器面板

```
┌─────────────────────────────────────┐
│          General Registers          │
├─────────┬───────────┬──────────────┤
│  x0     │  0x00000000 │  (zero)      │
│  x1     │  0x80000100 │  ← modified │
│  x2     │  0x10000000 │              │
│  ...    │  ...        │              │
│  x31    │  0x00000000 │              │
└─────────┴───────────┴──────────────┘
```

**交互功能**:
- 点击寄存器查看历史值
- 修改的寄存器高亮
- 支持 16 进制/10 进制切换

### 3. 内存视图

```
┌─────────────────────────────────────┐
│         Memory View (0x80000000)    │
├─────────┬─────────┬────────────────┤
│ Address │   Hex   │   ASCII        │
├─────────┼─────────┼────────────────┤
│ 00      │ 13 00   │ ....           │
│ 04      │ 97 01   │ ....           │
│ 08      │ 00 00   │ ..             │
└─────────┴─────────┴────────────────┘
```

### 4. 性能仪表盘

```
┌─────────────────────────────────────┐
│      Performance Dashboard          │
├─────────────────────────────────────┤
│  IPC: 0.85  ████████░░  (85%)       │
│  Branch Accuracy: 92%               │
│  Stalls: 234                         │
│  Cycles: 10,234                      │
│  Instructions: 8,697                 │
└─────────────────────────────────────┘
```

## 实现步骤（第4-8周映射）

### 第4周：后端 API 基础

1. 创建 `src/visualize/mod.rs`
2. 实现 `CpuSnapshot` 结构
3. 添加 `cpu.snapshot()` 方法
4. 实现 WebSocket 服务器

### 第5周：前端基础

1. 初始化 React 项目 (`frontend/`)
2. 实现 WebSocket 连接
3. 创建基础组件 (RegisterPanel, MemoryView)

### 第6-7周：流水线可视化

1. D3.js 流水线动画
2. 前递路径可视化
3. 冒险检测高亮

### 第8周：集成与优化

1. 控制面板 (运行/暂停/单步)
2. 性能图表
3. 代码编辑器集成

## 目录结构

```
myCPU/
├── src/
│   └── visualize/
│       ├── mod.rs           # 可视化模块入口
│       ├── snapshot.rs      # 状态快照定义
│       ├── server.rs        # WebSocket 服务器
│       └── api.rs           # HTTP API
├── frontend/                # React 前端
│   ├── src/
│   │   ├── components/
│   │   │   ├── PipelineVisualizer.tsx
│   │   │   ├── RegisterPanel.tsx
│   │   │   ├── MemoryView.tsx
│   │   │   └── PerformanceDashboard.tsx
│   │   ├── hooks/
│   │   │   └── useWebSocket.ts
│   │   ├── App.tsx
│   │   └── main.tsx
│   ├── package.json
│   └── vite.config.ts
└── docs/
    └── VISUALIZATION_DESIGN.md
```

## 参考项目

1. **RISC-V Visual Simulator**: https://risc-v-cpu-visualizer.vercel.app/
2. **QtRVSim**: https://github.com/cvut/qtrvsim
3. **WebMIPS**: https://webmips.diism.unisi.it/

## 验证方法

1. 启动后端: `cargo run -- visualize`
2. 启动前端: `cd frontend && npm run dev`
3. 浏览器访问: http://localhost:5173
4. 加载程序，观察流水线动画
5. 单步执行，验证状态同步

## 已知问题与修复记录

更多集中记录请参见：`docs/development/ISSUES_AND_FIXES.md`（按问题域组织的长期维护记录）。

### Reset 与 run_loop 的竞态导致 Reset 后 IF-stage PC 显示不正确

现象：在没有加载程序或低地址内存未映射的 demo 模式下，执行 `reset` 后前端有时会显示 IF 阶段已 advance（例如显示 0x001C 而非期望的 0x0018），并且可见周期计数列（Cn）没有从 C0 重新开始；在某些情况下 run_loop 因 MemoryOutOfBounds 不断报错并持续广播错误快照，造成界面卡住或显示混乱。

根因：后端的 `run_loop` 在独占 `cpu.clock()` 的同时可能与外部命令（如 `reset`）并发执行，且当未映射内存被访问时会返回 MemoryOutOfBounds 导致广播仍然继续；同时 Reset 路径最初广播的 snapshot 可能包含已 advance 的 IF-stage 状态，前端按 cycle 去重/合并历史后会保留错误帧。

修复要点（已在实现中）：

- 在 `src/visualize/server.rs` 中使用全局 clock 锁序列化 `run_loop` 与命令处理，避免 Reset 与 `cpu.clock()` 并发。
- 对 Reset 请求，构造并广播一个 modified snapshot：强制把 top-level `pc` 和 `pipeline.if_stage.pc` 设为记录的 `initial_pc`，并把 perf 计数器（`perf.cycles`/`instructions`/`ipc`/`stalls` 等）清零，以便前端列号从 C0 重新开始并立即看到预期的 IF 状态。
- 在 `run_loop` 的 `cpu.clock()` 周围增加错误处理：当遇到 `MemoryOutOfBounds`（低地址访问且可在 demo 中自动修复时）自动附加一段 NOP 填充的 RAM 来避免一直报错；对于无法恢复的错误则将 CPU halt 并广播最终 snapshot，避免 busy-loop。
- 前端继续保持按 `perf.cycles` 去重/合并策略，因此后端发布的 authoritative modified snapshot 将成为 UI 的正确来源。

影响与注意事项：

- 该修复偏向于提高 demo/可视化的健壮性（降低因未装载程序或内存未映射而导致的 UI 错乱）。在生产级别的模拟运行（加载完整 ELF/映射）下，仍建议保证程序/映像正确装载以避免自动附加 RAM 的行为。
- 如果你在使用过程中仍看到 Reset 后显示不一致，请把 Reset 前后后端日志（包含 `[visualize]` / `[visualize::run_loop]` 日志）以及前端收到的 Reset 前后一条 snapshot JSON 发给维护者以便进一步定位。

文件参考：`src/visualize/server.rs`

