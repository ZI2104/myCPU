import React from 'react';
import type { CsrSnapshot, TrapSnapshot } from '../types/snapshot';
import { formatHex } from '../utils/format';

interface CsrTrapPanelProps {
  csr: CsrSnapshot;
  trap: TrapSnapshot;
}

const bitFlag = (value: number, bit: number): '1' | '0' =>
  ((value >>> bit) & 1) === 1 ? '1' : '0';

export const CsrTrapPanel: React.FC<CsrTrapPanelProps> = ({ csr, trap }) => {
  const mstatusMie = bitFlag(csr.mstatus, 3);
  const mstatusMpie = bitFlag(csr.mstatus, 7);
  const mstatusMpp = (csr.mstatus >>> 11) & 0x3;
  const satpMode = (csr.satp >>> 31) & 0x1;
  const satpAsid = (csr.satp >>> 22) & 0x1ff;
  const satpPpn = csr.satp & 0x003f_ffff;

  return (
    <div className="csr-trap-panel">
      <h3>CSR & Trap</h3>

      <div className="csr-trap-grid">
        <div><span>mstatus</span><strong>{formatHex(csr.mstatus)}</strong></div>
        <div><span>mtvec</span><strong>{formatHex(csr.mtvec)}</strong></div>
        <div><span>mepc</span><strong>{formatHex(csr.mepc)}</strong></div>
        <div><span>mcause</span><strong>{formatHex(csr.mcause)}</strong></div>
        <div><span>mtval</span><strong>{formatHex(csr.mtval)}</strong></div>
        <div><span>satp</span><strong>{formatHex(csr.satp)}</strong></div>
        <div><span>mie</span><strong>{formatHex(csr.mie)}</strong></div>
        <div><span>mip</span><strong>{formatHex(csr.mip)}</strong></div>
      </div>

      <div className="csr-keyfields">
        <div className="csr-keyfield">
          <span>MSTATUS</span>
          <span>MIE={mstatusMie} | MPIE={mstatusMpie} | MPP={mstatusMpp}</span>
        </div>
        <div className="csr-keyfield">
          <span>SATP</span>
          <span>MODE={satpMode === 1 ? 'Sv32' : 'Bare'} | ASID={satpAsid} | PPN=0x{satpPpn.toString(16).toUpperCase()}</span>
        </div>
      </div>

      <div className="trap-summary">
        <div className="trap-summary-title">Latest Trap Summary</div>
        <div className="trap-summary-badges">
          <span className={`trap-badge ${trap.is_interrupt ? 'interrupt' : 'exception'}`}>
            {trap.is_interrupt ? 'Interrupt' : 'Exception'}
          </span>
          <span className="trap-badge mode">Handler: {trap.handler_mode}</span>
          <span className="trap-badge cause">{trap.cause_label} ({trap.cause_code})</span>
        </div>
        <div className="trap-summary-grid">
          <div><span>EPC</span><strong>{formatHex(trap.epc)}</strong></div>
          <div><span>TVAL</span><strong>{formatHex(trap.tval)}</strong></div>
          <div><span>mideleg</span><strong>{formatHex(csr.mideleg)}</strong></div>
          <div><span>medeleg</span><strong>{formatHex(csr.medeleg)}</strong></div>
        </div>
      </div>
    </div>
  );
};
