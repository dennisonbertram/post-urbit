import React from "react";

type ProgressBarProps = {
  value?: number;
  indeterminate?: boolean;
};

const ProgressBar = ({ value = 40, indeterminate = false }: ProgressBarProps) => {
  return (
    <div className={`s7-progress ${indeterminate ? "is-indeterminate" : ""}`}>
      {!indeterminate && (
        <div className="s7-progress-fill" style={{ width: `${value}%` }} />
      )}
      {indeterminate && <div className="s7-progress-fill" />}
    </div>
  );
};

export default ProgressBar;
