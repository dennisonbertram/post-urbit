import React, { useState, useEffect, useRef } from "react";
import Button from "../system7/Button";
import Radio from "../system7/Radio";

type PermissionScope = "once" | "session" | "always";

interface PermissionPromptProps {
  appName: string;
  permission: string;
  description: string;
  onAllow?: (scope: PermissionScope) => void;
  onDeny?: () => void;
}

/**
 * PermissionPrompt - Modal dialog for permission requests
 *
 * Usage:
 * <PermissionPrompt
 *   appName="ExampleApp"
 *   permission="clipboard"
 *   description="This will allow the app to read text and images you copy."
 *   onAllow={(scope) => console.log('Allowed:', scope)}
 *   onDeny={() => console.log('Denied')}
 * />
 */
const PermissionPrompt = ({
  appName,
  permission,
  description,
  onAllow,
  onDeny,
}: PermissionPromptProps) => {
  const [selectedScope, setSelectedScope] = useState<PermissionScope>("once");
  const allowButtonRef = useRef<HTMLButtonElement>(null);

  // Focus allow button on mount
  useEffect(() => {
    if (allowButtonRef.current) {
      allowButtonRef.current.focus();
    }
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      onAllow?.(selectedScope);
    } else if (e.key === "Escape") {
      onDeny?.();
    }
  };

  return (
    <div
      className="s7-permission"
      role="dialog"
      aria-modal="true"
      aria-labelledby="permission-title"
      aria-describedby="permission-desc"
      onKeyDown={handleKeyDown}
      tabIndex={-1}
    >
      <div className="s7-permission-header">
        <div className="s7-permission-icon" aria-hidden="true" />
        <div>
          <div className="s7-permission-title" id="permission-title">
            "{appName}" wants to access your {permission}
          </div>
          <div className="s7-permission-subtitle" id="permission-desc">
            {description}
          </div>
        </div>
      </div>
      <div className="s7-permission-options">
        <Radio
          label="Allow once"
          value="once"
          selected={selectedScope === "once"}
          onChange={(value) => setSelectedScope(value as PermissionScope)}
        />
        <Radio
          label="Allow for this session"
          value="session"
          selected={selectedScope === "session"}
          onChange={(value) => setSelectedScope(value as PermissionScope)}
        />
        <Radio
          label="Always allow"
          value="always"
          selected={selectedScope === "always"}
          onChange={(value) => setSelectedScope(value as PermissionScope)}
        />
      </div>
      <div className="s7-permission-actions">
        <Button label="Deny" onClick={onDeny} />
        <Button
          ref={allowButtonRef}
          label="Allow"
          variant="default"
          onClick={() => onAllow?.(selectedScope)}
        />
      </div>
    </div>
  );
};

export default PermissionPrompt;
