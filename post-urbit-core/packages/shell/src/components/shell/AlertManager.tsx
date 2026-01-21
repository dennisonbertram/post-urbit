import React, { useEffect } from "react";
import { useAlert } from "../../context/AlertContext";
import Alert from "../system7/Alert";

/**
 * AlertManager - Renders modal alerts from the AlertContext
 *
 * Features:
 * - Modal overlay that blocks interaction with content behind
 * - Centered alert dialog
 * - Keyboard handling (Enter/Escape)
 * - Automatic focus management
 */
const AlertManager = () => {
  const { currentAlert, dismissAlert } = useAlert();

  // Prevent body scroll when alert is shown
  useEffect(() => {
    if (currentAlert) {
      document.body.style.overflow = "hidden";
      return () => {
        document.body.style.overflow = "";
      };
    }
  }, [currentAlert]);

  if (!currentAlert) {
    return null;
  }

  const handleDismiss = () => {
    dismissAlert(currentAlert.id);
  };

  const handleOverlayClick = (e: React.MouseEvent) => {
    // Only dismiss if clicking directly on overlay, not its children
    if (e.target === e.currentTarget) {
      handleDismiss();
    }
  };

  return (
    <div className="s7-modal-overlay" onClick={handleOverlayClick}>
      <div className="s7-modal-content">
        <Alert
          type={currentAlert.type}
          title={currentAlert.title}
          message={currentAlert.message}
          buttons={currentAlert.buttons}
          onDismiss={handleDismiss}
        />
      </div>
    </div>
  );
};

export default AlertManager;
