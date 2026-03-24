# Claude Code 斜杠命令完整参考

> 本文档介绍 Claude Code 中所有可用的斜杠命令、技能和代理

## 目录

- [内置命令](#内置命令)
- [开发工作流技能](#开发工作流技能)
- [语言特定技能](#语言特定技能)
- [架构与设计](#架构与设计)
- [测试与验证](#测试与验证)
- [部署与运维](#部署与运维)
- [AI 与 LLM](#ai-与-llm)
- [内容与媒体](#内容与媒体)
- [行业专业知识](#行业专业知识)
- [工具与集成](#工具与集成)
- [系统管理](#系统管理)

---

## 内置命令

### `/help` - 显示帮助信息

**说明**: 显示所有可用命令或特定命令的详细说明

**用法**:
```
/help                    # 显示所有命令
/help <command>          # 显示特定命令的帮助
```

### `/compact` - 压缩上下文

**说明**: 压缩对话历史以释放上下文空间，保留重要信息

**用法**:
```
/compact
```

### `/clear` - 清除对话

**说明**: 清除当前对话历史

**用法**:
```
/clear
```

### `/fast` - 切换快速模式

**说明**: 开启/关闭快速模式（更快输出，相同模型）

**用法**:
```
/fast
```

---

## 开发工作流技能

### `/plan` - 实施规划

**说明**: 为复杂功能创建分步实施计划

**使用场景**:
- 实现新功能前进行架构规划
- 重构大型模块前制定方案
- 多步骤任务的分解

**示例**:
```
/plan 实现一个支持 RV32I 指令集的译码器
```

### `/tdd` - 测试驱动开发

**说明**: 强制执行测试优先开发流程，确保 80%+ 覆盖率

**使用场景**:
- 新功能开发
- Bug 修复
- 代码重构

**示例**:
```
/tdd 实现 RAM 模块
```

### `/commit` - Git 提交

**说明**: 分析更改并创建符合规范的 Git commit

**示例**:
```
/commit
```

### `/review` - 代码审查

**说明**: 审查代码质量、安全性和最佳实践

**示例**:
```
/review
```

### `/simplify` - 代码简化

**说明**: 审查并改进代码的可读性和效率

**示例**:
```
/simplify
```

### `/doc-updater` - 文档更新

**说明**: 分析 git diff 识别变更并批量更新项目文档

**示例**:
```
/doc-updater              # 分析所有未提交的变更
/doc-updater HEAD~3..HEAD # 分析指定范围
/doc-updater --staged     # 分析已暂存的变更
```

**变更类型**:
- 新功能实现 → 更新 API 规范、开发指南
- 问题修复 → 更新故障排查文档
- 前端修改 → 更新前端开发指南
- 后端修改 → 更新后端开发指南、API 规范
- 架构修改 → 更新系统架构文档

---

## 语言特定技能

### Rust

#### `/rust-test` - Rust TDD 工作流

**说明**: 使用 TDD 方法编写 Rust 代码，确保 80%+ 测试覆盖率

**工作流程**:
1. 先写测试（RED）
2. 实现最小代码（GREEN）
3. 重构改进（IMPROVE）

**示例**:
```
/rust-test 实现 Memory trait
```

#### `/rust-review` - Rust 代码审查

**说明**: 全面审查 Rust 代码，检查：
- 所有权和生命周期
- 错误处理
- unsafe 代码使用
- Rust 惯用法

**示例**:
```
/rust-review
```

#### `/rust-build` - Rust 构建修复

**说明**: 修复 Rust 编译错误、借用检查器问题

**示例**:
```
/rust-build
```

### Python

#### `/python-review` - Python 代码审查

**说明**: 审查 Python 代码的 PEP 8 合规性、类型提示、安全性

**示例**:
```
/python-review
```

#### `/python-testing` - Python 测试策略

**说明**: 使用 pytest、fixtures、mocking 和覆盖率

**示例**:
```
/python-testing
```

### Go

#### `/go-test` - Go TDD 工作流

**说明**: 表驱动测试、子测试、基准测试

**示例**:
```
/go-test
```

#### `/go-review` - Go 代码审查

**说明**: 审查 Go 惯用法、并发安全、错误处理

**示例**:
```
/go-review
```

#### `/go-build` - Go 构建修复

**说明**: 修复 go build、go vet 问题

**示例**:
```
/go-build
```

### Java/Kotlin

#### `/java-review` - Java 代码审查

**说明**: 审查 Java Spring Boot 应用

**示例**:
```
/java-review
```

#### `/kotlin-review` - Kotlin 代码审查

**说明**: 审查 Kotlin/Android/KMP 代码

**示例**:
```
/kotlin-review
```

#### `/kotlin-test` - Kotlin 测试

**说明**: 使用 Kotest、MockK 进行 TDD

**示例**:
```
/kotlin-test
```

### C++

#### `/cpp-review` - C++ 代码审查

**说明**: 审查内存安全、现代 C++ 惯用法

**示例**:
```
/cpp-review
```

#### `/cpp-testing` - C++ 测试

**说明**: 使用 GoogleTest 进行 TDD

**示例**:
```
/cpp-testing
```

### JavaScript/TypeScript

#### `/typescript-review` - TypeScript 代码审查

**说明**: 审查 TS/JS 代码的类型安全、异步正确性

**示例**:
```
/typescript-review
```

---

## 架构与设计

### `/blueprint` - 蓝图规划

**说明**: 将目标转化为多会话、多代理工程项目的分步施工计划

**使用场景**:
- 复杂的多 PR 任务
- 需要 3 次以上工具调用的任务
- 架构级别的设计决策

**示例**:
```
/blueprint 实现一个完整的 RISC-V 模拟器
```

**输出**:
- 自包含的上下文简报
- 依赖关系图
- 并行步骤检测
- 对抗性审查关卡

### `/architecture` - 架构设计

**说明**: 系统设计、可扩展性、技术决策

**示例**:
```
/architecture 设计一个可扩展的内存系统
```

---

## 测试与验证

### `/e2e` - 端到端测试

**说明**: 使用 Playwright 生成和运行 E2E 测试

**功能**:
- 测试旅程管理
- 不稳定测试隔离
- 截图/视频/追踪上传
- 关键用户流程验证

**示例**:
```
/e2e 测试用户登录流程
```

### `/verification-loop` - 综合验证系统

**说明**: 完整的验证系统，适用于 Claude Code 会话

**示例**:
```
/verification-loop
```

---

## 部署与运维

### `/deployment` - 部署模式

**说明**: CI/CD 流水线、Docker 容器化、部署策略

**示例**:
```
/deployment
```

---

## AI 与 LLM

### `/claude-api` - Claude API 开发

**说明**: 使用 Python 和 TypeScript 的 Anthropic Claude API 模式

**覆盖**:
- Messages API
- Streaming
- Tool use
- Vision
- Extended thinking
- Batches
- Prompt caching
- Claude Agent SDK

**示例**:
```
/claude-api 如何实现流式响应
```

---

## 内容与媒体

### `/content-engine` - 内容引擎

**说明**: 为 X、LinkedIn、TikTok、YouTube、Newsletter 创建原生内容系统

**功能**:
- 跨平台内容分发
- 原生内容适配
- 内容日历
- 单一资产多平台改编

**示例**:
```
/content-engine 创建产品发布推文系列
```

### `/article-writing` - 文章写作

**说明**: 撰写文章、指南、博客、教程、Newsletter

**示例**:
```
/article-writing 写一篇关于 RISC-V 的技术文章
```

### `/video-editing` - 视频编辑

**说明**: AI 辅助视频编辑工作流

**工具链**:
- FFmpeg
- Remotion
- ElevenLabs
- fal.ai
- Descript / CapCut

**示例**:
```
/video-editing 剪辑一个教程视频
```

---

## 行业专业知识

### `/carrier-management` - 承运商关系管理

**说明**: 管理承运商组合、费率谈判、绩效评估、货物分配

**专业知识**:
- 15+ 年运输管理经验
- 记分卡框架
- RFP 流程
- 市场情报
- 合规审查

### `/energy-procurement` - 能源采购

**说明**: 电力和天然气采购、关税优化、需求管理

**专业知识**:
- 市场结构分析
- 对冲策略
- 负载曲线
- PPA 评估
- 可持续发展报告

### `/inventory-planning` - 库存需求规划

**说明**: 需求预测、安全库存优化、补货规划

**专业知识**:
- ABC/XYZ 分析
- 季节性转换管理
- 供应商谈判框架

### `/quality-ncon` - 质量不合格品管理

**说明**: 质量控制、不合格品调查、根本原因分析、CAPA

**专业知识**:
- FDA、IATF 16949、AS9100 环境
- NCR 生命周期管理
- SPA 解释
- 审计方法

### `/logistics-exception` - 物流异常管理

**说明**: 货运异常、运输延迟、货损、承运商争议

**专业知识**:
- 15+ 年运营经验
- 升级协议
- 承运商特定行为
- 索赔流程

### `/production-scheduling` - 生产调度

**说明**: 生产调度、作业排序、生产线平衡

**专业知识**:
- TOC/鼓-缓冲-绳
- SMED
- OEE 分析
- 扰动响应框架

### `/returns-logistics` - 退货和逆向物流

**说明**: 退货授权、收货检验、处置决策、退款处理

**专业知识**:
- 分级框架
- 处置经济学
- 欺诈模式识别
- 供应商恢复流程

### `/customs-compliance` - 海关贸易合规

**说明**: 海关文件、关税分类、关税优化、受限方筛查

**专业知识**:
- HS 分类逻辑
- Incoterms 应用
- FTA 利用
- 惩罚减免

---

## 工具与集成

### `/docs` - 文档查找

**说明**: 通过 Context7 MCP 获取库和框架的最新文档

**示例**:
```
/docs 如何使用 Tokio 进行异步编程
```

### `/exa-search` - Exa 神经搜索

**说明**: 通过 Exa MCP 进行网页、代码和公司研究

**示例**:
```
/exa-search 搜索 RISC-V 模拟器实现
```

### `/smithery` - Smithery CLI

**说明**: 发现、连接和使用 MCP 工具和技能

**功能**:
- 发现新工具和技能
- 连接 MCP
- 安装技能
- 与外部服务交互

**示例**:
```
/smithery 搜索可用的 MCP 服务器
```

### `/mcp-server` - MCP 服务器

**说明**: 使用 Node/TypeScript SDK 构建 MCP 服务器

**覆盖**:
- 工具、资源、提示
- Zod 验证
- stdio vs Streamable HTTP

**示例**:
```
/mcp-server 创建一个新的 MCP 工具
```

---

## 系统管理

### `/save-session` - 保存会话

**说明**: 将当前会话状态保存到文件

**示例**:
```
/save-session
```

### `/resume-session` - 恢复会话

**说明**: 从最近的会话文件恢复工作

**示例**:
```
/resume-session
```

### `/loop` - 循环任务

**说明**: 设置定期执行的任务

**语法**:
```
/loop <间隔> <命令>
```

**示例**:
```
/loop 5m /test          # 每 5 分钟运行测试
/loop 1h /save-session  # 每小时保存会话
```

### `/sessions` - 会话管理

**说明**: 管理 Claude Code 会话历史、别名和元数据

**示例**:
```
/sessions list          # 列出所有会话
/sessions resume <id>   # 恢复特定会话
```

### `/instinct` - 本能管理

**说明**: 管理从会话中提取的可重用模式

**子命令**:
```
/instinct-status        # 显示已学习的本能
/instinct-export        # 导出本能到文件
/instinct-import        # 从文件导入本能
/instinct-promote       # 将项目本能提升到全局
/learn-eval             # 提取模式并保存
```

### `/projects` - 项目列表

**说明**: 列出已知项目及其本能统计

**示例**:
```
/projects
```

### `/configure-ecc` - ECC 配置安装

**说明**: 交互式安装 Everything Claude Code

**功能**:
- 选择和安装技能和规则
- 验证路径
- 优化安装文件

**示例**:
```
/configure-ecc
```

---

## 安全相关

### `/security-review` - 安全审查

**说明**: 当添加认证、处理用户输入、处理敏感数据时使用

**检查项**:
- 认证/授权验证
- 输入验证
- SQL 注入预防
- XSS 预防
- CSRF 保护
- 密钥管理
- 速率限制

**示例**:
```
/security-review
```

### `/security-scan` - 安全扫描

**说明**: 扫描 Claude Code 配置中的安全漏洞

**扫描内容**:
- CLAUDE.md
- settings.json
- MCP 服务器
- hooks
- agent 定义

**示例**:
```
/security-scan
```

---

## 技能健康检查

### `/skill-health` - 技能组合健康仪表板

**说明**: 显示技能组合健康状况、图表和分析

**示例**:
```
/skill-health
```

### `/skill-stocktake` - 技能盘点

**说明**: 审计 Claude 技能和命令的质量

**模式**:
- Quick Scan: 仅变更的技能
- Full Stocktake: 完整盘点

**示例**:
```
/skill-stocktake        # 快速扫描
/skill-stocktake full   # 完整盘点
```

---

## 其他工具

### `/context-budget` - 上下文预算分析

**说明**: 分析代理、技能、MCP 服务器和规则的上下文窗口消耗

**示例**:
```
/context-budget
```

### `/regex-vs-llm` - 正则 vs LLM 决策

**说明**: 选择正则表达式还是 LLM 解析结构化文本的决策框架

**原则**:
- 从正则开始
- 低置信度边缘情况添加 LLM

**示例**:
```
/regex-vs-llm 如何解析日志文件
```

### `/aside` - 旁白模式

**说明**: 回答快速问题而不中断或丢失当前任务的上下文

**示例**:
```
/aside Rust 的 Option 和 Result 有什么区别
```

---

## 快速参考表

| 类别 | 命令 | 用途 |
|------|------|------|
| 规划 | `/plan` | 创建实施计划 |
| | `/blueprint` | 多项目规划 |
| 测试 | `/tdd` | 测试驱动开发 |
| | `/rust-test` | Rust TDD |
| | `/python-testing` | Python 测试 |
| | `/e2e` | 端到端测试 |
| 审查 | `/review` | 通用代码审查 |
| | `/rust-review` | Rust 审查 |
| | `/python-review` | Python 审查 |
| | `/security-review` | 安全审查 |
| 构建 | `/rust-build` | 修复构建错误 |
| | `/go-build` | Go 构建修复 |
| 文档 | `/doc-updater` | 更新项目文档 |
| | `/docs` | 查找库文档 |
| 会话 | `/save-session` | 保存会话 |
| | `/resume-session` | 恢复会话 |
| | `/compact` | 压缩上下文 |
| AI | `/claude-api` | Claude API 开发 |
| 内容 | `/content-engine` | 跨平台内容 |
| | `/article-writing` | 文章写作 |
| | `/video-editing` | 视频编辑 |

---

## 使用技巧

1. **Tab 补全**: 输入命令前几个字符后按 Tab 键补全
2. **命令链**: 可以组合使用多个命令完成复杂任务
3. **上下文感知**: 命令会根据当前项目类型和文件自动调整
4. **错误恢复**: 大多数命令会提供修复建议

---

## 获取帮助

- 使用 `/help` 查看所有可用命令
- 使用 `/help <command>` 查看特定命令的详细帮助
- 查看 [CLAUDE.md](../CLAUDE.md) 了解项目特定指令

---

*最后更新: 2026-03-24*
