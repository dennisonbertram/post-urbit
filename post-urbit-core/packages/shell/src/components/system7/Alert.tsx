import React, { useEffect, useRef } from "react";
import Button from "./Button";

export interface AlertButton {
  label: string;
  onClick: () => void;
  variant?: "standard" | "default";
}

type AlertProps = {
  type: "stop" | "caution" | "note";
  title: string;
  message: string;
  buttons?: AlertButton[];
  onDismiss?: () => void;
};

const Alert = ({ type, title, message, buttons, onDismiss }: AlertProps) => {
  const defaultButtonRef = useRef<HTMLButtonElement>(null);

  // Focus default button on mount
  useEffect(() => {
    if (defaultButtonRef.current) {
      defaultButtonRef.current.focus();
    }
  }, []);

  // Default OK button if no buttons provided
  const alertButtons: AlertButton[] = buttons || [
    {
      label: "OK",
      onClick: () => onDismiss?.(),
      variant: "default",
    },
  ];

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      // Trigger default button
      const defaultButton = alertButtons.find((b) => b.variant === "default");
      if (defaultButton) {
        defaultButton.onClick();
      } else if (alertButtons.length > 0) {
        alertButtons[alertButtons.length - 1].onClick();
      }
    } else if (e.key === "Escape") {
      // Trigger first standard button or dismiss
      const cancelButton = alertButtons.find((b) => b.variant !== "default");
      if (cancelButton) {
        cancelButton.onClick();
      } else {
        onDismiss?.();
      }
    }
  };

  return (
    <div
      className={`s7-alert s7-alert-${type}`}
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="alert-title"
      aria-describedby="alert-message"
      onKeyDown={handleKeyDown}
      tabIndex={-1}
    >
      <div className="s7-alert-icon" aria-hidden="true" />
      <div className="s7-alert-body">
        <div className="s7-alert-title" id="alert-title">
          {title}
        </div>
        <div className="s7-alert-message" id="alert-message">
          {message}
        </div>
      </div>
      <div className="s7-alert-actions">
        {alertButtons.map((button, index) => (
          <Button
            key={index}
            ref={button.variant === "default" ? defaultButtonRef : undefined}
            label={button.label}
            variant={button.variant}
            onClick={button.onClick}
          />
        ))}
      </div>
    </div>
  );
};

export default Alert;
