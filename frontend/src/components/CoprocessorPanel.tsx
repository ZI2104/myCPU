import { useCallback, useEffect, useState, type ReactNode } from 'react';
import type { LpuStateResponse, NpuStateResponse } from '../types/snapshot';
import { formatHex, formatHex64 } from '../utils/format';

interface CoprocessorPanelProps {
  sendCommand: (command: string) => void;
  npuState: NpuStateResponse | null;
  lpuState: LpuStateResponse | null;
}

interface TimelinePoint {
  ts: number;
  notify: number;
  done: number;
  error: number;
  cycles: number;
  pending: boolean;
}

// 限制时间线最大点数，避免内存无限增长
const MAX_TIMELINE_POINTS = 24;

// 独立的 CoprocessorCard 组件，避免在渲染时创建组件
interface CoprocessorCardProps {
  title: string;
  state: NpuStateResponse | LpuStateResponse | null;
  refreshCommand: string;
  timeline: TimelinePoint[];
  onRefresh: (command: string) => void;
  renderTimeline: (timeline: TimelinePoint[]) => React.ReactNode;
}

function CoprocessorCard({
  title,
  state,
  refreshCommand,
  timeline,
  onRefresh,
  renderTimeline,
}: CoprocessorCardProps) {
  return (
    <div className="coproc-card">
      <div className="coproc-card-header">
        <h3>{title}</h3>
        <button onClick={() => onRefresh(refreshCommand)}>刷新</button>
      </div>

      {!state && <div className="coproc-empty">暂无状态，点击"刷新"获取</div>}

      {state && state.success === false && (
        <div className="coproc-error">{state.error ?? '读取失败'}</div>
      )}

      {state?.success && (
        <>
          <div className="coproc-grid">
            <div><span>control</span><strong>{formatHex(state.control)}</strong></div>
            <div><span>status</span><strong>{formatHex(state.status)}</strong></div>
            <div><span>opcode</span><strong>{state.opcode ?? '--'}</strong></div>
            <div><span>cycles</span><strong>{state.cycles ?? '--'}</strong></div>
            <div><span>desc_addr</span><strong>{formatHex64(state.desc_addr)}</strong></div>
            <div><span>desc_len</span><strong>{state.desc_len ?? '--'}</strong></div>
            <div><span>tasks_done</span><strong>{state.tasks_done ?? '--'}</strong></div>
            <div><span>tasks_error</span><strong>{state.tasks_error ?? '--'}</strong></div>
            <div><span>notify_count</span><strong>{state.desc_notify_count ?? '--'}</strong></div>
            <div><span>pending</span><strong>{state.pending_desc_notify ? 'YES' : 'NO'}</strong></div>
          </div>

          <div className="coproc-timeline-block">
            <h4>任务时间线（最近 {MAX_TIMELINE_POINTS} 条）</h4>
            {renderTimeline(timeline)}
          </div>
        </>
      )}
    </div>
  );
}

// 提取通用的时间线更新逻辑，避免代码重复
function useTimeline(
  state: NpuStateResponse | LpuStateResponse | null,
  maxPoints: number,
): TimelinePoint[] {
  const [timeline, setTimeline] = useState<TimelinePoint[]>([]);

  useEffect(() => {
    if (!state?.success) {
      return;
    }

    const point: TimelinePoint = {
      ts: Date.now(),
      notify: state.desc_notify_count ?? 0,
      done: state.tasks_done ?? 0,
      error: state.tasks_error ?? 0,
      cycles: state.cycles ?? 0,
      pending: Boolean(state.pending_desc_notify),
    };

    // eslint-disable-next-line react-hooks/set-state-in-effect -- 从外部系统（WebSocket）同步状态是正确的用例
    setTimeline((prev) => {
      const last = prev[prev.length - 1];
      const unchanged =
        last &&
        last.notify === point.notify &&
        last.done === point.done &&
        last.error === point.error &&
        last.cycles === point.cycles &&
        last.pending === point.pending;

      if (unchanged) {
        return prev;
      }

      const next = [...prev, point];
      return next.slice(-maxPoints);
    });
  }, [state, maxPoints]);

  return timeline;
}

export function CoprocessorPanel({ sendCommand, npuState, lpuState }: CoprocessorPanelProps) {
  const npuTimeline = useTimeline(npuState, MAX_TIMELINE_POINTS);
  const lpuTimeline = useTimeline(lpuState, MAX_TIMELINE_POINTS);

  // 使用 useCallback 缓存函数，避免不必要的子组件重新渲染
  const refreshAll = useCallback(() => {
    sendCommand('npu state');
    sendCommand('lpu state');
  }, [sendCommand]);

  const handleRefresh = useCallback((command: string) => {
    sendCommand(command);
  }, [sendCommand]);

  // 使用 useMemo 缓存时间线渲染结果，避免不必要的重新计算
  const renderTimeline = useCallback((timeline: TimelinePoint[]): ReactNode => {
    if (timeline.length === 0) {
      return <div className="coproc-timeline-empty">暂无时间线数据</div>;
    }

    return (
      <div className="coproc-timeline">
        {timeline
          .slice()
          .reverse()
          .map((point, index, reversed) => {
            const older = reversed[index + 1];
            const doneDelta = older ? point.done - older.done : 0;
            const errorDelta = older ? point.error - older.error : 0;
            const notifyDelta = older ? point.notify - older.notify : 0;
            return (
              <div className="coproc-timeline-row" key={index}>
                <span>{new Date(point.ts).toLocaleTimeString()}</span>
                <span>notify {point.notify} ({notifyDelta >= 0 ? '+' : ''}{notifyDelta})</span>
                <span>done {point.done} ({doneDelta >= 0 ? '+' : ''}{doneDelta})</span>
                <span>err {point.error} ({errorDelta >= 0 ? '+' : ''}{errorDelta})</span>
                <span>pending {point.pending ? 'Y' : 'N'}</span>
              </div>
            );
          })}
      </div>
    );
  }, []);

  return (
    <div className="coproc-panel">
      <div className="coproc-toolbar">
        <h2>NPU/LPU 协处理器状态</h2>
        <button onClick={refreshAll}>刷新全部</button>
      </div>
      <div className="coproc-cards">
        <CoprocessorCard title="NPU" state={npuState} refreshCommand="npu state" timeline={npuTimeline} onRefresh={handleRefresh} renderTimeline={renderTimeline} />
        <CoprocessorCard title="LPU" state={lpuState} refreshCommand="lpu state" timeline={lpuTimeline} onRefresh={handleRefresh} renderTimeline={renderTimeline} />
      </div>
    </div>
  );
}
