import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { FramebufferGameResponse, InputStateResponse } from '../types/snapshot';

type GameFlowPanelProps = {
  sendCommand: (command: string) => void;
  gameState: FramebufferGameResponse | null;
  inputState: InputStateResponse | null;
};

export const GameFlowPanel: React.FC<GameFlowPanelProps> = ({
  sendCommand,
  gameState,
  inputState,
}) => {
  const [running, setRunning] = useState(false);
  const [intervalMs, setIntervalMs] = useState(120);
  const tickCounterRef = useRef(0);

  const syncState = useCallback(() => {
    sendCommand('fb_game state');
    sendCommand('input state');
    sendCommand('fb linux');
  }, [sendCommand]);

  const stepOnce = useCallback(() => {
    sendCommand('fb_game step');
    sendCommand('fb linux');
    tickCounterRef.current += 1;
    if (tickCounterRef.current % 4 === 0) {
      sendCommand('input state');
    }
  }, [sendCommand]);

  const initGame = useCallback(() => {
    setRunning(false);
    sendCommand('fb_game init');
    sendCommand('input clear');
    syncState();
  }, [sendCommand, syncState]);

  const resetGame = useCallback(() => {
    setRunning(false);
    sendCommand('fb_game reset');
    sendCommand('input clear');
    syncState();
  }, [sendCommand, syncState]);

  useEffect(() => {
    if (!running) {
      return;
    }

    const timer = window.setInterval(() => {
      stepOnce();
    }, Math.max(30, intervalMs));

    return () => {
      window.clearInterval(timer);
    };
  }, [intervalMs, running, stepOnce]);

  useEffect(() => {
    syncState();
  }, [syncState]);

  const statusText = useMemo(() => {
    if (!gameState?.success) {
      return '尚未获取游戏状态';
    }

    const tick = gameState.tick ?? 0;
    const left = gameState.score_left ?? 0;
    const right = gameState.score_right ?? 0;
    return `tick=${tick} score=${left}:${right}`;
  }, [gameState]);

  return (
    <div className="game-flow-panel">
      <div className="game-flow-header">
        <h3>Game Flow</h3>
        <div className={`game-run-indicator ${running ? 'running' : 'paused'}`}>
          {running ? 'Running' : 'Paused'}
        </div>
      </div>

      <div className="game-flow-controls">
        <button onClick={initGame}>Init</button>
        <button onClick={stepOnce}>Step</button>
        {running ? (
          <button onClick={() => setRunning(false)}>Pause</button>
        ) : (
          <button onClick={() => setRunning(true)}>Run</button>
        )}
        <button onClick={resetGame}>Reset</button>
        <button onClick={syncState}>Sync</button>

        <label className="game-flow-interval">
          Interval(ms)
          <input
            type="number"
            min={30}
            max={1000}
            value={intervalMs}
            onChange={(event) => setIntervalMs(Number(event.target.value) || 120)}
          />
        </label>
      </div>

      <div className="game-flow-meta">
        <div>{statusText}</div>
        <div>
          input_state={
            inputState?.key_state !== undefined
              ? `0x${inputState.key_state.toString(16).toUpperCase()}`
              : 'N/A'
          }
          {inputState?.irq_pending !== undefined ? ` irq=${inputState.irq_pending}` : ''}
        </div>
      </div>
    </div>
  );
};
