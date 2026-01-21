import React from "react";

type RadioProps = {
  label: string;
  selected?: boolean;
  value: string;
  name?: string;
  onChange?: (value: string) => void;
  disabled?: boolean;
};

// Usage:
// <Radio label="Option 1" value="opt1" selected={selected === "opt1"} onChange={setSelected} />
// <Radio label="Option 2" value="opt2" selected={selected === "opt2"} onChange={setSelected} />
const Radio = ({ label, selected = false, value, name, onChange, disabled = false }: RadioProps) => {
  const handleClick = () => {
    if (disabled || selected) return;
    onChange?.(value);
  };

  const disabledClass = disabled ? "is-disabled" : "";

  return (
    <label
      className={`s7-radio ${selected ? "is-selected" : ""} ${disabledClass}`}
      onClick={handleClick}
      style={{ cursor: disabled ? "default" : "pointer" }}
    >
      <span className="s7-radio-ring" aria-hidden="true" />
      <span className="s7-control-label">{label}</span>
      <input
        type="radio"
        name={name}
        value={value}
        checked={selected}
        onChange={() => onChange?.(value)}
        disabled={disabled}
        style={{ position: "absolute", opacity: 0, pointerEvents: "none" }}
        aria-label={label}
      />
    </label>
  );
};

export default Radio;
