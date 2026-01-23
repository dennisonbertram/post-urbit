import React, { useState, useCallback } from "react";
import Icon from "../system7/Icon";
import { useApps } from "../../api/hooks";
import { useWindows } from "../../context/WindowContext";
import FilesApp from "../apps/FilesApp";
import SettingsApp from "../apps/SettingsApp";
import AppStoreApp from "../apps/AppStoreApp";
import AppsManagerApp from "../apps/AppsManagerApp";
import SystemMonitorApp from "../apps/SystemMonitorApp";
import MessagesApp from "../apps/MessagesApp";
import type { App } from "../../api/types";

// Built-in system apps that always appear
const SYSTEM_APPS: App[] = [
  {
    id: 'com.posturbit.mail',
    name: 'Mail',
    version: '1.0.0',
    author_name: 'Post-Urbit',
    author_iid: 'system',
    description: 'Send and receive messages',
    status: 'installed',
    permissions: { granted: [], denied: [], pending: [] },
    installed_at: new Date().toISOString(),
    storage_used: 0,
    storage_quota: 0,
  },
  {
    id: 'com.posturbit.monitor',
    name: 'System Monitor',
    version: '1.0.0',
    author_name: 'Post-Urbit',
    author_iid: 'system',
    description: 'Monitor system health, status, and logs',
    status: 'installed',
    permissions: { granted: [], denied: [], pending: [] },
    installed_at: new Date().toISOString(),
    storage_used: 0,
    storage_quota: 0,
  },
  {
    id: 'com.posturbit.apps',
    name: 'Apps Manager',
    version: '1.0.0',
    author_name: 'Post-Urbit',
    author_iid: 'system',
    description: 'Manage installed applications',
    status: 'installed',
    permissions: { granted: [], denied: [], pending: [] },
    installed_at: new Date().toISOString(),
    storage_used: 0,
    storage_quota: 0,
  },
];

const normalizeAppId = (appId: string) => {
  return appId === 'com.posturbit.messages' ? 'com.posturbit.mail' : appId;
};

// Valid icon names from Icon component
type IconName = "folder" | "mail" | "notes" | "browser" | "settings" | "trash" | "activity" | "install";

// Map icon names to known system icon types
const getIconForApp = (appId: string): IconName => {
  const iconMap: Record<string, IconName> = {
    'com.posturbit.files': 'folder',
    'com.posturbit.mail': 'mail',
    'com.posturbit.messages': 'mail',
    'com.posturbit.notes': 'notes',
    'com.posturbit.browser': 'browser',
    'com.posturbit.settings': 'settings',
    'com.posturbit.activity': 'activity',
    'com.posturbit.apps': 'settings',
    'com.posturbit.monitor': 'activity',
  };
  return iconMap[appId] || 'install';
};

// Map app IDs to their content components
const getAppContent = (app: App): React.ReactNode => {
  const contentMap: Record<string, React.ReactNode> = {
    'com.posturbit.files': <FilesApp />,
    'com.posturbit.settings': <SettingsApp />,
    'com.posturbit.browser': <AppStoreApp />,
    'com.posturbit.apps': <AppsManagerApp />,
    'com.posturbit.monitor': <SystemMonitorApp />,
    'com.posturbit.messages': <MessagesApp />,
    'com.posturbit.mail': <MessagesApp />,
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
  const { data: backendApps, isInitialLoading, error } = useApps();
  const { openWindow, isAppOpen } = useWindows();
  const [selectedAppId, setSelectedAppId] = useState<string | null>(null);

  // Merge system apps with backend apps, avoiding duplicates
  const apps = React.useMemo(() => {
    const backendAppIds = new Set((backendApps || []).map(a => normalizeAppId(a.id)));
    const uniqueSystemApps = SYSTEM_APPS.filter(a => !backendAppIds.has(normalizeAppId(a.id)));
    return [...uniqueSystemApps, ...(backendApps || [])];
  }, [backendApps]);

  // Show loading state (but still show system apps)
  if (isInitialLoading && apps.length === 0) {
    return (
      <div className="s7-app-grid">
        <div style={{ padding: '20px', textAlign: 'center' }}>
          Loading apps...
        </div>
      </div>
    );
  }

  // Show error state (but still show system apps if we have them)
  if (error && apps.length === 0) {
    return (
      <div className="s7-app-grid">
        <div style={{ padding: '20px', textAlign: 'center', color: '#cc0000' }}>
          Failed to load apps: {error.message}
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
