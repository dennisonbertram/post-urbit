import React, { useState, useCallback } from "react";
import Icon from "../system7/Icon";
import { useApps } from "../../api/hooks";
import { useWindows } from "../../context/WindowContext";
import FilesApp from "../apps/FilesApp";
import SettingsApp from "../apps/SettingsApp";
import AppStoreApp from "../apps/AppStoreApp";
import type { App } from "../../api/types";

// Valid icon names from Icon component
type IconName = "folder" | "mail" | "notes" | "browser" | "settings" | "trash" | "activity" | "install";

// Map icon names to known system icon types
const getIconForApp = (appId: string): IconName => {
  const iconMap: Record<string, IconName> = {
    'com.posturbit.files': 'folder',
    'com.posturbit.mail': 'mail',
    'com.posturbit.notes': 'notes',
    'com.posturbit.browser': 'browser',
    'com.posturbit.settings': 'settings',
    'com.posturbit.activity': 'activity',
  };
  return iconMap[appId] || 'install';
};

// Map app IDs to their content components
const getAppContent = (app: App): React.ReactNode => {
  const contentMap: Record<string, React.ReactNode> = {
    'com.posturbit.files': <FilesApp />,
    'com.posturbit.settings': <SettingsApp />,
    'com.posturbit.browser': <AppStoreApp />,
  };

  return contentMap[app.id] || (
    <div style={{ padding: '12px' }}>
      <p>App: {app.name}</p>
      <p>Version: {app.version}</p>
      <p>Status: {app.status}</p>
      <p style={{ marginTop: '12px', color: '#666' }}>
        This app has no UI yet.
      </p>
    </div>
  );
};

const AppGrid = () => {
  const { data: apps, loading, error } = useApps();
  const { openWindow, isAppOpen } = useWindows();
  const [selectedAppId, setSelectedAppId] = useState<string | null>(null);

  // Show loading state
  if (loading) {
    return (
      <div className="s7-app-grid">
        <div style={{ padding: '20px', textAlign: 'center' }}>
          Loading apps...
        </div>
      </div>
    );
  }

  // Show error state
  if (error) {
    return (
      <div className="s7-app-grid">
        <div style={{ padding: '20px', textAlign: 'center', color: '#cc0000' }}>
          Failed to load apps: {error.message}
        </div>
      </div>
    );
  }

  // Show empty state
  if (!apps || apps.length === 0) {
    return (
      <div className="s7-app-grid">
        <div style={{ padding: '20px', textAlign: 'center' }}>
          No apps installed
        </div>
      </div>
    );
  }

  // Handle single click to select
  const handleClick = useCallback((app: App, event: React.MouseEvent) => {
    event.stopPropagation();
    setSelectedAppId(app.id);
  }, []);

  // Handle double click to open
  const handleDoubleClick = useCallback((app: App, event: React.MouseEvent) => {
    event.stopPropagation();

    // Prevent opening duplicate windows
    if (isAppOpen(app.id)) {
      return;
    }

    // Open the app in a new window
    openWindow(app.id, app.name, getAppContent(app));
  }, [openWindow, isAppOpen]);

  // Handle click on desktop to deselect
  const handleDesktopClick = useCallback(() => {
    setSelectedAppId(null);
  }, []);

  return (
    <div className="s7-app-grid" onClick={handleDesktopClick}>
      {apps.map((app) => {
        const isSelected = selectedAppId === app.id;
        const isRunning = app.status === 'running' || isAppOpen(app.id);

        return (
          <div
            className={`s7-app-tile ${isSelected ? 's7-app-tile-selected' : ''}`}
            key={app.id}
            onClick={(e) => handleClick(app, e)}
            onDoubleClick={(e) => handleDoubleClick(app, e)}
          >
            <div className="s7-app-icon">
              {app.icon ? (
                <img
                  src={app.icon}
                  alt={app.name}
                  style={{
                    width: '32px',
                    height: '32px',
                    filter: isSelected ? 'invert(1)' : 'none',
                  }}
                />
              ) : (
                <Icon
                  name={getIconForApp(app.id)}
                  selected={isSelected}
                />
              )}
            </div>
            <div className="s7-app-label">{app.name}</div>
            {isRunning && (
              <div className="s7-app-status s7-app-status-running">
                Running
              </div>
            )}
            {app.status === 'error' && (
              <div className="s7-app-status s7-app-status-error">
                Error
              </div>
            )}
            {app.status === 'disabled' && (
              <div className="s7-app-status s7-app-status-disabled">
                Disabled
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
};

export default AppGrid;
