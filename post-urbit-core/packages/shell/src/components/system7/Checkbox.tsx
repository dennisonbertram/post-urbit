import React from "react";

type CheckboxProps = {
  label: string;
  checked?: boolean;
  mixed?: boolean;
};

const Checkbox = ({ label, checked = false, mixed = false }: CheckboxProps) => {
  const stateClass = mixed ? "is-mixed" : checked ? "is-checked" : "";

  return (
    <label className={`s7-checkbox ${stateClass}`}>
      <span className="s7-checkbox-box" aria-hidden="true" />
      <span className="s7-control-label">{label}</span>
    </label>
  );
};

export default Checkbox;
