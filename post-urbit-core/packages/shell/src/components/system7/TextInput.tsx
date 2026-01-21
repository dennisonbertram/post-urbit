import React from "react";

type TextInputProps = {
  placeholder?: string;
  value?: string;
};

const TextInput = ({ placeholder, value }: TextInputProps) => {
  return (
    <input
      className="s7-text-input"
      placeholder={placeholder}
      value={value}
      readOnly
    />
  );
};

export default TextInput;
