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
