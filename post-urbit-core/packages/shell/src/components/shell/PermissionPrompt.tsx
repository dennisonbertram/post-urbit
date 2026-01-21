import React from "react";
import Button from "../system7/Button";
import Radio from "../system7/Radio";

const PermissionPrompt = () => {
  return (
    <div className="s7-permission" role="dialog" aria-modal>
      <div className="s7-permission-header">
        <div className="s7-permission-icon" aria-hidden="true" />
        <div>
          <div className="s7-permission-title">
            "ExampleApp" wants to access your clipboard
          </div>
          <div className="s7-permission-subtitle">
            This will allow the app to read text and images you copy.
          </div>
        </div>
      </div>
      <div className="s7-permission-options">
        <Radio label="Allow once" selected />
        <Radio label="Allow for this session" />
        <Radio label="Always allow" />
      </div>
      <div className="s7-permission-actions">
        <Button label="Deny" />
        <Button label="Allow" variant="default" />
      </div>
    </div>
  );
};

export default PermissionPrompt;
