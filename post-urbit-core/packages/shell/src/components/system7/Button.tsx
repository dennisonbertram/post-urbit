import React from "react";

type ButtonProps = {
  label: string;
  variant?: "standard" | "default";
  pressed?: boolean;
  disabled?: boolean;
};

const Button = ({
  label,
  variant = "standard",
  pressed = false,
  disabled = false
}: ButtonProps) => {
  const classes = [
    "s7-button",
    variant === "default" ? "is-default" : "",
    pressed ? "is-pressed" : "",
    disabled ? "is-disabled" : ""
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <button className={classes} disabled={disabled}>
      {label}
    </button>
  );
};

export default Button;
