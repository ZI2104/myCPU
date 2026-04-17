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
  const tlbLookups = safe(perf.tlb_lookups);
  const tlbActive = perf.tlb_active === true;
  const tlbBypassReason = (perf.tlb_bypass_reason ?? '').trim();
  const baselineWriteTraffic = safe(perf.memory_writes);
  const currentWriteTraffic = safe(perf.cache_writebacks);
  const savedWriteTraffic = Math.max(0, baselineWriteTraffic - currentWriteTraffic);
  const savedWriteTrafficRate =
    baselineWriteTraffic > 0 ? (savedWriteTraffic / baselineWriteTraffic) * 100 : null;

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

      <div className="mh-summary mh-summary-tlb">
        <div className="mh-summary-item">
          <span>TLB Status</span>
          <span className={tlbActive ? 'mh-status-active' : 'mh-status-inactive'}>
            {tlbActive ? 'Active (Sv32)' : 'Bypassed'}
          </span>
        </div>
        <div className="mh-summary-item">
          <span>TLB Lookups</span>
          <span>{formatNumber(tlbLookups)}</span>
        </div>
        {!tlbActive && tlbBypassReason.length > 0 && (
          <div className="mh-note">Reason: {tlbBypassReason}</div>
        )}
      </div>

      <div className="mh-compare">
        <div className="mh-compare-title">Cache Policy Compare Demo</div>
        <div className="mh-compare-grid">
          <div className="mh-compare-card current">
            <div className="mh-compare-name">Current: Set-Assoc + Write-Back + Write-Allocate</div>
            <div className="mh-row">
              <span>Cache Hit Rate</span>
              <span>{cacheStat.hitRate === null ? 'N/A' : `${cacheStat.hitRate.toFixed(1)}%`}</span>
            </div>
            <div className="mh-row">
              <span>Off-chip Writes (writebacks)</span>
              <span>{formatNumber(currentWriteTraffic)}</span>
            </div>
          </div>

          <div className="mh-compare-card baseline">
            <div className="mh-compare-name">Baseline Demo: Direct-Mapped + Write-Through</div>
            <div className="mh-row">
              <span>Assumed Off-chip Writes</span>
              <span>{formatNumber(baselineWriteTraffic)}</span>
            </div>
            <div className="mh-row">
              <span>Write Traffic Saved</span>
              <span>
                {formatNumber(savedWriteTraffic)}
                {savedWriteTrafficRate === null ? '' : ` (${savedWriteTrafficRate.toFixed(1)}%)`}
              </span>
            </div>
          </div>
        </div>
        <div className="mh-note">
          Baseline is an in-panel demo estimate: each store is treated as an off-chip write.
          Current mode uses measured cache writeback count from simulator runtime.
        </div>
      </div>
    </div>
  );
};
