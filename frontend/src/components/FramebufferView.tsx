import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type {
    FramebufferGameResponse,
    FramebufferResponse,
    InputStateResponse,
    PerfSnapshot,
} from '../types/snapshot';

interface FramebufferViewProps {
  sendCommand: (command: string) => void;
  onFramebufferData: (handler: (data: FramebufferResponse) => void) => void;
  perf: PerfSnapshot | null;
  gameState: FramebufferGameResponse | null;
  inputState: InputStateResponse | null;
  mode: 'gameflow' | 'cpu';
}

type PixelFormat = 'gray8' | 'rgb565' | 'rgb888';
type DemoPattern = 'pong' | 'checker' | 'gradient';

const LINUX_FB_ADDR = '0x80E00000';
const LINUX_FB_WIDTH = 320;
const LINUX_FB_HEIGHT = 240;
const LINUX_FB_FORMAT: PixelFormat = 'rgb565';

export const FramebufferView: React.FC<FramebufferViewProps> = ({
  sendCommand,
  onFramebufferData,
  perf,
  gameState,
  inputState,
  mode,
}) => {
  const [addrInput, setAddrInput] = useState(LINUX_FB_ADDR);
  const [width, setWidth] = useState(LINUX_FB_WIDTH);
  const [height, setHeight] = useState(LINUX_FB_HEIGHT);
  const [format, setFormat] = useState<PixelFormat>(LINUX_FB_FORMAT);
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [demoPattern, setDemoPattern] = useState<DemoPattern>('pong');
  const [cpuStepCount, setCpuStepCount] = useState(500);
  const [lastFrame, setLastFrame] = useState<FramebufferResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fps, setFps] = useState(0);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const frameTimeRef = useRef<number | null>(null);

  const refresh = useCallback(() => {
    sendCommand(`framebuffer ${addrInput} ${width} ${height} ${format}`);
  }, [addrInput, width, height, format, sendCommand]);

  const applyLinuxPreset = useCallback(() => {
    setAddrInput(LINUX_FB_ADDR);
    setWidth(LINUX_FB_WIDTH);
    setHeight(LINUX_FB_HEIGHT);
    setFormat(LINUX_FB_FORMAT);
    sendCommand('fb linux');
  }, [sendCommand]);

  const runDemoFrame = useCallback(() => {
    sendCommand(`fb_demo ${demoPattern}`);
    sendCommand('fb linux');
  }, [demoPattern, sendCommand]);

  const initCpuDemo = useCallback(() => {
    sendCommand('pause');
    sendCommand('reset');
    sendCommand('fb linux');
  }, [sendCommand]);

  const stepCpuDemo = useCallback(() => {
    const steps = Math.max(1, cpuStepCount);
    sendCommand(`stepn ${steps}`);
    sendCommand('fb linux');
  }, [cpuStepCount, sendCommand]);

  const runCpuDemo = useCallback(() => {
    sendCommand('run');
  }, [sendCommand]);

  const pauseCpuDemo = useCallback(() => {
    sendCommand('pause');
    sendCommand('fb linux');
  }, [sendCommand]);

  useEffect(() => {
    onFramebufferData((data: FramebufferResponse) => {
      if (data.success) {
        const now = performance.now();
        if (frameTimeRef.current !== null) {
          const delta = now - frameTimeRef.current;
          if (delta > 0) {
            setFps(1000 / delta);
          }
        }
        frameTimeRef.current = now;
        setLastFrame(data);
        setError(null);
      } else {
        setError(data.error ?? 'framebuffer read failed');
      }
    });
  }, [onFramebufferData]);

  useEffect(() => {
    sendCommand('fb linux');
  }, [sendCommand]);

  useEffect(() => {
    if (!lastFrame || !lastFrame.success || !canvasRef.current) {
      return;
    }

    const canvas = canvasRef.current;
    canvas.width = lastFrame.width;
    canvas.height = lastFrame.height;

    const context = canvas.getContext('2d');
    if (!context) {
      return;
    }

    const imageData = new ImageData(
      new Uint8ClampedArray(lastFrame.pixels),
      lastFrame.width,
      lastFrame.height,
    );
    context.putImageData(imageData, 0, 0);
  }, [lastFrame]);

  useEffect(() => {
    if (!autoRefresh) {
      return;
    }

    const timer = window.setInterval(() => {
      refresh();
    }, 250);

    return () => {
      window.clearInterval(timer);
    };
  }, [autoRefresh, refresh]);

  const frameInfo = useMemo(() => {
    if (error) {
      return `读取失败：${error}`;
    }
    if (!lastFrame) {
      return mode === 'cpu' ? 'CPU 模式：尚未读取帧缓冲（可先 Reset/Step）' : '尚未读取帧缓冲';
    }
    return `addr=${lastFrame.addr.toString(16)} format=${lastFrame.format} ${lastFrame.width}x${lastFrame.height}`;
  }, [error, lastFrame, mode]);

  const overlayText = useMemo(() => {
    const ipc = perf ? perf.ipc.toFixed(3) : 'N/A';
    const stalls = perf ? perf.stalls.toLocaleString() : 'N/A';
    const tick = gameState?.tick ?? 0;
    const scoreLeft = gameState?.score_left ?? 0;
    const scoreRight = gameState?.score_right ?? 0;
    const inputBits = inputState?.key_state !== undefined
      ? `0x${inputState.key_state.toString(16).toUpperCase()}`
      : 'N/A';
    return {
      fps: fps > 0 ? fps.toFixed(1) : '0.0',
      ipc,
      stalls,
      tick,
      score: `${scoreLeft}:${scoreRight}`,
      inputBits,
    };
  }, [fps, gameState, inputState, perf]);

  return (
    <div className="framebuffer-view">
      <div className="framebuffer-header">
        <h3>Framebuffer View</h3>
        <div className="framebuffer-controls">
          <input
            className="addr-input"
            value={addrInput}
            onChange={(event) => setAddrInput(event.target.value)}
            placeholder={LINUX_FB_ADDR}
          />
          <input
            type="number"
            min={1}
            max={2048}
            value={width}
            onChange={(event) => setWidth(Number(event.target.value) || 1)}
            title="width"
          />
          <input
            type="number"
            min={1}
            max={2048}
            value={height}
            onChange={(event) => setHeight(Number(event.target.value) || 1)}
            title="height"
          />
          <select
            value={format}
            onChange={(event) => setFormat(event.target.value as PixelFormat)}
            title="pixel format"
            aria-label="pixel format"
          >
            <option value="rgb565">rgb565</option>
            <option value="rgb888">rgb888</option>
            <option value="gray8">gray8</option>
          </select>
          <button onClick={refresh}>Refresh</button>
          <button onClick={applyLinuxPreset}>Linux Preset</button>
          {mode === 'gameflow' ? (
            <>
              <select
                value={demoPattern}
                onChange={(event) => setDemoPattern(event.target.value as DemoPattern)}
                title="demo pattern"
              >
                <option value="pong">pong</option>
                <option value="checker">checker</option>
                <option value="gradient">gradient</option>
              </select>
              <button onClick={runDemoFrame}>Demo Frame</button>
            </>
          ) : (
            <>
              <button onClick={initCpuDemo}>CPU Reset</button>
              <input
                type="number"
                min={1}
                max={5000000}
                value={cpuStepCount}
                onChange={(event) => setCpuStepCount(Number(event.target.value) || 1)}
                title="cpu step count"
              />
              <button onClick={stepCpuDemo}>CPU StepN</button>
              <button onClick={runCpuDemo}>CPU Run</button>
              <button onClick={pauseCpuDemo}>CPU Pause</button>
            </>
          )}
          <label className="framebuffer-autorefresh">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(event) => setAutoRefresh(event.target.checked)}
            />
            Auto
          </label>
        </div>
      </div>

      <div className="framebuffer-meta">{frameInfo}</div>

      <div className="framebuffer-canvas-wrapper">
        <canvas ref={canvasRef} className="framebuffer-canvas" />
        <div className="framebuffer-overlay">
          <div className="overlay-item">FPS {overlayText.fps}</div>
          <div className="overlay-item">IPC {overlayText.ipc}</div>
          <div className="overlay-item">Stalls {overlayText.stalls}</div>
          <div className="overlay-item">Tick {overlayText.tick}</div>
          <div className="overlay-item">Score {overlayText.score}</div>
          <div className="overlay-item">Input {overlayText.inputBits}</div>
        </div>
      </div>
    </div>
  );
};
