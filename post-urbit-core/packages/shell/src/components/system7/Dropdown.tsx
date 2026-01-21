import React from "react";

type DropdownProps = {
  label: string;
  options: string[];
};

const Dropdown = ({ label, options }: DropdownProps) => {
  return (
    <div className="s7-dropdown">
      <div className="s7-dropdown-selected">{label}</div>
      <div className="s7-dropdown-caret">v</div>
      <div className="s7-dropdown-menu">
        {options.map((option) => (
          <div className="s7-dropdown-item" key={option}>
            {option}
          </div>
        ))}
      </div>
    </div>
  );
};

export default Dropdown;
