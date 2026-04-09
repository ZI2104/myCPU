import React from 'react';
import type { PerfSnapshot } from '../types/snapshot';
import { formatNumber } from '../utils/format';

interface MemoryHierarchyPanelProps {
  perf: PerfSnapshot | null;
}

const safe = (n: number | null | undefined): number =>
  (typeof n === 'number' && Number.isFinite(n) ? n : 0);

type Stat = {
  hits: number;
  misses: number;
  total: number;
  hitRate: number | null;
  missRate: number | null;
};

const buildStat = (hitsRaw: number | null | undefined, missesRaw: number | null | undefined): Stat => {
  const hits = safe(hitsRaw);
  const misses = safe(missesRaw);
  const total = hits + misses;
  if (total === 0) {
    return {
      hits,
      misses,
      total,
      hitRate: null,
      missRate: null,
    };
  }

  return {
    hits,
    misses,
    total,
    hitRate: (hits / total) * 100,
    missRate: (misses / total) * 100,
  };
};

const RateBar: React.FC<{ label: string; value: number | null }> = ({ label, value }) => {
  const shown = value ?? 0;
  const ariaValue = value === null ? 'N/A' : `${shown.toFixed(1)}%`;
  return (
    <div className="mh-rate-row">
      <span className="mh-rate-label">{label}</span>
      <progress
        className="mh-rate-progress"
        max={100}
        value={shown}
        aria-label={`${label}: ${ariaValue}`}
      />
      <span className="mh-rate-value">{value === null ? 'N/A' : `${shown.toFixed(1)}%`}</span>
    </div>
  );
};

const StatCard: React.FC<{ title: string; stat: Stat; kind: 'cache' | 'tlb' }> = ({ title, stat, kind }) => {
  return (
    <div className="mh-card">
      <div className="mh-card-title">{title}</div>
      <div className="mh-rows">
        <div className="mh-row">
          <span>Hits</span>
          <span>{formatNumber(stat.hits)}</span>
        </div>
        <div className="mh-row">
          <span>Misses</span>
          <span>{formatNumber(stat.misses)}</span>
        </div>
        <div className="mh-row">
          <span>Total Accesses</span>
          <span>{formatNumber(stat.total)}</span>
        </div>
      </div>
      <div className="mh-rates">
        <RateBar label="Hit Rate" value={stat.hitRate} />
        <RateBar label="Miss Rate" value={stat.missRate} />
      </div>
      <div className="mh-note">
        {kind === 'cache'
          ? 'Cache uses I-Cache + D-Cache aggregate counters.'
          : 'TLB counters reflect translation lookup behavior.'}
      </div>
    </div>
  );
};

/**
 * Memory hierarchy telemetry panel.
 *
 * Displays cache/TLB hit-miss statistics and memory read/write totals.
 * When `perf` is null, the panel shows an empty-state message.
 */
export const MemoryHierarchyPanel: React.FC<MemoryHierarchyPanelProps> = ({ perf }) => {
  if (!perf) {
    return <div className="memory-hierarchy-panel">No memory hierarchy data</div>;
  }

  const cacheStat = buildStat(perf.cache_hits, perf.cache_misses);
  const tlbStat = buildStat(perf.tlb_hits, perf.tlb_misses);

  return (
    <div className="memory-hierarchy-panel">
      <h3>Memory Hierarchy</h3>

      <div className="mh-summary">
        <div className="mh-summary-item">
          <span>Memory Reads</span>
          <span>{formatNumber(safe(perf.memory_reads))}</span>
        </div>
        <div className="mh-summary-item">
          <span>Memory Writes</span>
          <span>{formatNumber(safe(perf.memory_writes))}</span>
        </div>
      </div>

      <div className="mh-cards">
        <StatCard title="Cache" stat={cacheStat} kind="cache" />
        <StatCard title="TLB" stat={tlbStat} kind="tlb" />
      </div>
    </div>
  );
};
