import React from 'react';
import type { PipelineSnapshot } from '../types/snapshot';

interface PipelineVisualizerProps {
  pipeline: PipelineSnapshot | null;
}

const StageBox: React.FC<{
  name: string;
  active: boolean;
  children?: React.ReactNode;
  highlight?: boolean;
}> = ({ name, active, children, highlight }) => (
  <div className={`pipeline-stage ${active ? 'active' : ''} ${highlight ? 'highlight' : ''}`}>
    <div className="stage-header">{name}</div>
    <div className="stage-content">
      {active ? children : <span className="bubble">bubble</span>}
    </div>
  </div>
);

const formatHex = (value: number, digits: number = 8): string => {
  if (value === undefined || value === null) return '0x00000000';
  return '0x' + (value >>> 0).toString(16).toUpperCase().padStart(digits, '0');
};

export const PipelineVisualizer: React.FC<PipelineVisualizerProps> = ({ pipeline }) => {
  if (!pipeline) {
    return <div className="pipeline-visualizer">No pipeline data</div>;
  }

  return (
    <div className="pipeline-visualizer">
      <div className="pipeline-flow">
        <StageBox name="IF" active={pipeline.if_stage !== null}>
          {pipeline.if_stage && (
            <>
              <div className="pc">{formatHex(pipeline.if_stage.pc)}</div>
              <div className="instruction">{formatHex(pipeline.if_stage.instruction)}</div>
            </>
          )}
        </StageBox>

        <div className="arrow">→</div>

        <StageBox name="ID" active={pipeline.id_stage !== null}>
          {pipeline.id_stage && (
            <>
              <div className="pc">{formatHex(pipeline.id_stage.pc)}</div>
              <div className="regs">
                rs1: x{pipeline.id_stage.rs1} = {formatHex(pipeline.id_stage.rs1_val)}
              </div>
              <div className="regs">
                rs2: x{pipeline.id_stage.rs2} = {formatHex(pipeline.id_stage.rs2_val)}
              </div>
            </>
          )}
        </StageBox>

        <div className="arrow">→</div>

        <StageBox
          name="EX"
          active={pipeline.ex_stage !== null}
          highlight={pipeline.ex_stage?.branch_taken}
        >
          {pipeline.ex_stage && (
            <>
              <div className="pc">{formatHex(pipeline.ex_stage.pc)}</div>
              <div className="alu-result">
                ALU: {formatHex(pipeline.ex_stage.alu_result)}
              </div>
              {pipeline.ex_stage.branch_taken && (
                <div className="branch-info">
                  Branch → {formatHex(pipeline.ex_stage.branch_target)}
                </div>
              )}
            </>
          )}
        </StageBox>

        <div className="arrow">→</div>

        <StageBox name="MEM" active={pipeline.mem_stage !== null}>
          {pipeline.mem_stage && (
            <>
              <div className="pc">{formatHex(pipeline.mem_stage.pc)}</div>
              {pipeline.mem_stage.mem_read && (
                <div className="mem-op">LOAD from {formatHex(pipeline.mem_stage.alu_result)}</div>
              )}
              {pipeline.mem_stage.mem_write && (
                <div className="mem-op">STORE to {formatHex(pipeline.mem_stage.alu_result)}</div>
              )}
              {!pipeline.mem_stage.mem_read && !pipeline.mem_stage.mem_write && (
                <div className="mem-op">No memory access</div>
              )}
            </>
          )}
        </StageBox>

        <div className="arrow">→</div>

        <StageBox name="WB" active={pipeline.wb_stage !== null}>
          {pipeline.wb_stage && (
            <>
              <div className="pc">{formatHex(pipeline.wb_stage.pc)}</div>
              {pipeline.wb_stage.reg_write ? (
                <div className="write-back">
                  x{pipeline.wb_stage.rd} ← {formatHex(pipeline.wb_stage.write_data)}
                </div>
              ) : (
                <div className="no-write">No register write</div>
              )}
            </>
          )}
        </StageBox>
      </div>

      <div className="pipeline-status">
        {pipeline.stall && <span className="status stall">STALL (Load-Use)</span>}
        {pipeline.flush && <span className="status flush">FLUSH</span>}
      </div>
    </div>
  );
};
