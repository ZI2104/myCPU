import { useState, useCallback, useEffect } from 'react';
import type { MemoryReadResponse } from '../types/snapshot';
import { formatHexRaw } from '../utils/format';

interface MemoryViewProps {
  sendCommand: (command: string) => void;
  onMemoryData: (handler: (data: MemoryReadResponse) => void) => void;
}

export const MemoryView: React.FC<MemoryViewProps> = ({ sendCommand, onMemoryData }) => {
  const [baseAddr, setBaseAddr] = useState(0x80000000);
  const [bytesPerRow, setBytesPerRow] = useState(16);
  const [memoryData, setMemoryData] = useState<Map<number, Uint8Array>>(new Map());
  const [inputValue, setInputValue] = useState('0x80000000');

  // Register handler for memory data responses
  useEffect(() => {
    onMemoryData((data: MemoryReadResponse) => {
      if (data.success && data.data) {
        setMemoryData(prev => {
          const newMap = new Map(prev);
          newMap.set(data.addr, new Uint8Array(data.data));
          return newMap;
        });
      }
    });
  }, [onMemoryData]);

  const refreshMemory = useCallback(() => {
    const size = bytesPerRow * 8; // 8 rows
    sendCommand(`memory 0x${baseAddr.toString(16)} ${size}`);
  }, [baseAddr, bytesPerRow, sendCommand]);

  const handleJump = useCallback(() => {
    const addr = parseInt(inputValue, 16);
    if (!isNaN(addr)) {
      setBaseAddr(addr);
      // Clear cached memory data when jumping
      setMemoryData(new Map());
    }
  }, [inputValue]);

  const formatAscii = (bytes: Uint8Array): string => {
    return Array.from(bytes)
      .map(b => (b >= 32 && b <= 126) ? String.fromCharCode(b) : '.')
      .join('');
  };

  // Get byte at address
  const getByte = (addr: number): number => {
    for (const [blockAddr, blockData] of memoryData) {
      const offset = addr - blockAddr;
      if (offset >= 0 && offset < blockData.length) {
        return blockData[offset];
      }
    }
    return 0;
  };

  // Render memory rows
  const renderMemoryRows = () => {
    const rows = [];
    for (let row = 0; row < 8; row++) {
      const rowAddr = baseAddr + row * bytesPerRow;
      const rowData: number[] = [];

      for (let col = 0; col < bytesPerRow; col++) {
        rowData.push(getByte(rowAddr + col));
      }

      rows.push(
        <div key={row} className="memory-row">
          <span className="memory-addr">0x{formatHexRaw(rowAddr, 8)}</span>
          <span className="memory-hex">
            {rowData.map((byte, col) => (
              <span key={col} className="byte">
                {formatHexRaw(byte, 2)}
              </span>
            ))}
          </span>
          <span className="memory-ascii">{formatAscii(new Uint8Array(rowData))}</span>
        </div>
      );
    }
    return rows;
  };

  return (
    <div className="memory-view">
      <div className="memory-header">
        <h3>Memory View</h3>
        <div className="memory-controls">
          <input
            type="text"
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleJump()}
            placeholder="0x80000000"
            className="addr-input"
          />
          <button onClick={handleJump}>Jump</button>
          <button onClick={refreshMemory}>Refresh</button>
          <select
            value={bytesPerRow}
            onChange={(e) => setBytesPerRow(Number(e.target.value))}
          >
            <option value={4}>4 bytes/row</option>
            <option value={8}>8 bytes/row</option>
            <option value={16}>16 bytes/row</option>
          </select>
        </div>
      </div>
      <div className="memory-content">
        <div className="memory-row header-row">
          <span className="memory-addr">Address</span>
          <span className="memory-hex">
            {Array.from({ length: bytesPerRow }, (_, i) => (
              <span key={i} className="byte">{formatHexRaw(i, 2)}</span>
            ))}
          </span>
          <span className="memory-ascii">ASCII</span>
        </div>
        {renderMemoryRows()}
      </div>
    </div>
  );
};
