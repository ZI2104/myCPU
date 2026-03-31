import { useState, useEffect, useRef, useCallback } from 'react';
import type { CpuSnapshot } from '../types/snapshot';

interface UseWebSocketReturn {
  snapshot: CpuSnapshot | null;
  connected: boolean;
  send: (command: string) => void;
  error: string | null;
  lastMessage: string | null;
}

export function useWebSocket(url: string): UseWebSocketReturn {
  const [snapshot, setSnapshot] = useState<CpuSnapshot | null>(null);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastMessage, setLastMessage] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    const ws = new WebSocket(url);

    ws.onopen = () => {
      console.log('WebSocket connected');
      setConnected(true);
      setError(null);
    };

    ws.onclose = () => {
      console.log('WebSocket disconnected');
      setConnected(false);
    };

    ws.onerror = (e) => {
      console.error('WebSocket error:', e);
      setError('Connection error');
    };

    ws.onmessage = (event) => {
      const rawData = event.data as string;
      setLastMessage(rawData);

      try {
        const data = JSON.parse(rawData);
        if (data.registers) {
          setSnapshot(data as CpuSnapshot);
        } else if (data.status) {
          console.log('Status:', data.status);
        } else if (data.error) {
          setError(data.error);
        }
      } catch (e) {
        console.error('Parse error:', e);
      }
    };

    wsRef.current = ws;

    return () => {
      ws.close();
    };
  }, [url]);

  const send = useCallback((command: string) => {
    if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
      wsRef.current.send(command);
    }
  }, []);

  return { snapshot, connected, send, error, lastMessage };
}
