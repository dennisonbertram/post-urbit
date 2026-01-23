import React, { useEffect } from "react";
import MenuBar from "./components/system7/MenuBar";
import AppGrid from "./components/shell/AppGrid";
import StatusBar from "./components/shell/StatusBar";
import LoginPrompt from "./components/shell/LoginPrompt";
import AlertManager from "./components/shell/AlertManager";
import WindowManager from "./components/shell/WindowManager";
import Dock from "./components/shell/Dock";
import { useAuth, useBackendStatus } from "./api/hooks";
import { WindowProvider } from "./context/WindowContext";
import { AlertProvider, useAlert } from "./context/AlertContext";

const AppContent = () => {
  const { isAuthenticated, checking: authChecking, login, logout } = useAuth();
  const { isReachable, checking } = useBackendStatus();
  const { showAlert } = useAlert();

  // Show backend unreachable error
  useEffect(() => {
    if (!checking && !isReachable) {
      showAlert(
        "stop",
        "Backend Unreachable",
        "Unable to connect to the Post-Urbit backend. Please make sure the node is running."
      );
    }
  }, [checking, isReachable, showAlert]);

  // Show loading state while checking backend
  if (checking) {
    return (
      <div className="s7-shell">
        <div style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          height: '100vh',
          padding: '20px',
        }}>
          <p>Connecting to backend...</p>
        </div>
      </div>
    );
  }

  if (authChecking) {
    return (
      <div className="s7-shell">
        <div style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          height: '100vh',
          padding: '20px',
        }}>
          <p>Checking authentication...</p>
        </div>
      </div>
    );
  }

  // Show login prompt if not authenticated
  if (!isAuthenticated) {
    return (
      <div className="s7-shell">
        <LoginPrompt onLogin={login} />
      </div>
    );
  }

  // Main shell UI
  return (
    <WindowProvider>
      <div className="s7-shell">
        <MenuBar onLogout={logout} />
        <div className="s7-desktop">
          <div className="s7-desktop-grid">
            <AppGrid />
          </div>
          <WindowManager />
        </div>
        <StatusBar />
        <Dock />
      </div>
    </WindowProvider>
  );
};

const App = () => {
  return (
    <AlertProvider>
      <AppContent />
      <AlertManager />
    </AlertProvider>
  );
};

export default App;
