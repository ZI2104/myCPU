import { useCallback, useEffect, useRef, useState } from 'react';
import './App.css';
import { ControlPanel } from './components/ControlPanel';
import { CoprocessorPanel } from './components/CoprocessorPanel';
import { DebugInspector } from './components/DebugInspector';
import { FramebufferView } from './components/FramebufferView';
import { GameFlowPanel } from './components/GameFlowPanel';
import { InputPanel } from './components/InputPanel';
import { MemoryView } from './components/MemoryView';
import { PerformanceDashboard } from './components/PerformanceDashboard';
import { PipelineVisualizer } from './components/PipelineVisualizer';
import { RegisterPanel } from './components/RegisterPanel';
import { useWebSocket } from './hooks/useWebSocket';
import type {
    Breakpoint,
    CpuSnapshot,
    DisassemblyResponse,
    FramebufferGameResponse,
    FramebufferResponse,
    HistoryResponse,
    InputStateResponse,
    LpuStateResponse,
    MemoryReadResponse,
    NpuStateResponse,
} from './types/snapshot';

const WS_URL = 'ws://127.0.0.1:8080';

function App() {
  const [running, setRunning] = useState(false);
  const [speed, setSpeed] = useState(10);
  const [previousSnapshot, setPreviousSnapshot] = useState<CpuSnapshot | null>(null);
  const [activeTab, setActiveTab] = useState<'pipeline' | 'memory' | 'framebuffer' | 'coprocessor' | 'debug'>('pipeline');
  const [disassembly, setDisassembly] = useState<DisassemblyResponse | null>(null);
  const [history, setHistory] = useState<HistoryResponse | null>(null);
  const [breakpoints, setBreakpoints] = useState<Breakpoint[]>([]);
  const [gameState, setGameState] = useState<FramebufferGameResponse | null>(null);
  const [inputState, setInputState] = useState<InputStateResponse | null>(null);
  const [npuState, setNpuState] = useState<NpuStateResponse | null>(null);
  const [lpuState, setLpuState] = useState<LpuStateResponse | null>(null);
  const memoryHandlersRef = useRef<((data: MemoryReadResponse) => void)[]>([]);
  const framebufferHandlersRef = useRef<((data: FramebufferResponse) => void)[]>([]);

  const { snapshot, connected, send, error, lastMessage } = useWebSocket(WS_URL);

  // Handle memory data responses
  useEffect(() => {
    if (lastMessage) {
      try {
        const data = JSON.parse(lastMessage);
        if (data.addr !== undefined && data.data !== undefined && data.success !== undefined) {
          // This is a memory response
          const memResponse = data as MemoryReadResponse;
          memoryHandlersRef.current.forEach(handler => handler(memResponse));
        } else if (data.type === 'framebuffer' && data.width !== undefined && data.height !== undefined) {
          const fbResponse = data as FramebufferResponse;
          framebufferHandlersRef.current.forEach(handler => handler(fbResponse));
        } else if (data.type === 'framebuffer_game') {
          setGameState(data as FramebufferGameResponse);
        } else if (data.type === 'input_state') {
          setInputState(data as InputStateResponse);
        } else if (data.type === 'npu_state') {
          setNpuState(data as NpuStateResponse);
        } else if (data.type === 'lpu_state') {
          setLpuState(data as LpuStateResponse);
        } else if (data.base_addr !== undefined && Array.isArray(data.instructions)) {
          setDisassembly(data as DisassemblyResponse);
        } else if (Array.isArray(data.records) && data.total !== undefined) {
          setHistory(data as HistoryResponse);
        } else if (data.type === 'breakpoint_list' && Array.isArray(data.breakpoints)) {
          setBreakpoints(data.breakpoints as Breakpoint[]);
        } else if (data.type === 'breakpoint_added' || data.type === 'breakpoint_removed') {
          send('bp_list');
        }
      } catch {
        // Not JSON or not a memory response
      }
    }
  }, [lastMessage, send]);

  const registerMemoryHandler = useCallback((handler: (data: MemoryReadResponse) => void) => {
    memoryHandlersRef.current.push(handler);
  }, []);

  const registerFramebufferHandler = useCallback((handler: (data: FramebufferResponse) => void) => {
    framebufferHandlersRef.current.push(handler);
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

  useEffect(() => {
    if (activeTab !== 'coprocessor') {
      return;
    }

    send('npu state');
    send('lpu state');

    const timer = window.setInterval(() => {
      send('npu state');
      send('lpu state');
    }, 1000);

    return () => {
      window.clearInterval(timer);
    };
  }, [activeTab, send]);

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
                <button
                  className={activeTab === 'framebuffer' ? 'active' : ''}
                  onClick={() => setActiveTab('framebuffer')}
                >
                  Framebuffer
                </button>
                <button
                  className={activeTab === 'coprocessor' ? 'active' : ''}
                  onClick={() => setActiveTab('coprocessor')}
                >
                  Coprocessor
                </button>
                <button
                  className={activeTab === 'debug' ? 'active' : ''}
                  onClick={() => {
                    setActiveTab('debug');
                    send('bp_list');
                    send(`disasm 0x${snapshot.pc.toString(16)} 24`);
                    send('history 0 30');
                  }}
                >
                  Debug
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

              {activeTab === 'framebuffer' && (
                <>
                  <GameFlowPanel
                    sendCommand={send}
                    gameState={gameState}
                    inputState={inputState}
                  />
                  <FramebufferView
                    sendCommand={send}
                    onFramebufferData={registerFramebufferHandler}
                    perf={snapshot.perf}
                    gameState={gameState}
                    inputState={inputState}
                  />
                  <InputPanel sendCommand={send} />
                </>
              )}

              {activeTab === 'debug' && (
                <DebugInspector
                  currentPc={snapshot.pc}
                  breakpoints={breakpoints}
                  disassembly={disassembly}
                  history={history}
                  sendCommand={send}
                />
              )}

              {activeTab === 'coprocessor' && (
                <CoprocessorPanel
                  sendCommand={send}
                  npuState={npuState}
                  lpuState={lpuState}
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
