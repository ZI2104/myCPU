import React from 'react';
import { useInlineEdit } from '../hooks/useInlineEdit';

interface ControlPanelProps {
  connected: boolean;
  running: boolean;
  onStep: () => void;
  onRun: () => void;
  onPause: () => void;
  onReset: () => void;
  speed: number;
  onSpeedChange: (speed: number) => void;
}

export const ControlPanel: React.FC<ControlPanelProps> = ({
  connected,
  running,
  onStep,
  onRun,
  onPause,
  onReset,
  speed,
  onSpeedChange,
}) => {
  const {
    isEditing,
    inputValue,
    startEdit,
    handleInputChange,
    handleSubmit,
    handleKeyDown,
  } = useInlineEdit({
    value: speed,
    onSubmit: onSpeedChange,
    parse: (input) => {
      const value = parseInt(input, 10);
      if (isNaN(value) || value < 0 || value > 100) return null;
      return value;
    },
  });

  return (
    <div className="control-panel">
      <div className="connection-status">
        <span className={`status-indicator ${connected ? 'connected' : 'disconnected'}`} />
        {connected ? 'Connected' : 'Disconnected'}
      </div>

      <div className="controls">
        <button
          type="button"
          onClick={onStep}
          disabled={!connected || running}
          className="btn btn-step"
        >
          Step
        </button>

        {running ? (
          <button
            type="button"
            onClick={onPause}
            disabled={!connected}
            className="btn btn-pause"
          >
            Pause
          </button>
        ) : (
          <button
            type="button"
            onClick={onRun}
            disabled={!connected}
            className="btn btn-run"
          >
            Run
          </button>
        )}

        <button
          type="button"
          onClick={onReset}
          disabled={!connected}
          className="btn btn-reset"
        >
          Reset
        </button>
      </div>

      <div className="speed-control">
        <label>Speed:</label>
        <input
          type="range"
          min="0"
          max="100"
          value={speed}
          onChange={(e) => onSpeedChange(parseInt(e.target.value))}
          disabled={!connected}
        />
        {isEditing ? (
          <input
            type="number"
            min="0"
            max="100"
            value={inputValue}
            onChange={handleInputChange}
            onBlur={handleSubmit}
            onKeyDown={handleKeyDown}
            disabled={!connected}
            className="speed-input"
            autoFocus
          />
        ) : (
          <span
            className="speed-value"
            onClick={startEdit}
            title="点击编辑"
          >
            {speed === 0 ? 'Unlimited' : `${speed}`}
          </span>
        )}
        <span className="speed-unit">cyc/s</span>
      </div>
    </div>
  );
};
