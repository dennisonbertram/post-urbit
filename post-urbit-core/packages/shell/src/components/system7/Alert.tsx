import React from "react";
import Button from "./Button";

type AlertProps = {
  type: "stop" | "caution" | "note";
  title: string;
  message: string;
};

const Alert = ({ type, title, message }: AlertProps) => {
  return (
    <div className={`s7-alert s7-alert-${type}`} role="alertdialog" aria-modal>
      <div className="s7-alert-icon" aria-hidden="true" />
      <div className="s7-alert-body">
        <div className="s7-alert-title">{title}</div>
        <div className="s7-alert-message">{message}</div>
      </div>
      <div className="s7-alert-actions">
        <Button label="Cancel" />
        <Button label="OK" variant="default" />
      </div>
    </div>
  );
};

export default Alert;
