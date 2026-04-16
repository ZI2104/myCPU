import React, { useEffect, useRef, useState } from 'react';
import type { ExStageInfo, ForwardSourceSnapshot, ForwardingInfo, IdStageInfo, IfStageInfo, MemStageInfo, PipelineSnapshot, PreIfStageInfo, WbStageInfo } from '../types/snapshot';
import { formatShortHex } from '../utils/format';

interface PipelineVisualizerProps {
  pipeline: PipelineSnapshot | null;
  resetKey?: number;
  followKey?: number;
  cycle?: number; // optional server-provided global cycle counter
  running?: boolean;
  resetSequence?: number;
}

interface HistoryEntry {
  pipeline: PipelineSnapshot;
  timestamp: number;
  cycle: number;
  resetSequence: number;
}

interface CellContent {
  pc: string;
  detail: string;
  highlight: boolean;
  badges?: string[];
}

// Reduced high-contrast palette for instruction tracing.
// Keep color kinds small and clearly separated.
const INSTRUCTION_COLORS = [
  '#ef4444', // red
  '#3b82f6', // blue
  '#22c55e', // green
  '#a855f7', // purple
  '#f59e0b', // amber
] as const;

// Stride over the palette so consecutive PCs don't look like adjacent hues.
const COLOR_STRIDE = 3;

const getInstructionColorByPc = (pcHex: string): string => {
  const normalized = pcHex.replace(/^0x/i, '');
  const pc = Number.parseInt(normalized, 16);
  if (Number.isNaN(pc)) {
    return INSTRUCTION_COLORS[0];
  }

  // Deterministic color by instruction address (PC).
  // Use PC>>2 to ignore alignment zeros; stride helps separate neighbors.
  const idx = ((pc >>> 2) * COLOR_STRIDE) % INSTRUCTION_COLORS.length;
  return INSTRUCTION_COLORS[idx];
};

// Format forwarding badge text.
const formatForwardBadge = (reg: string, source: ForwardSourceSnapshot): string | null => {
  if (source === 'none') return null;
  // Label reflects the pipeline stage that PRODUCED the data:
  //   ex_mem → EX stage output (1-cycle-old result)
  //   mem_wb → MEM stage output (2-cycle-old result)
  const label = source === 'ex_mem' ? 'EX' : 'MEM';
  return `${reg}←${label}`;
};

// Extract forwarding badges from a ForwardingInfo object.
const getForwardingBadges = (fwd: ForwardingInfo | null | undefined): string[] => {
  if (!fwd) return [];
  const badges: string[] = [];
  const b1 = formatForwardBadge('rs1', fwd.rs1);
  const b2 = formatForwardBadge('rs2', fwd.rs2);
  if (b1) badges.push(b1);
  if (b2) badges.push(b2);
  return badges;
};

// 获取各阶段的详情信息
const getPreIfDetail = (stage: PreIfStageInfo | null): { pc: string; detail: string; highlight: boolean } => {
  if (!stage) return { pc: '', detail: '', highlight: false };
  return {
    pc: formatShortHex(stage.next_pc),
    detail: `→ ${formatShortHex(stage.fetch_addr)}`,
    highlight: false,
  };
};

const getIfDetail = (stage: IfStageInfo | null): { pc: string; detail: string } => {
  if (!stage) return { pc: '', detail: '' };
  // Prefer a pre-decoded instruction string from the backend if available;
  // otherwise fall back to showing the short hex encoding.
  const instrStr = stage.instruction_str && stage.instruction_str.length > 0
    ? stage.instruction_str
    : `inst: ${formatShortHex(stage.instruction)}`;
  return {
    pc: formatShortHex(stage.pc),
    detail: instrStr
  };
};

const getIdDetail = (stage: IdStageInfo | null): { pc: string; detail: string; highlight: boolean; badges: string[] } => {
  if (!stage) return { pc: '', detail: '', highlight: false, badges: [] };

  const badges = getForwardingBadges(stage.forwarding);

  if (stage.is_branch) {
    // Show branch target (or "not taken") and, when forwarding occurred,
    // the actual forwarded operand value so the user can verify it.
    const fwd = stage.forwarding;
    const fwdParts: string[] = [];
    if (fwd && fwd.rs1 !== 'none' && stage.rs1 !== 0) {
      fwdParts.push(`x${stage.rs1}=${formatShortHex(stage.rs1_val)}`);
    }
    if (fwd && fwd.rs2 !== 'none' && stage.rs2 !== 0) {
      fwdParts.push(`x${stage.rs2}=${formatShortHex(stage.rs2_val)}`);
    }
    const base = stage.branch_taken
      ? `-> ${formatShortHex(stage.branch_target)}`
      : 'branch: not taken';
    const detail = fwdParts.length > 0
      ? `${base} (${fwdParts.join(', ')})`
      : base;
    return {
      pc: formatShortHex(stage.pc),
      detail,
      highlight: stage.branch_taken,
      badges,
    };
  }

  const parts: string[] = [];
  if (stage.rs1 !== 0) parts.push(`x${stage.rs1}`);
  if (stage.rs2 !== 0) parts.push(`x${stage.rs2}`);
  return {
    pc: formatShortHex(stage.pc),
    detail: parts.length > 0 ? parts.join(', ') : '-',
    highlight: false,
    badges,
  };
};

const getExDetail = (stage: ExStageInfo | null): { pc: string; detail: string; highlight: boolean; badges: string[] } => {
  if (!stage) return { pc: '', detail: '', highlight: false, badges: [] };
  return {
    pc: formatShortHex(stage.pc),
    detail: stage.branch_taken
      ? `-> ${formatShortHex(stage.branch_target)}`
      : `alu: ${formatShortHex(stage.alu_result)}`,
    // Branch is resolved in ID stage now; EX stays informational.
    highlight: false,
    badges: getForwardingBadges(stage.forwarding),
  };
};

const getMemDetail = (stage: MemStageInfo | null): { pc: string; detail: string } => {
  if (!stage) return { pc: '', detail: '' };
  let detail = '-';
  if (stage.mem_read) detail = `LD ${formatShortHex(stage.alu_result)}`;
  else if (stage.mem_write) detail = `ST ${formatShortHex(stage.alu_result)}`;
  return { pc: formatShortHex(stage.pc), detail };
};

const getWbDetail = (stage: WbStageInfo | null): { pc: string; detail: string } => {
  if (!stage) return { pc: '', detail: '' };
  let detail = '-';
  if (stage.reg_write && stage.rd !== 0) {
    detail = `x${stage.rd} <- ${formatShortHex(stage.write_data)}`;
  }
  return { pc: formatShortHex(stage.pc), detail };
};

// 阶段配置
const STAGES = [
  { key: 'preIF' as const, label: 'pre-IF' },
  { key: 'IF' as const, label: 'IF' },
  { key: 'ID' as const, label: 'ID' },
  { key: 'EX' as const, label: 'EX' },
  { key: 'MEM' as const, label: 'MEM' },
  { key: 'WB' as const, label: 'WB' },
];

export const PipelineVisualizer: React.FC<PipelineVisualizerProps> = ({
  pipeline,
  resetKey,
  followKey,
  cycle,
  running = false,
  resetSequence = 0,
}) => {
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const prevPipelineRef = useRef<PipelineSnapshot | null>(null);
  const prevResetKeyRef = useRef<number>(resetKey ?? 0);
  const cycleCounterRef = useRef<number>(0);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const userScrolledRef = useRef<boolean>(false);
  const suppressNextScrollEventRef = useRef<boolean>(false);
  const prevFollowKeyRef = useRef<number | undefined>(undefined);

  // 当 resetKey 变化时清空历史
  useEffect(() => {
    if (resetKey !== prevResetKeyRef.current) {
      setHistory([]);
      cycleCounterRef.current = 0;
      prevResetKeyRef.current = resetKey ?? 0;
      prevPipelineRef.current = null;
      // After a reset we want the UI to auto-follow the incoming
      // timeline again (clear any manual-scroll state). This ensures
      // that running after reset will keep the timeline scrolled to
      // the latest cycle by default.
      userScrolledRef.current = false;
    }
  }, [resetKey]);

  // when parent signals followKey changed (user requested follow), re-enable auto-follow
  useEffect(() => {
    if (followKey !== undefined && followKey !== prevFollowKeyRef.current) {
      userScrolledRef.current = false;
      prevFollowKeyRef.current = followKey;
    }
  }, [followKey]);

  // While running, keep auto-follow enabled to always show the newest cycle.
  useEffect(() => {
    if (running) {
      userScrolledRef.current = false;
    }
  }, [running]);

  useEffect(() => {
    if (!pipeline) return;

    // Always record a history entry for each incoming pipeline snapshot so
    // that bubbles/empty cycles are visible in the timeline. Prefer the
    // server-provided cycle number when available (prevents ordering issues
    // when multiple snapshots arrive quickly); fall back to a local
    // incrementing counter otherwise.
    setHistory(prev => {
      const cycleNumber = (typeof cycle === 'number') ? cycle : (cycleCounterRef.current + 1);
      // update local counter
      cycleCounterRef.current = cycleNumber;
      const newEntry: HistoryEntry = {
        pipeline,
        timestamp: Date.now(),
        cycle: cycleNumber,
        resetSequence,
      };
      // debug: log cycle numbers for troubleshooting missing-even-cycles (dev only)
      if (import.meta.env.DEV) {
        console.debug('[PipelineVisualizer] append cycle', newEntry.cycle, 'pc', pipeline.if_stage?.pc);
      }
      // detect gaps between last recorded cycle (prev[0]) and this new cycle
      const placeholders: HistoryEntry[] = [];
      const sameResetPrev = prev.filter(e => e.resetSequence === resetSequence);
      const lastRecorded = sameResetPrev.length > 0 ? sameResetPrev[0].cycle : (newEntry.cycle - 1);
      const gap = newEntry.cycle - lastRecorded;
      if (gap > 1) {
        for (let c = lastRecorded + 1; c < newEntry.cycle; c++) {
          placeholders.push({
            pipeline: {
              if_stage: null,
              pre_if_stage: null,
              id_stage: null,
              ex_stage: null,
              mem_stage: null,
              wb_stage: null,
              stall: false,
              flush: false,
              stall_type: null,
              control_hazard: false,
            },
            timestamp: Date.now() + c,
            cycle: c,
            resetSequence,
          });
        }
      }
      // Merge with existing history and deduplicate by cycle.
      // Keep the newest entry for each cycle (prefer newEntry), and include
      // placeholders only when a cycle is missing.
      const map = new Map<number, HistoryEntry>();

      // Start by inserting previous entries (they will be overwritten by newEntry)
      for (const e of sameResetPrev) {
        map.set(e.cycle, e);
      }

      // Insert placeholders if that cycle isn't already present
      for (const p of placeholders) {
        if (!map.has(p.cycle)) {
          map.set(p.cycle, p);
        }
      }

      // Finally, insert/overwrite with the newly received entry
      map.set(newEntry.cycle, newEntry);

      // Convert map to array and sort by descending cycle (newest first)
      const merged = Array.from(map.values()).sort((a, b) => b.cycle - a.cycle);
      const newHistory = merged;
      return newHistory.slice(0, 200);
    });

    // auto-scroll to the latest cycles if user hasn't manually scrolled
    requestAnimationFrame(() => {
      if (containerRef.current && (running || !userScrolledRef.current)) {
        // scroll to the rightmost position (latest cycles)
        suppressNextScrollEventRef.current = true;
        containerRef.current.scrollLeft = Math.max(0, containerRef.current.scrollWidth - containerRef.current.clientWidth);
      }
    });

    prevPipelineRef.current = pipeline;
  }, [pipeline, running, cycle, resetSequence]);

  if (!pipeline) {
    return <div className="pipeline-visualizer">No pipeline data</div>;
  }

  // 获取单元格内容
  const getCellContent = (stageKey: string, entry: HistoryEntry): CellContent => {
    const p = entry.pipeline;
    switch (stageKey) {
      case 'preIF': return getPreIfDetail(p.pre_if_stage);
      case 'IF': return { ...getIfDetail(p.if_stage), highlight: false };
      case 'ID': return getIdDetail(p.id_stage);
      case 'EX': return getExDetail(p.ex_stage);
      case 'MEM': return { ...getMemDetail(p.mem_stage), highlight: false };
      case 'WB': return { ...getWbDetail(p.wb_stage), highlight: false };
      default: return { pc: '', detail: '', highlight: false };
    }
  };

  // 反转历史顺序，让最新在右边（时间从右到左），显示完整历史（已在写入时截断到最大长度）
  const reversedHistory = [...history].reverse();
  const visibleHistory = reversedHistory;

  return (
    <div className="pipeline-visualizer">
      <div className="pipeline-timeline-container">
        {/* 左侧阶段标签列 */}
        <div className="timeline-stage-labels">
          <div className="timeline-header-cell" />
          {STAGES.map(stage => (
            <div key={stage.key} className="timeline-stage-label">
              {stage.label}
            </div>
          ))}
          <div className="timeline-stage-label stall-label-col">Stall</div>
        </div>

        {/* 时间轴（从左到右，最新在右边） - 横向可滚动，用户可拖动来改变时间窗口 */}
        <div
          className="timeline-columns"
          ref={el => { containerRef.current = el; }}
          onScroll={() => {
            if (suppressNextScrollEventRef.current) {
              suppressNextScrollEventRef.current = false;
              return;
            }
            if (!running) {
              userScrolledRef.current = true;
            }
          }}
        >
          {visibleHistory.map((entry) => (
            <div key={entry.cycle} className="timeline-column">
              <div className="timeline-header-cell">
                C{entry.cycle}
              </div>
              {STAGES.map(stage => {
                const content = getCellContent(stage.key, entry);
                const hasData = content.pc !== '';
                const instructionColor = hasData ? getInstructionColorByPc(content.pc) : undefined;

                // Stall cell styling: IF and ID are frozen during stalls
                const isFrozenStage = entry.pipeline.stall && (stage.key === 'IF' || stage.key === 'ID');
                const stallClass = isFrozenStage
                  ? entry.pipeline.stall_type === 'load_use'
                    ? 'stall-load-use'
                    : 'stall-branch-data'
                  : '';

                // Control hazard styling: IF cell flushed by misprediction
                const isControlHazard = entry.pipeline.control_hazard && stage.key === 'IF';
                const hazardClass = isControlHazard ? 'stall-control-hazard' : '';

                return (
                  <div
                    key={stage.key}
                    className={`timeline-cell ${hasData ? 'has-data' : 'empty'} ${stallClass} ${hazardClass}`}
                    style={
                      hasData
                        ? ({ border: `2px solid ${instructionColor}` } as React.CSSProperties)
                        : undefined
                    }
                  >
                    {hasData ? (
                      <>
                        <div className="cell-pc">{content.pc}</div>
                        <div className="cell-detail">{content.detail}</div>
                        {content.badges && content.badges.length > 0 && (
                          <div className="cell-badges">
                            {content.badges.map((b, i) => (
                              <span key={i} className="forward-badge">{b}</span>
                            ))}
                          </div>
                        )}
                      </>
                    ) : (
                      // render a small bubble for empty cycles so "空拍" 可见
                      <div className="empty-bubble" aria-hidden="true" />
                    )}
                  </div>
                );
              })}
              {/* Stall legend row */}
              <div className="timeline-stall-legend">
                {entry.pipeline.stall_type ? (
                  <span className={`stall-label ${entry.pipeline.stall_type}`}>
                    {entry.pipeline.stall_type === 'load_use' ? 'Load-Use' : 'Branch Data'}
                  </span>
                ) : entry.pipeline.control_hazard ? (
                  <span className="stall-label control-hazard">
                    Control Hazard
                  </span>
                ) : null}
              </div>
            </div>
          ))}
        </div>

        {/* legend removed - timeline is horizontally scrollable */}
      </div>

      {/* 状态标签 */}
      <div className="pipeline-status">
        {pipeline.stall && (
          <span className={`status stall ${pipeline.stall_type === 'branch_data' ? 'branch-data' : ''}`}>
            STALL ({pipeline.stall_type === 'load_use' ? 'Load-Use' : 'Branch Data'})
          </span>
        )}
        {pipeline.flush && <span className="status flush">FLUSH</span>}
        {pipeline.control_hazard && !pipeline.stall && <span className="status control-hazard">CONTROL HAZARD</span>}
      </div>
    </div>
  );
};
