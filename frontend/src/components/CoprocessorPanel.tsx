import { useCallback, useEffect, useState, type ReactNode } from 'react';
import type {
    GpuStateResponse,
    LpuStateResponse,
    NpuStateResponse,
    TpuStateResponse,
} from '../types/snapshot';
import { formatHex, formatHex64 } from '../utils/format';

interface CoprocessorPanelProps {
  sendCommand: (command: string) => void;
  npuState: NpuStateResponse | null;
  lpuState: LpuStateResponse | null;
  gpuState: GpuStateResponse | null;
  tpuState: TpuStateResponse | null;
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

// ── Shared card for NPU/LPU ──────────────────────────────────────────
interface CoprocessorCardProps {
  title: string;
  state: NpuStateResponse | LpuStateResponse | null;
  refreshCommand: string;
  timeline: TimelinePoint[];
  onRefresh: (command: string) => void;
  renderTimeline: (timeline: TimelinePoint[]) => React.ReactNode;
}

function isLpuState(state: NpuStateResponse | LpuStateResponse | null): state is LpuStateResponse {
  return Boolean(state && state.type === 'lpu_state');
}

function formatLpuOpcode(opcode: number | undefined): string {
  if (opcode === undefined) {
    return '--';
  }

  const language: Record<number, string> = {
    16: 'ByteTokenize',
    17: 'EmbeddingBag',
    18: 'GreedyDecode',
    19: 'TopKSampleDecode',
    20: 'TopPSampleDecode',
  };

  return language[opcode] ?? String(opcode);
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
            <div>
              <span>opcode</span>
              <strong>{isLpuState(state) ? formatLpuOpcode(state.opcode) : (state.opcode ?? '--')}</strong>
            </div>
            <div><span>cycles</span><strong>{state.cycles ?? '--'}</strong></div>
            <div><span>desc_addr</span><strong>{formatHex64(state.desc_addr)}</strong></div>
            <div><span>desc_len</span><strong>{state.desc_len ?? '--'}</strong></div>
            <div><span>tasks_done</span><strong>{state.tasks_done ?? '--'}</strong></div>
            <div><span>tasks_error</span><strong>{state.tasks_error ?? '--'}</strong></div>
            <div><span>notify_count</span><strong>{state.desc_notify_count ?? '--'}</strong></div>
            <div><span>pending</span><strong>{state.pending_desc_notify ? 'YES' : 'NO'}</strong></div>
            {isLpuState(state) && (
              <>
                <div><span>bytes_processed</span><strong>{state.bytes_processed ?? '--'}</strong></div>
                <div><span>tokens_generated</span><strong>{state.tokens_generated ?? '--'}</strong></div>
                <div><span>embedding_lookups</span><strong>{state.embedding_lookups ?? '--'}</strong></div>
                <div><span>embedding_bags</span><strong>{state.embedding_bags ?? '--'}</strong></div>
                <div><span>decode_candidates</span><strong>{state.decode_candidates_evaluated ?? '--'}</strong></div>
                <div><span>decoded_tokens</span><strong>{state.decoded_tokens ?? '--'}</strong></div>
                <div><span>decode_top_k</span><strong>{state.decode_top_k ?? '--'}</strong></div>
                <div><span>decode_top_p_milli</span><strong>{state.decode_top_p_milli ?? '--'}</strong></div>
                <div><span>decode_temp_milli</span><strong>{state.decode_temperature_milli ?? '--'}</strong></div>
                <div><span>decode_seed</span><strong>{state.decode_seed ?? '--'}</strong></div>
                <div><span>sampled_decodes</span><strong>{state.sampled_decodes ?? '--'}</strong></div>
                <div><span>nucleus_decodes</span><strong>{state.nucleus_decodes ?? '--'}</strong></div>
              </>
            )}
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

// ── GPU Card ─────────────────────────────────────────────────────────
interface GpuCardProps {
  state: GpuStateResponse | null;
  onRefresh: (command: string) => void;
}

const KERNEL_NAMES: Record<number, string> = {
  0: 'MatMul', 1: 'MatAdd', 10: 'Conv2d', 12: 'Pool2dMax', 13: 'Pool2dAvg',
  20: 'VectorAdd', 21: 'VectorMul', 22: 'VectorDot', 23: 'VectorScale',
  30: 'Relu', 31: 'Relu6', 32: 'Sigmoid', 33: 'Tanh', 34: 'Softmax', 35: 'LeakyRelu',
};

const PRECISION_NAMES: Record<number, string> = {
  0: 'FP32', 1: 'FP16', 2: 'INT8', 3: 'INT32', 4: 'BF16',
};

function GpuCard({ state, onRefresh }: GpuCardProps) {
  return (
    <div className="coproc-card">
      <div className="coproc-card-header">
        <h3>GPU</h3>
        <button onClick={() => onRefresh('gpu state')}>刷新</button>
      </div>

      {!state && <div className="coproc-empty">暂无状态，点击"刷新"获取</div>}
      {state && state.success === false && (
        <div className="coproc-error">{state.error ?? '读取失败'}</div>
      )}

      {state?.success && (
        <div className="coproc-grid">
          <div><span>control</span><strong>{formatHex(state.control)}</strong></div>
          <div><span>status</span><strong>{formatHex(state.status)}</strong></div>
          <div><span>kernel</span><strong>{KERNEL_NAMES[state.kernel_type ?? 0] ?? state.kernel_type}</strong></div>
          <div><span>precision</span><strong>{PRECISION_NAMES[state.precision ?? 0] ?? state.precision}</strong></div>
          <div><span>kernels_done</span><strong>{state.kernels_executed ?? 0}</strong></div>
          <div><span>cycles</span><strong>{state.cycles ?? 0}</strong></div>
          <div><span>ops</span><strong>{state.ops_count ?? 0}</strong></div>
          <div><span>bytes</span><strong>{state.bytes_transferred ?? 0}</strong></div>
          <div><span>tasks_done</span><strong>{state.tasks_done ?? 0}</strong></div>
          <div><span>tasks_error</span><strong>{state.tasks_error ?? 0}</strong></div>
          <div><span>queue_len</span><strong>{state.work_queue_len ?? 0}</strong></div>
          <div><span>conv_k</span><strong>{((state.conv_kernel_size ?? 0) >> 16) & 0xFFFF}x{(state.conv_kernel_size ?? 0) & 0xFFFF}</strong></div>
        </div>
      )}
    </div>
  );
}

// ── TPU Card ─────────────────────────────────────────────────────────
interface TpuCardProps {
  state: TpuStateResponse | null;
  onRefresh: (command: string) => void;
}

function TpuCard({ state, onRefresh }: TpuCardProps) {
  const formatScale = (bits: number | undefined) => {
    if (bits === undefined) return '--';
    return f32FromBits(bits).toFixed(4);
  };

  return (
    <div className="coproc-card">
      <div className="coproc-card-header">
        <h3>TPU</h3>
        <button onClick={() => onRefresh('tpu state')}>刷新</button>
      </div>

      {!state && <div className="coproc-empty">暂无状态，点击"刷新"获取</div>}
      {state && state.success === false && (
        <div className="coproc-error">{state.error ?? '读取失败'}</div>
      )}

      {state?.success && (
        <div className="coproc-grid">
          <div><span>control</span><strong>{formatHex(state.control)}</strong></div>
          <div><span>status</span><strong>{formatHex(state.status)}</strong></div>
          <div><span>MxNxK</span><strong>{state.m}x{state.n}x{state.k}</strong></div>
          <div><span>computed</span><strong>{state.matrices_computed ?? 0}</strong></div>
          <div><span>cycles</span><strong>{state.cycles ?? 0}</strong></div>
          <div><span>ops</span><strong>{state.ops_count ?? 0}</strong></div>
          <div><span>tasks_done</span><strong>{state.tasks_done ?? 0}</strong></div>
          <div><span>tasks_error</span><strong>{state.tasks_error ?? 0}</strong></div>
          <div><span>in_scale</span><strong>{formatScale(state.input_scale)}</strong></div>
          <div><span>out_scale</span><strong>{formatScale(state.output_scale)}</strong></div>
          <div><span>in_zp</span><strong>{state.input_zero_point ?? 0}</strong></div>
          <div><span>out_zp</span><strong>{state.output_zero_point ?? 0}</strong></div>
        </div>
      )}
    </div>
  );
}

// IEEE 754 float from u32 bits
function f32FromBits(bits: number): number {
  const buf = new ArrayBuffer(4);
  const u32 = new Uint32Array(buf);
  const f32 = new Float32Array(buf);
  u32[0] = bits >>> 0;
  return f32[0];
}

// ── Timeline hook ────────────────────────────────────────────────────
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

    // eslint-disable-next-line react-hooks/set-state-in-effect
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

// ── Main panel ───────────────────────────────────────────────────────
export function CoprocessorPanel({ sendCommand, npuState, lpuState, gpuState, tpuState }: CoprocessorPanelProps) {
  const npuTimeline = useTimeline(npuState, MAX_TIMELINE_POINTS);
  const lpuTimeline = useTimeline(lpuState, MAX_TIMELINE_POINTS);

  const refreshAll = useCallback(() => {
    sendCommand('npu state');
    sendCommand('lpu state');
    sendCommand('gpu state');
    sendCommand('tpu state');
  }, [sendCommand]);

  const handleRefresh = useCallback((command: string) => {
    sendCommand(command);
  }, [sendCommand]);

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
        <h2>协处理器状态</h2>
        <button onClick={refreshAll}>刷新全部</button>
      </div>
      <div className="coproc-cards">
        <CoprocessorCard title="NPU" state={npuState} refreshCommand="npu state" timeline={npuTimeline} onRefresh={handleRefresh} renderTimeline={renderTimeline} />
        <CoprocessorCard title="LPU (Language)" state={lpuState} refreshCommand="lpu state" timeline={lpuTimeline} onRefresh={handleRefresh} renderTimeline={renderTimeline} />
        <GpuCard state={gpuState} onRefresh={handleRefresh} />
        <TpuCard state={tpuState} onRefresh={handleRefresh} />
      </div>
    </div>
  );
}
