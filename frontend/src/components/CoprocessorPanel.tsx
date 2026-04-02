import { useEffect, useState } from 'react';
import type { LpuStateResponse, NpuStateResponse } from '../types/snapshot';

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

const MAX_TIMELINE_POINTS = 24;

function formatHex(value?: number) {
  if (value === undefined) {
    return '--';
  }
  return `0x${value.toString(16).toUpperCase().padStart(8, '0')}`;
}

function formatHex64(value?: number) {
  if (value === undefined) {
    return '--';
  }
  return `0x${Math.trunc(value).toString(16).toUpperCase()}`;
}

export function CoprocessorPanel({ sendCommand, npuState, lpuState }: CoprocessorPanelProps) {
  const [npuTimeline, setNpuTimeline] = useState<TimelinePoint[]>([]);
  const [lpuTimeline, setLpuTimeline] = useState<TimelinePoint[]>([]);

  useEffect(() => {
    if (!npuState?.success) {
      return;
    }

    const point: TimelinePoint = {
      ts: Date.now(),
      notify: npuState.desc_notify_count ?? 0,
      done: npuState.tasks_done ?? 0,
      error: npuState.tasks_error ?? 0,
      cycles: npuState.cycles ?? 0,
      pending: Boolean(npuState.pending_desc_notify),
    };

    setNpuTimeline((prev) => {
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
      return next.slice(-MAX_TIMELINE_POINTS);
    });
  }, [npuState]);

  useEffect(() => {
    if (!lpuState?.success) {
      return;
    }

    const point: TimelinePoint = {
      ts: Date.now(),
      notify: lpuState.desc_notify_count ?? 0,
      done: lpuState.tasks_done ?? 0,
      error: lpuState.tasks_error ?? 0,
      cycles: lpuState.cycles ?? 0,
      pending: Boolean(lpuState.pending_desc_notify),
    };

    setLpuTimeline((prev) => {
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
      return next.slice(-MAX_TIMELINE_POINTS);
    });
  }, [lpuState]);

  const refreshAll = () => {
    sendCommand('npu state');
    sendCommand('lpu state');
  };

  const renderTimeline = (timeline: TimelinePoint[]) => {
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
              <div className="coproc-timeline-row" key={`${point.ts}-${point.done}-${point.notify}-${index}`}>
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
  };

  const renderCard = (
    title: string,
    state: NpuStateResponse | LpuStateResponse | null,
    refreshCommand: string,
    timeline: TimelinePoint[],
  ) => (
    <div className="coproc-card">
      <div className="coproc-card-header">
        <h3>{title}</h3>
        <button onClick={() => sendCommand(refreshCommand)}>刷新</button>
      </div>

      {!state && <div className="coproc-empty">暂无状态，点击“刷新”获取</div>}

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

  return (
    <div className="coproc-panel">
      <div className="coproc-toolbar">
        <h2>NPU/LPU 协处理器状态</h2>
        <button onClick={refreshAll}>刷新全部</button>
      </div>
      <div className="coproc-cards">
        {renderCard('NPU', npuState, 'npu state', npuTimeline)}
        {renderCard('LPU', lpuState, 'lpu state', lpuTimeline)}
      </div>
    </div>
  );
}
