# myCPU 项目目录结构说明

> 更新日期：2026-04-02
>
> 目标：保持“根目录简洁、职责清晰、产物可清理”。

## 一级目录职责

| 目录           | 职责                                           | 是否应提交       |
| -------------- | ---------------------------------------------- | ---------------- |
| `src/`         | Rust 主体代码（CPU/指令/内存/外设/可视化后端） | 是               |
| `tests/`       | 集成测试、测试程序                             | 是               |
| `frontend/`    | 前端可视化项目（Vite + React + TS）            | 是               |
| `scripts/`     | 自动化脚本（构建、验收、清理）                 | 是               |
| `docs/`        | 架构/路线图/执行记录/演示文档                  | 是               |
| `plans/`       | 蓝图与规划草案                                 | 是               |
| `artifacts/`   | 外部工件输入（如 Phase3 Linux 工件）           | 是（按需）       |
| `third_party/` | 三方源码（xv6、buildroot 等）                  | 是（按仓库策略） |
| `target/`      | Rust 构建输出                                  | 否（可清理）     |
| `tmp/`         | 临时文件                                       | 否（可清理）     |

## 文档与演示资料归档约定

- 设计与执行文档：放在 `docs/`。
- 汇报材料（PPT/开题报告）：统一放在 `docs/presentations/`。
- 规划草案与阶段蓝图：放在 `plans/`。

## `src/` 代码目录分层约定

- `src/cpu/`：CPU 核心、执行模型、流水线、CSR。
- `src/instruction/`：指令解码与执行分发。
- `src/memory/`：RAM/ROM/Bus 等内存系统。
- `src/peripheral/` + `src/interrupt/`：设备与中断控制器。
- `src/visualize/`：可视化后端与快照协议。
- `src/perf/`：性能统计与报告（已完成从根级 `perf_report.rs` 的迁移）。

## 产物落位约定

### 1) Linux/OS 工件

- Phase3 相关工件建议统一放在：`artifacts/phase3/`
  - 示例：`fw_jump.elf`、`Image`、`rootfs.ext2`、`virt-qemu.dtb`

### 2) 日志与临时输出

- 优先写入 `target/` 下子目录（例如 `target/phase6-demo-logs/`）。
- 避免在仓库根目录散落 `*-logs/` 目录。

## 根目录整洁规则

根目录仅保留“工程入口与关键配置”：

- `Cargo.toml` / `Cargo.lock`
- `README.md`
- 核心目录：`src/`, `tests/`, `frontend/`, `scripts/`, `docs/`, `plans/`, `artifacts/`, `third_party/`

其余资料或临时文件应归档到对应子目录，不应长期留在根目录。

## 推荐清理动作

- 定期执行：`scripts/cleanup_workspace.ps1`
- 在大规模回归后执行深度瘦身：`scripts/cleanup_workspace.ps1 -PruneBuildCaches`

### 清理档位（建议）

1. **常规清理（安全默认）**
  - 命令：`scripts/cleanup_workspace.ps1`
  - 清理内容：测试临时文件、验收日志目录、`target/tmp`。

2. **深度瘦身（可重建缓存）**
  - 命令：`scripts/cleanup_workspace.ps1 -PruneBuildCaches`
  - 清理内容：`target/`、`third_party/buildroot-riscv32-virt/output/`、`frontend/node_modules`、`frontend/dist`、`tmp/`。

3. **可选扩展清理**
  - `-PurgeXv6`：清理本地 xv6 third_party 源码缓存。
  - `-PurgePhase3Artifacts`：清理 `artifacts/phase3` 本地工件缓存。

## 当前体积热点与精简优先级（2026-04-02）

- `third_party/`（主要是 buildroot output）
- `target/`（Rust 构建缓存与日志）

优先建议：先执行 `-PruneBuildCaches`，通常可释放绝大多数占用空间，且不会破坏仓库源码结构。

---

如需进一步收敛，可在后续迭代中增加：

1. CI 自动检查（阻止新的根目录散落文件）
2. 脚本统一日志输出目录规范（默认写 `target/logs/*`）
3. 文档结构 lint（检测失效路径引用）
