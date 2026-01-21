import React from "react";

type RadioProps = {
  label: string;
  selected?: boolean;
};

const Radio = ({ label, selected = false }: RadioProps) => {
  return (
    <label className={`s7-radio ${selected ? "is-selected" : ""}`}>
      <span className="s7-radio-ring" aria-hidden="true" />
      <span className="s7-control-label">{label}</span>
    </label>
  );
};

export default Radio;
