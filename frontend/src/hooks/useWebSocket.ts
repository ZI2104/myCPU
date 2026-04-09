import { useCallback, useEffect, useRef, useState } from 'react';
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
  const latestResetSequenceRef = useRef<number>(0);
  const latestCycleRef = useRef<number>(-1);

  useEffect(() => {
    const ws = new WebSocket(url);

    ws.onopen = () => {
      console.log('WebSocket connected');
      setConnected(true);
      setError(null);
      latestResetSequenceRef.current = 0;
      latestCycleRef.current = -1;
    };

    ws.onclose = () => {
      console.log('WebSocket disconnected');
      setConnected(false);
      latestResetSequenceRef.current = 0;
      latestCycleRef.current = -1;
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
          const next = data as CpuSnapshot;
          const nextResetSequence = typeof next.reset_sequence === 'number' ? next.reset_sequence : 0;
          const nextCycle = typeof next.perf?.cycles === 'number' ? next.perf.cycles : -1;

          const currentResetSequence = latestResetSequenceRef.current;
          const currentCycle = latestCycleRef.current;

          // Drop stale snapshots that are older than the latest accepted
          // (important around reset where mixed frames may arrive close together).
          if (nextResetSequence < currentResetSequence) {
            return;
          }
          if (nextResetSequence === currentResetSequence && nextCycle < currentCycle) {
            return;
          }

          latestResetSequenceRef.current = nextResetSequence;
          latestCycleRef.current = nextCycle;
          setSnapshot(next);
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
