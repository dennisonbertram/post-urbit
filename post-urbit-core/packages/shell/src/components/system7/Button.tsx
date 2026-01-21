import React, { forwardRef, useState } from "react";

type ButtonProps = {
  label?: string;
  children?: React.ReactNode;
  variant?: "standard" | "default";
  isDefault?: boolean; // Alias for variant="default"
  pressed?: boolean;
  disabled?: boolean;
  onClick?: (e: React.MouseEvent<HTMLButtonElement>) => void;
};

// Usage:
// <Button label="OK" variant="default" onClick={handleOk} />
// <Button label="Cancel" onClick={handleCancel} disabled={isLoading} />
// <Button>Children Text</Button>
const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      label,
      children,
      variant = "standard",
      isDefault = false,
      pressed = false,
      disabled = false,
      onClick
    },
    ref
  ) => {
    const [isHovered, setIsHovered] = useState(false);
    const [isActive, setIsActive] = useState(false);

    // Support both 'variant="default"' and 'isDefault' prop
    const isDefaultButton = variant === "default" || isDefault;

    const classes = [
      "s7-button",
      isDefaultButton ? "is-default" : "",
      pressed ? "is-pressed" : "",
      disabled ? "is-disabled" : "",
      isHovered && !disabled ? "is-hovered" : "",
      isActive && !disabled ? "is-active" : ""
    ]
      .filter(Boolean)
      .join(" ");

    return (
      <button
        ref={ref}
        className={classes}
        disabled={disabled}
        onClick={onClick}
        onMouseEnter={() => setIsHovered(true)}
        onMouseLeave={() => {
          setIsHovered(false);
          setIsActive(false);
        }}
        onMouseDown={() => setIsActive(true)}
        onMouseUp={() => setIsActive(false)}
        style={{
          cursor: disabled ? "default" : "pointer",
        }}
      >
        {children || label}
      </button>
    );
  }
);

Button.displayName = "Button";

export default Button;
