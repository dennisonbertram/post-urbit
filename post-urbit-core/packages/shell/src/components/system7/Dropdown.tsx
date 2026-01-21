import React, { useState, useRef, useEffect } from "react";

type DropdownProps = {
  label?: string;
  options: string[];
  value?: string;
  onChange?: (value: string) => void;
  disabled?: boolean;
};

// Usage:
// <Dropdown label="Choose" options={["A", "B", "C"]} value={selected} onChange={setSelected} />
const Dropdown = ({ label, options, value, onChange, disabled = false }: DropdownProps) => {
  const [isOpen, setIsOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const displayValue = value || label || (options.length > 0 ? options[0] : "Select");

  // Close dropdown when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };

    if (isOpen) {
      document.addEventListener("mousedown", handleClickOutside);
      return () => document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [isOpen]);

  // Keyboard navigation
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((prev) => Math.min(prev + 1, options.length - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((prev) => Math.max(prev - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          handleSelect(options[selectedIndex]);
          break;
        case "Escape":
          e.preventDefault();
          setIsOpen(false);
          break;
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, selectedIndex, options]);

  const handleToggle = () => {
    if (disabled) return;
    setIsOpen(!isOpen);
  };

  const handleSelect = (option: string) => {
    onChange?.(option);
    setIsOpen(false);
  };

  const disabledClass = disabled ? "is-disabled" : "";

  return (
    <div
      className={`s7-dropdown ${isOpen ? "is-open" : ""} ${disabledClass}`}
      ref={dropdownRef}
      style={{ position: "relative", cursor: disabled ? "default" : "pointer" }}
    >
      <div className="s7-dropdown-selected" onClick={handleToggle}>
        {displayValue}
      </div>
      <div className="s7-dropdown-caret" onClick={handleToggle}>
        {isOpen ? "^" : "v"}
      </div>
      {isOpen && (
        <div
          className="s7-dropdown-menu"
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            right: 0,
            zIndex: 1000,
          }}
        >
          {options.map((option, index) => (
            <div
              className={`s7-dropdown-item ${index === selectedIndex ? "is-highlighted" : ""}`}
              key={option}
              onClick={() => handleSelect(option)}
              onMouseEnter={() => setSelectedIndex(index)}
            >
              {option}
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

export default Dropdown;
