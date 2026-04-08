import React, { useState } from 'react';
import { REGISTER_NAMES } from '../types/snapshot';
import { formatHex, formatDecimal } from '../utils/format';

interface RegisterPanelProps {
  registers: number[];
  previousRegisters?: number[];
  highlightRegisters?: number[];
}

export const RegisterPanel: React.FC<RegisterPanelProps> = ({
  registers,
  previousRegisters,
  highlightRegisters = [],
}) => {
  const [displayMode, setDisplayMode] = useState<'hex' | 'dec'>('hex');

  const formatValue = (value: number): string => {
    return displayMode === 'hex' ? formatHex(value) : formatDecimal(value);
  };

  const isModified = (index: number): boolean => {
    if (!previousRegisters) return false;
    return registers[index] !== previousRegisters[index];
  };

  const isHighlighted = (index: number): boolean => {
    return highlightRegisters?.includes(index);
  };

  return (
    <div className="register-panel">
      <div className="panel-header">
        <h3>Registers</h3>
        <div className="display-mode">
          <button
            type="button"
            className={displayMode === 'hex' ? 'active' : ''}
            onClick={() => setDisplayMode('hex')}
          >
            Hex
          </button>
          <button
            type="button"
            className={displayMode === 'dec' ? 'active' : ''}
            onClick={() => setDisplayMode('dec')}
          >
            Dec
          </button>
        </div>
      </div>
      <div className="register-grid">
        {registers.map((value, index) => (
          <div
            key={index}
            className={
              `register-item ${isModified(index) ? 'modified' : ''} ${isHighlighted(index) ? 'highlight-read' : ''}`
            }
            title={`${REGISTER_NAMES[index]}`}
          >
            <span className="reg-index">x{index}</span>
            <span className="reg-value">{formatValue(value)}</span>
          </div>
        ))}
      </div>
    </div>
  );
};
