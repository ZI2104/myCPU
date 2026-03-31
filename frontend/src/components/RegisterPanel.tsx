import React, { useState } from 'react';
import { REGISTER_NAMES } from '../types/snapshot';

interface RegisterPanelProps {
  registers: number[];
  previousRegisters?: number[];
}

const formatHex = (value: number): string => {
  return '0x' + (value >>> 0).toString(16).toUpperCase().padStart(8, '0');
};

const formatDecimal = (value: number): string => {
  // Handle as signed 32-bit
  const signed = value | 0;
  return signed.toString();
};

export const RegisterPanel: React.FC<RegisterPanelProps> = ({
  registers,
  previousRegisters,
}) => {
  const [displayMode, setDisplayMode] = useState<'hex' | 'dec'>('hex');

  const formatValue = (value: number): string => {
    return displayMode === 'hex' ? formatHex(value) : formatDecimal(value);
  };

  const isModified = (index: number): boolean => {
    if (!previousRegisters) return false;
    return registers[index] !== previousRegisters[index];
  };

  return (
    <div className="register-panel">
      <div className="panel-header">
        <h3>Registers</h3>
        <div className="display-mode">
          <button
            className={displayMode === 'hex' ? 'active' : ''}
            onClick={() => setDisplayMode('hex')}
          >
            Hex
          </button>
          <button
            className={displayMode === 'dec' ? 'active' : ''}
            onClick={() => setDisplayMode('dec')}
          >
            Dec
          </button>
        </div>
      </div>
      <div className="register-list">
        {registers.map((value, index) => (
          <div
            key={index}
            className={`register-row ${isModified(index) ? 'modified' : ''}`}
          >
            <span className="reg-name">
              x{index.toString().padStart(2, '0')}
            </span>
            <span className="reg-alias">
              ({REGISTER_NAMES[index]})
            </span>
            <span className="reg-value">
              {formatValue(value)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
};
