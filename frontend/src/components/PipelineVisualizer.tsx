import React, { useEffect, useRef, useState } from 'react';
import type { ExStageInfo, IdStageInfo, IfStageInfo, MemStageInfo, PipelineSnapshot, PreIfStageInfo, WbStageInfo } from '../types/snapshot';
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

const getIdDetail = (stage: IdStageInfo | null): { pc: string; detail: string } => {
  if (!stage) return { pc: '', detail: '' };
  const parts: string[] = [];
  if (stage.rs1 !== 0) parts.push(`x${stage.rs1}`);
  if (stage.rs2 !== 0) parts.push(`x${stage.rs2}`);
  return {
    pc: formatShortHex(stage.pc),
    detail: parts.length > 0 ? parts.join(', ') : '-'
  };
};

const getExDetail = (stage: ExStageInfo | null): { pc: string; detail: string; highlight: boolean } => {
  if (!stage) return { pc: '', detail: '', highlight: false };
  return {
    pc: formatShortHex(stage.pc),
    detail: stage.branch_taken
      ? `-> ${formatShortHex(stage.branch_target)}`
      : `alu: ${formatShortHex(stage.alu_result)}`,
    highlight: stage.branch_taken
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
  const getCellContent = (stageKey: string, entry: HistoryEntry): { pc: string; detail: string; highlight: boolean } => {
    const p = entry.pipeline;
    switch (stageKey) {
      case 'preIF': return getPreIfDetail(p.pre_if_stage);
      case 'IF': return { ...getIfDetail(p.if_stage), highlight: false };
      case 'ID': return { ...getIdDetail(p.id_stage), highlight: false };
      case 'EX': return { ...getExDetail(p.ex_stage), highlight: p.ex_stage?.branch_taken ?? false };
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
                return (
                  <div
                    key={stage.key}
                    className={`timeline-cell ${hasData ? 'has-data' : 'empty'} ${content.highlight ? 'highlight' : ''}`}
                  >
                    {hasData ? (
                      <>
                        <div className="cell-pc">{content.pc}</div>
                        <div className="cell-detail">{content.detail}</div>
                      </>
                    ) : (
                      // render a small bubble for empty cycles so "空拍" 可见
                      <div className="empty-bubble" aria-hidden="true" />
                    )}
                  </div>
                );
              })}
            </div>
          ))}
        </div>

        {/* legend removed - timeline is horizontally scrollable */}
      </div>

      {/* 状态标签 */}
      <div className="pipeline-status">
        {pipeline.stall && <span className="status stall">STALL (Load-Use)</span>}
        {pipeline.flush && <span className="status flush">FLUSH</span>}
      </div>
    </div>
  );
};
