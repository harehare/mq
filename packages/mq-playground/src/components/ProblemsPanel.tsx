import { VscError, VscWarning, VscInfo } from "react-icons/vsc";
import "./ProblemsPanel.css";

export type DiagnosticItem = {
  startLineNumber: number;
  startColumn: number;
  endLineNumber: number;
  endColumn: number;
  message: string;
  severity: number;
};

// Monaco MarkerSeverity values
const SEVERITY_ERROR = 8;
const SEVERITY_WARNING = 4;

type ProblemsPanelProps = {
  problems: DiagnosticItem[];
  height: number;
  onProblemClick: (lineNumber: number, column: number) => void;
};

export const ProblemsPanel = ({ problems, height, onProblemClick }: ProblemsPanelProps) => {
  return (
    <div className="problems-panel" style={{ height }}>
      <div className="problems-header">
        <span className="problems-title">PROBLEMS</span>
        <span className="problems-count">
          {problems.length} {problems.length === 1 ? "problem" : "problems"}
        </span>
      </div>
      <div className="problems-list">
        {problems.length === 0 ? (
          <div className="problems-empty">No problems detected</div>
        ) : (
          problems.map((problem, i) => (
            <div
              key={i}
              className="problem-item"
              onClick={() => onProblemClick(problem.startLineNumber, problem.startColumn)}
            >
              <span className="problem-icon">
                {problem.severity >= SEVERITY_ERROR ? (
                  <VscError size={13} className="problem-icon-error" />
                ) : problem.severity >= SEVERITY_WARNING ? (
                  <VscWarning size={13} className="problem-icon-warning" />
                ) : (
                  <VscInfo size={13} className="problem-icon-info" />
                )}
              </span>
              <span className="problem-message">{problem.message}</span>
              <span className="problem-location">
                Ln {problem.startLineNumber}, Col {problem.startColumn}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
