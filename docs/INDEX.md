# 文档索引（Documentation Index）

本文档用于提供 `docs/` 的统一入口。目标是：

- **高召回**：同主题保留"概览 + 详版 + 历史快照"多层信息；
- **低迷路**：先给入口文档，再给专题文档，再给历史快照路径。

## 目录结构

```text
docs/
├── INDEX.md                    # 本文件（文档索引）
├── guides/                     # 用户指南
├── design/                     # 架构设计
├── npu/                        # NPU 扩展
├── development/                # 开发记录
├── research/                   # 研究调研
├── reference/                  # 参考资料
└── presentations/              # 演示文稿
```

---

## 1) 先看这些（高频入口）

| 文档 | 说明 |
|------|------|
| [guides/GETTING_STARTED.md](guides/GETTING_STARTED.md) | 快速上手、构建运行与常用命令 |
| [design/ROADMAP.md](design/ROADMAP.md) | 阶段进度、里程碑、验收证据 |
| [design/PROJECT_STRUCTURE.md](design/PROJECT_STRUCTURE.md) | 目录职责与产物落位 |
| [guides/DEMO_GUIDE.md](guides/DEMO_GUIDE.md) | 课程演示与验收脚本串联 |

---

## 2) 架构与实现（核心技术）

| 文档 | 说明 |
|------|------|
| [design/ARCHITECTURE.md](design/ARCHITECTURE.md) | 架构详版（完整设计） |
| [design/PERFORMANCE_MONITORING.md](design/PERFORMANCE_MONITORING.md) | 性能计数器与报告 |
| [design/VISUALIZATION_DESIGN.md](design/VISUALIZATION_DESIGN.md) | 可视化设计与交互 |

---

## 3) 协处理器与加速器

| 文档 | 说明 |
|------|------|
| [npu/NPU_SPEC.md](npu/NPU_SPEC.md) | NPU ABI / 寄存器规范（权威细节） |
| [npu/NPU_OVERVIEW.md](npu/NPU_OVERVIEW.md) | NPU 集成概览与运行说明 |
| [npu/NPU_INTEGRATION_SUMMARY.md](npu/NPU_INTEGRATION_SUMMARY.md) | NPU 集成调试总结 |
| [guides/GPU_TPU_API.md](guides/GPU_TPU_API.md) | GPU/TPU MMIO 软件 API 文档 |

---

## 4) 开发记录与复盘

| 文档 | 说明 |
|------|------|
| [development/MILESTONE_EXECUTION.md](development/MILESTONE_EXECUTION.md) | 里程碑执行台账 |
| [development/CONSTRUCTION_BLUEPRINT.md](development/CONSTRUCTION_BLUEPRINT.md) | 施工蓝图 v3.0（已合并到 ROADMAP） |
| [development/PHASE3_STRICT_ACCEPTANCE_ISSUES.md](development/PHASE3_STRICT_ACCEPTANCE_ISSUES.md) | Phase 3 严格验收问题清单 |
| [development/PHASE6_IMPROVEMENTS.md](development/PHASE6_IMPROVEMENTS.md) | Phase 6 改进方向与后续优化 |

---

## 5) 研究与调研

| 文档 | 说明 |
|------|------|
| [research/CPU_SIMULATOR_RESEARCH_REVIEW_20260331.md](research/CPU_SIMULATOR_RESEARCH_REVIEW_20260331.md) | CPU 模拟器调研报告 |

---

## 6) 参考资料

| 文档 | 说明 |
|------|------|
| [reference/CLAUDE_COMMANDS.md](reference/CLAUDE_COMMANDS.md) | Claude Code 斜杠命令参考 |

---

## 7) 汇报材料

| 文档 | 说明 |
|------|------|
| [presentations/汇报讲稿.md](presentations/汇报讲稿.md) | 汇报讲稿 |
| [presentations/开题报告_PPT大纲与5分钟讲稿.md](presentations/开题报告_PPT大纲与5分钟讲稿.md) | 开题报告材料 |

---

## 维护约定

1. **不做大段删除**；优先重排结构与补充索引。
2. 同主题文档允许"概览 + 详版 + 快照"并存。
3. 新增文档后**同步更新本索引**。
4. 若内容较长，可先加"速读摘要"，原文保留。
5. 文件应放入对应的子目录，保持分类清晰。
