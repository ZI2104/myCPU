import React from 'react';
import type { PerfSnapshot } from '../types/snapshot';
import { formatNumber } from '../utils/format';

interface PerformanceDashboardProps {
  perf: PerfSnapshot | null;
  mode?: 'full' | 'summary' | 'details';
}

const safe = (n: number | null | undefined): number =>
  (typeof n === 'number' && Number.isFinite(n) ? n : 0);

const ProgressBar: React.FC<{
  value: number;
  max: number;
  label: string;
  color?: string;
}> = ({ value, max, label, color = '#4CAF50' }) => {
  const safeValue = safe(value);
  const safeMax = safe(max);
  const percentage = safeMax > 0 ? (safeValue / safeMax) * 100 : 0;
  const clamped = Math.max(0, Math.min(percentage, 100));
  return (
    <div className="progress-item">
      <div className="progress-label">{label}</div>
      <div className="progress-bar">
        <div
          className="progress-fill"
          style={{ width: `${clamped}%`, backgroundColor: color }}
        />
      </div>
      <div className="progress-value">{clamped.toFixed(1)}%</div>
    </div>
  );
};

export const PerformanceDashboard: React.FC<PerformanceDashboardProps> = ({
  perf,
  mode = 'full',
}) => {
  if (!perf) {
    return <div className="perf-dashboard">No performance data</div>;
  }

  const ipcPercentage = safe(perf.ipc) * 100;
  const stallPercentage = safe(perf.cycles) > 0 ? (safe(perf.stalls) / safe(perf.cycles)) * 100 : 0;
  const showSummary = mode === 'full' || mode === 'summary';
  const showDetails = mode === 'full' || mode === 'details';

  return (
    <div className="perf-dashboard">
      <h3>Performance Dashboard</h3>

      {showSummary && (
        <div className="perf-grid">
          <div className="perf-card ipc">
            <div className="perf-value">{safe(perf.ipc).toFixed(3)}</div>
            <div className="perf-label">IPC</div>
            <ProgressBar value={ipcPercentage} max={100} label="" color="#2196F3" />
          </div>

          <div className="perf-card cycles">
            <div className="perf-value">{formatNumber(safe(perf.cycles))}</div>
            <div className="perf-label">Cycles</div>
          </div>

          <div className="perf-card instructions">
            <div className="perf-value">{formatNumber(safe(perf.instructions))}</div>
            <div className="perf-label">Instructions</div>
          </div>

          <div className="perf-card stalls">
            <div className="perf-value">{formatNumber(safe(perf.stalls))}</div>
            <div className="perf-label">Stalls</div>
            <ProgressBar value={stallPercentage} max={100} label="" color="#FF5722" />
          </div>
        </div>
      )}

      {showDetails && (
        <div className="perf-details">
          <div className="detail-row">
            <span>Load-Use Stalls:</span>
            <span>{formatNumber(safe(perf.load_use_stalls))}</span>
          </div>
          <ProgressBar
            value={safe(perf.load_use_stall_rate)}
            max={100}
            label="Load-Use Cycle Rate"
            color="#FF9800"
          />
          <div className="detail-row">
            <span>Control Hazards:</span>
            <span>{formatNumber(safe(perf.control_hazards))}</span>
          </div>
          <ProgressBar
            value={safe(perf.control_hazard_rate)}
            max={100}
            label="Control Hazard Cycle Rate"
            color="#E91E63"
          />
          {perf.branch_accuracy !== null && (
            <div className="detail-row">
              <span>Branch Accuracy:</span>
              <span>{safe(perf.branch_accuracy).toFixed(1)}%</span>
            </div>
          )}
          <div className="detail-row">
            <span>Memory Reads:</span>
            <span>{formatNumber(safe(perf.memory_reads))}</span>
          </div>
          <div className="detail-row">
            <span>Memory Writes:</span>
            <span>{formatNumber(safe(perf.memory_writes))}</span>
          </div>
          {typeof perf.cache_hits === 'number' && (
            <div className="detail-row">
              <span>Cache Hits:</span>
              <span>{formatNumber(safe(perf.cache_hits))}</span>
            </div>
          )}
          {typeof perf.cache_misses === 'number' && (
            <div className="detail-row">
              <span>Cache Misses:</span>
              <span>{formatNumber(safe(perf.cache_misses))}</span>
            </div>
          )}
          {typeof perf.tlb_hits === 'number' && (
            <div className="detail-row">
              <span>TLB Hits:</span>
              <span>{formatNumber(safe(perf.tlb_hits))}</span>
            </div>
          )}
          {typeof perf.tlb_misses === 'number' && (
            <div className="detail-row">
              <span>TLB Misses:</span>
              <span>{formatNumber(safe(perf.tlb_misses))}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
