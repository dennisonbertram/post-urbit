import React, { useState } from "react";

type CheckboxProps = {
  label: string;
  checked?: boolean;
  mixed?: boolean;
  onChange?: (checked: boolean) => void;
  disabled?: boolean;
};

// Usage:
// Controlled: <Checkbox label="Option" checked={isChecked} onChange={setIsChecked} />
// Uncontrolled: <Checkbox label="Option" />
const Checkbox = ({ label, checked, mixed = false, onChange, disabled = false }: CheckboxProps) => {
  const [internalChecked, setInternalChecked] = useState(false);

  // Use controlled value if provided, otherwise use internal state
  const isControlled = checked !== undefined;
  const isChecked = isControlled ? checked : internalChecked;

  const handleClick = () => {
    if (disabled) return;

    const newChecked = !isChecked;

    if (!isControlled) {
      setInternalChecked(newChecked);
    }

    onChange?.(newChecked);
  };

  const stateClass = mixed ? "is-mixed" : isChecked ? "is-checked" : "";
  const disabledClass = disabled ? "is-disabled" : "";

  return (
    <label
      className={`s7-checkbox ${stateClass} ${disabledClass}`}
      onClick={handleClick}
      style={{ cursor: disabled ? "default" : "pointer" }}
    >
      <span className="s7-checkbox-box" aria-hidden="true" />
      <span className="s7-control-label">{label}</span>
    </label>
  );
};

export default Checkbox;
