import React, { createContext, useContext, useState, useCallback } from "react";

export type AlertType = "stop" | "caution" | "note";

export interface AlertButton {
  label: string;
  onClick: () => void;
  variant?: "standard" | "default";
}

export interface Alert {
  id: string;
  type: AlertType;
  title: string;
  message: string;
  buttons?: AlertButton[];
  onDismiss?: () => void;
}

interface AlertContextType {
  currentAlert: Alert | null;
  showAlert: (
    type: AlertType,
    title: string,
    message: string,
    buttons?: AlertButton[],
    onDismiss?: () => void
  ) => string;
  dismissAlert: (id: string) => void;
  confirm: (title: string, message: string) => Promise<boolean>;
}

const AlertContext = createContext<AlertContextType | undefined>(undefined);

export const useAlert = () => {
  const context = useContext(AlertContext);
  if (!context) {
    throw new Error("useAlert must be used within an AlertProvider");
  }
  return context;
};

interface AlertProviderProps {
  children: React.ReactNode;
}

export const AlertProvider = ({ children }: AlertProviderProps) => {
  const [alertQueue, setAlertQueue] = useState<Alert[]>([]);
  const currentAlert = alertQueue[0] || null;

  const showAlert = useCallback(
    (
      type: AlertType,
      title: string,
      message: string,
      buttons?: AlertButton[],
      onDismiss?: () => void
    ): string => {
      const id = `alert-${Date.now()}-${Math.random()}`;
      const alert: Alert = {
        id,
        type,
        title,
        message,
        buttons,
        onDismiss,
      };

      setAlertQueue((prev) => [...prev, alert]);
      return id;
    },
    []
  );

  const dismissAlert = useCallback((id: string) => {
    setAlertQueue((prev) => {
      const alert = prev.find((a) => a.id === id);
      if (alert?.onDismiss) {
        alert.onDismiss();
      }
      return prev.filter((a) => a.id !== id);
    });
  }, []);

  const confirm = useCallback(
    (title: string, message: string): Promise<boolean> => {
      return new Promise((resolve) => {
        const buttons: AlertButton[] = [
          {
            label: "Cancel",
            onClick: () => {
              resolve(false);
            },
            variant: "standard",
          },
          {
            label: "OK",
            onClick: () => {
              resolve(true);
            },
            variant: "default",
          },
        ];

        showAlert("note", title, message, buttons);
      });
    },
    [showAlert]
  );

  return (
    <AlertContext.Provider
      value={{ currentAlert, showAlert, dismissAlert, confirm }}
    >
      {children}
    </AlertContext.Provider>
  );
};
