import React from 'react';

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
  return (
    <div className="control-panel">
      <div className="connection-status">
        <span className={`status-indicator ${connected ? 'connected' : 'disconnected'}`} />
        {connected ? 'Connected' : 'Disconnected'}
      </div>

      <div className="controls">
        <button
          onClick={onStep}
          disabled={!connected || running}
          className="btn btn-step"
        >
          Step
        </button>

        {running ? (
          <button
            onClick={onPause}
            disabled={!connected}
            className="btn btn-pause"
          >
            Pause
          </button>
        ) : (
          <button
            onClick={onRun}
            disabled={!connected}
            className="btn btn-run"
          >
            Run
          </button>
        )}

        <button
          onClick={onReset}
          disabled={!connected}
          className="btn btn-reset"
        >
          Reset
        </button>
      </div>

      <div className="speed-control">
        <label>Speed: {speed === 0 ? 'Unlimited' : `${speed} cyc/s`}</label>
        <input
          type="range"
          min="0"
          max="100"
          value={speed}
          onChange={(e) => onSpeedChange(parseInt(e.target.value))}
          disabled={!connected}
        />
      </div>
    </div>
  );
};
