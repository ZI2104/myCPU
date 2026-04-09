import React, { useState } from 'react';
import type { PredictorSnapshot } from '../types/snapshot';
import { PREDICTOR_TYPES } from '../types/snapshot';
import { formatHex, formatNumber } from '../utils/format';

interface PredictorPanelProps {
  predictor: PredictorSnapshot | null | undefined;
  onSwitchPredictor: (type: string) => void;
}

const AccuracyBadge: React.FC<{ accuracy: number | null }> = ({ accuracy }) => {
  if (accuracy === null) {
    return <span className="accuracy-badge na">N/A</span>;
  }
  const colorClass =
    accuracy >= 90 ? 'good' : accuracy >= 70 ? 'ok' : 'poor';
  return (
    <span className={`accuracy-badge ${colorClass}`}>
      {accuracy.toFixed(1)}%
    </span>
  );
};

export const PredictorPanel: React.FC<PredictorPanelProps> = ({
  predictor,
  onSwitchPredictor,
}) => {
  const [switching, setSwitching] = useState(false);

  const handleSwitch = (type: string) => {
    setSwitching(true);
    onSwitchPredictor(type);
    setTimeout(() => setSwitching(false), 300);
  };

  if (!predictor) {
    return (
      <div className="predictor-panel">
        <h3>Branch Predictor</h3>
        <div className="predictor-empty">No predictor data</div>
      </div>
    );
  }

  const hitRate =
    predictor.btb_hit_rate !== null ? predictor.btb_hit_rate : 0;
  const hitRateColorClass =
    hitRate >= 80 ? 'good' : hitRate >= 50 ? 'ok' : 'poor';

  return (
    <div className="predictor-panel">
      <h3>Branch Predictor</h3>

      <div className="predictor-selector">
        <label>Type:</label>
        <select
          value={predictor.predictor_type}
          onChange={(e) => handleSwitch(e.target.value)}
          disabled={switching}
          aria-label="Branch predictor type"
        >
          {PREDICTOR_TYPES.map((pt) => (
            <option key={pt.value} value={pt.value}>
              {pt.label}
            </option>
          ))}
        </select>
      </div>

      <div className="predictor-section">
        <div className="predictor-card accuracy-card">
          <div className="card-header">Prediction Accuracy</div>
          <div className="card-value">
            <AccuracyBadge accuracy={predictor.accuracy} />
          </div>
          <div className="card-detail">
            <span>
              {formatNumber(predictor.correct)} / {formatNumber(predictor.predictions)}
            </span>
            <span className="detail-separator">|</span>
            <span className="mispredict-count">
              {formatNumber(predictor.mispredictions)} mispredictions
            </span>
          </div>
        </div>
      </div>

      <div className="predictor-section">
        <div className="predictor-card">
          <div className="card-header">BTB (Branch Target Buffer)</div>
          <div className="btb-stats">
            <div className="btb-stat">
              <span className="stat-label">Hit Rate</span>
              <div className="progress-bar">
                <div
                  className={`progress-fill ${hitRateColorClass}`}
                  style={{ width: `${hitRate}%` }}
                />
              </div>
              <span className="stat-value">
                {predictor.btb_hit_rate !== null
                  ? `${predictor.btb_hit_rate.toFixed(1)}%`
                  : 'N/A'}
              </span>
            </div>
            <div className="btb-detail">
              <span>Hits: {formatNumber(predictor.btb_hits)}</span>
              <span>Misses: {formatNumber(predictor.btb_misses)}</span>
              <span>Lookups: {formatNumber(predictor.btb_lookups)}</span>
            </div>
          </div>
        </div>
      </div>

      {predictor.btb_entries.length > 0 && (
        <div className="predictor-section">
          <div className="predictor-card">
            <div className="card-header">
              BTB Entries ({predictor.btb_entries.length})
            </div>
            <div className="btb-table-wrapper">
              <table className="btb-table">
                <thead>
                  <tr>
                    <th>Tag</th>
                    <th>Target</th>
                    <th>Type</th>
                  </tr>
                </thead>
                <tbody>
                  {predictor.btb_entries.map((entry, i) => (
                    <tr key={i}>
                      <td className="hex">{formatHex(entry.tag)}</td>
                      <td className="hex">{formatHex(entry.target)}</td>
                      <td>{entry.is_branch ? 'Branch' : 'Jump'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
