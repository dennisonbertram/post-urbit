import React from "react";

type TextInputProps = React.InputHTMLAttributes<HTMLInputElement> & {
  placeholder?: string;
  value?: string;
};

const TextInput = ({ placeholder, value, onChange, type = "text", disabled, autoFocus, ...rest }: TextInputProps) => {
  return (
    <input
      className="s7-text-input"
      placeholder={placeholder}
      value={value}
      onChange={onChange}
      type={type}
      disabled={disabled}
      autoFocus={autoFocus}
      {...rest}
    />
  );
};

export default TextInput;
