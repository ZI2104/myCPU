import { useMemo, useState } from 'react';
import type {
    Breakpoint,
    DisassemblyResponse,
    HistoryResponse,
} from '../types/snapshot';

interface DebugInspectorProps {
  currentPc: number;
  breakpoints: Breakpoint[];
  disassembly: DisassemblyResponse | null;
  history: HistoryResponse | null;
  sendCommand: (command: string) => void;
}

const fmtHex = (value: number, width: number = 8) =>
  `0x${(value >>> 0).toString(16).toUpperCase().padStart(width, '0')}`;

export const DebugInspector: React.FC<DebugInspectorProps> = ({
  currentPc,
  breakpoints,
  disassembly,
  history,
  sendCommand,
}) => {
  const [disasmAddr, setDisasmAddr] = useState(() => fmtHex(currentPc));
  const [disasmCount, setDisasmCount] = useState(20);
  const [historyStart, setHistoryStart] = useState(0);
  const [historyCount, setHistoryCount] = useState(30);
  const [bpAddr, setBpAddr] = useState(() => fmtHex(currentPc));
  const [bpLabel, setBpLabel] = useState('');
  const [expandedPanels, setExpandedPanels] = useState({
    breakpoints: false,
    disassembly: false,
    history: false,
  });

  const sortedBreakpoints = useMemo(
    () => [...breakpoints].sort((a, b) => a.addr - b.addr),
    [breakpoints],
  );

  const requestDisasm = () => {
    sendCommand(`disasm ${disasmAddr} ${disasmCount}`);
  };

  const requestHistory = () => {
    sendCommand(`history ${historyStart} ${historyCount}`);
  };

  const refreshBreakpoints = () => {
    sendCommand('bp_list');
  };

  const addBreakpoint = () => {
    const label = bpLabel.trim();
    if (label.length > 0) {
      sendCommand(`bp_add ${bpAddr} ${label.replace(/\s+/g, '_')}`);
    } else {
      sendCommand(`bp_add ${bpAddr}`);
    }
  };

  const removeBreakpoint = (addr: number) => {
    sendCommand(`bp_remove ${fmtHex(addr)}`);
  };

  const togglePanel = (panel: 'breakpoints' | 'disassembly' | 'history') => {
    setExpandedPanels((prev) => ({ ...prev, [panel]: !prev[panel] }));
  };

  const previewDisassembly = useMemo(() => {
    const instructions = disassembly?.instructions ?? [];
    if (instructions.length === 0) return null;
    return instructions.find((item) => item.addr === currentPc) ?? instructions[0];
  }, [disassembly, currentPc]);

  const previewHistory = useMemo(() => {
    const records = history?.records ?? [];
    if (records.length === 0) return null;
    return records[0];
  }, [history]);

  return (
    <div className="debug-inspector">
      <h3>Debug Inspector</h3>

      <section className="debug-section">
        <div className="debug-section-header">
          <div className="debug-section-title">Breakpoints</div>
          <button
            type="button"
            className="debug-toggle-btn"
            aria-expanded={expandedPanels.breakpoints}
            onClick={() => togglePanel('breakpoints')}
          >
            {expandedPanels.breakpoints ? 'Collapse' : 'Expand'}
          </button>
        </div>
        <div className="debug-controls-inline">
          <input value={bpAddr} onChange={(e) => setBpAddr(e.target.value)} />
          <input
            value={bpLabel}
            onChange={(e) => setBpLabel(e.target.value)}
            placeholder="label(optional)"
          />
          <button onClick={addBreakpoint}>Add</button>
          <button onClick={refreshBreakpoints}>Refresh</button>
        </div>
        <div className={`debug-list ${expandedPanels.breakpoints ? '' : 'preview'}`.trim()}>
          {sortedBreakpoints.length === 0 && <div className="debug-empty">No breakpoints</div>}
          {(expandedPanels.breakpoints ? sortedBreakpoints : sortedBreakpoints.slice(0, 1)).map((bp) => (
            <div key={bp.addr} className="debug-list-item breakpoint-item">
              <span>{fmtHex(bp.addr)} ({bp.hit_count} hits)</span>
              <span className="debug-label">{bp.label ?? '-'}</span>
              {expandedPanels.breakpoints ? (
                <button onClick={() => removeBreakpoint(bp.addr)}>Remove</button>
              ) : (
                <span className="debug-preview-hint">Preview</span>
              )}
            </div>
          ))}
        </div>
      </section>

      <section className="debug-section">
        <div className="debug-section-header">
          <div className="debug-section-title">Disassembly</div>
          <button
            type="button"
            className="debug-toggle-btn"
            aria-expanded={expandedPanels.disassembly}
            onClick={() => togglePanel('disassembly')}
          >
            {expandedPanels.disassembly ? 'Collapse' : 'Expand'}
          </button>
        </div>
        <div className="debug-controls-inline">
          <input value={disasmAddr} onChange={(e) => setDisasmAddr(e.target.value)} />
          <input
            type="number"
            min={1}
            max={200}
            value={disasmCount}
            onChange={(e) => setDisasmCount(Number(e.target.value) || 20)}
          />
          <button onClick={requestDisasm}>Load</button>
        </div>
        <div className={`debug-list mono ${expandedPanels.disassembly ? '' : 'preview'}`.trim()}>
          {expandedPanels.disassembly ? (
            disassembly?.instructions?.length ? (
              disassembly.instructions.map((item) => (
                <div
                  key={item.addr}
                  className={`debug-list-item disasm-item ${item.addr === currentPc ? 'current-pc' : ''}`}
                >
                  <span>{fmtHex(item.addr)}</span>
                  <span className="debug-label">{item.instruction}</span>
                  <span>{item.has_breakpoint ? '●' : ''}</span>
                </div>
              ))
            ) : (
              <div className="debug-empty">No disassembly data</div>
            )
          ) : previewDisassembly ? (
            <div
              key={previewDisassembly.addr}
              className={`debug-list-item disasm-item ${previewDisassembly.addr === currentPc ? 'current-pc' : ''}`}
            >
              <span>{fmtHex(previewDisassembly.addr)}</span>
              <span className="debug-label">{previewDisassembly.instruction}</span>
              <span>{previewDisassembly.has_breakpoint ? '●' : ''}</span>
            </div>
          ) : (
            <div className="debug-empty">No disassembly data</div>
          )}
        </div>
      </section>

      <section className="debug-section">
        <div className="debug-section-header">
          <div className="debug-section-title">Execution History</div>
          <button
            type="button"
            className="debug-toggle-btn"
            aria-expanded={expandedPanels.history}
            onClick={() => togglePanel('history')}
          >
            {expandedPanels.history ? 'Collapse' : 'Expand'}
          </button>
        </div>
        <div className="debug-controls-inline">
          <input
            type="number"
            min={0}
            value={historyStart}
            onChange={(e) => setHistoryStart(Number(e.target.value) || 0)}
          />
          <input
            type="number"
            min={1}
            max={500}
            value={historyCount}
            onChange={(e) => setHistoryCount(Number(e.target.value) || 30)}
          />
          <button onClick={requestHistory}>Load</button>
        </div>
        <div className={`debug-list mono ${expandedPanels.history ? '' : 'preview'}`.trim()}>
          {expandedPanels.history ? (
            history?.records?.length ? (
              history.records.map((record) => (
                <div key={`${record.cycle}-${record.pc}`} className="debug-list-item history-item">
                  <span>c{record.cycle}</span>
                  <span>{fmtHex(record.pc)}</span>
                  <span className="debug-label">{record.instruction_str ?? 'N/A'}</span>
                </div>
              ))
            ) : (
              <div className="debug-empty">No history data</div>
            )
          ) : previewHistory ? (
            <div key={`${previewHistory.cycle}-${previewHistory.pc}`} className="debug-list-item history-item">
              <span>c{previewHistory.cycle}</span>
              <span>{fmtHex(previewHistory.pc)}</span>
              <span className="debug-label">{previewHistory.instruction_str ?? 'N/A'}</span>
            </div>
          ) : (
            <div className="debug-empty">No history data</div>
          )}
        </div>
      </section>
    </div>
  );
};
