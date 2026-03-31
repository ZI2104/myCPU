import { useState, useCallback, useRef, useEffect } from 'react';
import { useWebSocket } from './hooks/useWebSocket';
import { ControlPanel } from './components/ControlPanel';
import { PipelineVisualizer } from './components/PipelineVisualizer';
import { RegisterPanel } from './components/RegisterPanel';
import { PerformanceDashboard } from './components/PerformanceDashboard';
import { MemoryView } from './components/MemoryView';
import type { CpuSnapshot, MemoryReadResponse } from './types/snapshot';
import './App.css';

const WS_URL = 'ws://127.0.0.1:8080';

function App() {
  const [running, setRunning] = useState(false);
  const [speed, setSpeed] = useState(10);
  const [previousSnapshot, setPreviousSnapshot] = useState<CpuSnapshot | null>(null);
  const [activeTab, setActiveTab] = useState<'pipeline' | 'memory'>('pipeline');
  const memoryHandlersRef = useRef<((data: MemoryReadResponse) => void)[]>([]);

  const { snapshot, connected, send, error, lastMessage } = useWebSocket(WS_URL);

  // Handle memory data responses
  useEffect(() => {
    if (lastMessage) {
      try {
        const data = JSON.parse(lastMessage);
        if (data.addr !== undefined && data.data !== undefined) {
          // This is a memory response
          const memResponse = data as MemoryReadResponse;
          memoryHandlersRef.current.forEach(handler => handler(memResponse));
        }
      } catch {
        // Not JSON or not a memory response
      }
    }
  }, [lastMessage]);

  const registerMemoryHandler = useCallback((handler: (data: MemoryReadResponse) => void) => {
    memoryHandlersRef.current.push(handler);
  }, []);

  const handleStep = useCallback(() => {
    setPreviousSnapshot(snapshot);
    send('step');
  }, [send, snapshot]);

  const handleRun = useCallback(() => {
    setRunning(true);
    send('run');
  }, [send]);

  const handlePause = useCallback(() => {
    setRunning(false);
    send('pause');
  }, [send]);

  const handleReset = useCallback(() => {
    setRunning(false);
    send('reset');
  }, [send]);

  const handleSpeedChange = useCallback((newSpeed: number) => {
    setSpeed(newSpeed);
    send(`speed ${newSpeed}`);
  }, [send]);

  return (
    <div className="app">
      <header className="app-header">
        <h1>myCPU Visualizer</h1>
        <p>RISC-V RV32I Pipeline Simulator</p>
      </header>

      <ControlPanel
        connected={connected}
        running={running}
        onStep={handleStep}
        onRun={handleRun}
        onPause={handlePause}
        onReset={handleReset}
        speed={speed}
        onSpeedChange={handleSpeedChange}
      />

      {error && <div className="error-message">{error}</div>}

      {snapshot && (
        <>
          <div className="main-content">
            <div className="left-panel">
              <RegisterPanel
                registers={snapshot.registers}
                previousRegisters={previousSnapshot?.registers}
              />
            </div>

            <div className="center-panel">
              <div className="pc-display">
                <h3>Program Counter</h3>
                <div className="pc-value">
                  0x{snapshot.pc.toString(16).toUpperCase().padStart(8, '0')}
                </div>
                <div className="privilege">Mode: {snapshot.privilege}</div>
              </div>

              {/* Tab switcher */}
              <div className="tab-bar">
                <button
                  className={activeTab === 'pipeline' ? 'active' : ''}
                  onClick={() => setActiveTab('pipeline')}
                >
                  Pipeline
                </button>
                <button
                  className={activeTab === 'memory' ? 'active' : ''}
                  onClick={() => setActiveTab('memory')}
                >
                  Memory
                </button>
              </div>

              {activeTab === 'pipeline' && (
                <PipelineVisualizer pipeline={snapshot.pipeline} />
              )}

              {activeTab === 'memory' && (
                <MemoryView
                  sendCommand={send}
                  onMemoryData={registerMemoryHandler}
                />
              )}
            </div>

            <div className="right-panel">
              <PerformanceDashboard perf={snapshot.perf} />
            </div>
          </div>

          {snapshot.halted && (
            <div className="halted-message">
              CPU Halted - Execution Complete
            </div>
          )}
        </>
      )}

      {!snapshot && connected && (
        <div className="waiting">
          Waiting for CPU state...
        </div>
      )}

      {!connected && (
        <div className="disconnected">
          <p>Connect to myCPU visualization server:</p>
          <code>cargo run -- visualize &lt;program.elf&gt;</code>
        </div>
      )}
    </div>
  );
}

export default App;
