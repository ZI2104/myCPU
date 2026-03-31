import React from 'react';
import type { PerfSnapshot } from '../types/snapshot';

interface PerformanceDashboardProps {
  perf: PerfSnapshot | null;
}

const formatNumber = (n: number): string => {
  return n.toLocaleString();
};

const ProgressBar: React.FC<{
  value: number;
  max: number;
  label: string;
  color?: string;
}> = ({ value, max, label, color = '#4CAF50' }) => {
  const percentage = max > 0 ? (value / max) * 100 : 0;
  return (
    <div className="progress-item">
      <div className="progress-label">{label}</div>
      <div className="progress-bar">
        <div
          className="progress-fill"
          style={{ width: `${Math.min(percentage, 100)}%`, backgroundColor: color }}
        />
      </div>
      <div className="progress-value">{(percentage).toFixed(1)}%</div>
    </div>
  );
};

export const PerformanceDashboard: React.FC<PerformanceDashboardProps> = ({ perf }) => {
  if (!perf) {
    return <div className="perf-dashboard">No performance data</div>;
  }

  const ipcPercentage = perf.ipc * 100;
  const stallPercentage = perf.cycles > 0 ? (perf.stalls / perf.cycles) * 100 : 0;

  return (
    <div className="perf-dashboard">
      <h3>Performance Dashboard</h3>

      <div className="perf-grid">
        <div className="perf-card ipc">
          <div className="perf-value">{perf.ipc.toFixed(3)}</div>
          <div className="perf-label">IPC</div>
          <ProgressBar value={ipcPercentage} max={100} label="" color="#2196F3" />
        </div>

        <div className="perf-card cycles">
          <div className="perf-value">{formatNumber(perf.cycles)}</div>
          <div className="perf-label">Cycles</div>
        </div>

        <div className="perf-card instructions">
          <div className="perf-value">{formatNumber(perf.instructions)}</div>
          <div className="perf-label">Instructions</div>
        </div>

        <div className="perf-card stalls">
          <div className="perf-value">{formatNumber(perf.stalls)}</div>
          <div className="perf-label">Stalls</div>
          <ProgressBar value={stallPercentage} max={100} label="" color="#FF5722" />
        </div>
      </div>

      <div className="perf-details">
        <div className="detail-row">
          <span>Load-Use Stalls:</span>
          <span>{formatNumber(perf.load_use_stalls)}</span>
        </div>
        <div className="detail-row">
          <span>Control Hazards:</span>
          <span>{formatNumber(perf.control_hazards)}</span>
        </div>
        {perf.branch_accuracy !== null && (
          <div className="detail-row">
            <span>Branch Accuracy:</span>
            <span>{perf.branch_accuracy.toFixed(1)}%</span>
          </div>
        )}
        <div className="detail-row">
          <span>Memory Reads:</span>
          <span>{formatNumber(perf.memory_reads)}</span>
        </div>
        <div className="detail-row">
          <span>Memory Writes:</span>
          <span>{formatNumber(perf.memory_writes)}</span>
        </div>
      </div>
    </div>
  );
};
