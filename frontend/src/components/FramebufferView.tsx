import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { FramebufferResponse } from '../types/snapshot';

interface FramebufferViewProps {
  sendCommand: (command: string) => void;
  onFramebufferData: (handler: (data: FramebufferResponse) => void) => void;
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
}) => {
  const [addrInput, setAddrInput] = useState(LINUX_FB_ADDR);
  const [width, setWidth] = useState(LINUX_FB_WIDTH);
  const [height, setHeight] = useState(LINUX_FB_HEIGHT);
  const [format, setFormat] = useState<PixelFormat>(LINUX_FB_FORMAT);
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [demoPattern, setDemoPattern] = useState<DemoPattern>('pong');
  const [lastFrame, setLastFrame] = useState<FramebufferResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

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

  useEffect(() => {
    onFramebufferData((data: FramebufferResponse) => {
      if (data.success) {
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
    if (!lastFrame) {
      return '尚未读取帧缓冲';
    }
    return `addr=${lastFrame.addr.toString(16)} format=${lastFrame.format} ${lastFrame.width}x${lastFrame.height}`;
  }, [lastFrame]);

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
          >
            <option value="rgb565">rgb565</option>
            <option value="rgb888">rgb888</option>
            <option value="gray8">gray8</option>
          </select>
          <button onClick={refresh}>Refresh</button>
          <button onClick={applyLinuxPreset}>Linux Preset</button>
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

      {error && <div className="error-message">{error}</div>}

      <div className="framebuffer-meta">{frameInfo}</div>

      <div className="framebuffer-canvas-wrapper">
        <canvas ref={canvasRef} className="framebuffer-canvas" />
      </div>
    </div>
  );
};
