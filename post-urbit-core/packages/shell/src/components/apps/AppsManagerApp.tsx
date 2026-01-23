import React, { useState, useMemo } from 'react';
import { useApps, useAppRuntime, useAppSecrets, useAppPermissions, useAppSettings, useAppActions } from '../../api/hooks';
import { useWindows } from '../../context/WindowContext';
import Button from '../system7/Button';
import type { App } from '../../api/types';

/**
 * AppsManagerApp - Manage installed applications
 *
 * Features:
 * - Two-panel layout: app list + inspector
 * - View app info, runtime, permissions, secrets, settings
 * - Start/stop/restart/update/clear data actions
 * - System 7 styled UI
 *
 * Usage:
 * <AppsManagerApp />
 */

// Built-in system apps that always appear
const SYSTEM_APPS: App[] = [
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

type TabType = 'info' | 'runtime' | 'permissions' | 'secrets' | 'settings';

// Inspector component that only renders when an app is selected
// This prevents calling hooks with empty strings
interface AppInspectorProps {
  app: App;
  activeTab: TabType;
  setActiveTab: (tab: TabType) => void;
  refetchApps: () => void;
}

const AppInspector = ({ app, activeTab, setActiveTab, refetchApps }: AppInspectorProps) => {
  const { data: runtime } = useAppRuntime(app.id);
  const { data: secretsResponse } = useAppSecrets(app.id);
  const { data: permissions } = useAppPermissions(app.id);
  const { data: settings } = useAppSettings(app.id);
  const { startApp, stopApp, restartApp, updateApp, clearAppData } = useAppActions();
  const [actionLoading, setActionLoading] = React.useState(false);
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);
  const secrets = secretsResponse?.secrets || [];

  const handleStart = async () => {
    setActionLoading(true);
    try {
      await startApp(app.id);
      refetchApps();
    } catch (error) {
      console.error('Failed to start app:', error);
    } finally {
      setActionLoading(false);
    }
  };

  const handleStop = async () => {
    setActionLoading(true);
    try {
      await stopApp(app.id);
      refetchApps();
    } catch (error) {
      console.error('Failed to stop app:', error);
    } finally {
      setActionLoading(false);
    }
  };

  const handleRestart = async () => {
    setActionLoading(true);
    try {
      await restartApp(app.id);
      refetchApps();
    } catch (error) {
      console.error('Failed to restart app:', error);
    } finally {
      setActionLoading(false);
    }
  };

  const handleUpdate = async () => {
    setActionLoading(true);
    try {
      await updateApp(app.id);
      refetchApps();
    } catch (error) {
      console.error('Failed to update app:', error);
    } finally {
      setActionLoading(false);
    }
  };

  const handleClearData = async () => {
    setActionLoading(true);
    try {
      await clearAppData(app.id);
      setShowConfirmDialog(false);
      refetchApps();
    } catch (error) {
      console.error('Failed to clear app data:', error);
    } finally {
      setActionLoading(false);
    }
  };

  const handleGrantPermission = async (permission: string) => {
    // TODO: Implement grant permission
    console.log('Grant permission:', permission);
  };

  const handleRevokePermission = async (permission: string) => {
    // TODO: Implement revoke permission
    console.log('Revoke permission:', permission);
  };

  const handleDenyPermission = async (permission: string) => {
    // TODO: Implement deny permission
    console.log('Deny permission:', permission);
  };

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
  };

  const TabButton = ({ tab, label }: { tab: TabType; label: string }) => (
    <button
      onClick={() => setActiveTab(tab)}
      style={{
        padding: '3px 8px',
        border: '1px solid black',
        background: activeTab === tab ? 'black' : 'white',
        color: activeTab === tab ? 'white' : 'black',
        fontFamily: 'var(--font-chicago)',
        fontSize: '10px',
        cursor: 'pointer',
        marginRight: '2px',
      }}
    >
      {label}
    </button>
  );

  return (
    <>
      {/* Header */}
      <div style={{
        padding: '8px',
        borderBottom: '1px solid black',
        background: '#f0f0f0',
      }}>
        <div style={{ fontWeight: 'bold', marginBottom: '4px' }}>{app.name}</div>
        <div style={{ fontSize: '10px', color: '#666' }}>
          v{app.version} by {app.author_name}
        </div>
      </div>

      {/* Tabs */}
      <div style={{ padding: '8px', borderBottom: '1px solid black' }}>
        <TabButton tab="info" label="Info" />
        <TabButton tab="runtime" label="Runtime" />
        <TabButton tab="permissions" label="Permissions" />
        <TabButton tab="secrets" label="Secrets" />
        <TabButton tab="settings" label="Settings" />
      </div>

      {/* Tab Content */}
      <div style={{ padding: '8px', flex: 1, overflowY: 'auto' }}>
        {activeTab === 'info' && (
          <div>
            <div style={{ marginBottom: '8px' }}>
              <strong>Description:</strong>
              <div style={{ marginTop: '4px' }}>{app.description || 'No description'}</div>
            </div>
            <div style={{ marginBottom: '8px' }}>
              <strong>Installed:</strong> {new Date(app.installed_at).toLocaleDateString()}
            </div>
            {app.last_opened && (
              <div style={{ marginBottom: '8px' }}>
                <strong>Last Opened:</strong> {new Date(app.last_opened).toLocaleDateString()}
              </div>
            )}
            <div style={{ marginBottom: '8px' }}>
              <strong>Storage:</strong>
              <div style={{
                marginTop: '4px',
                height: '16px',
                border: '1px solid black',
                background: 'white',
              }}>
                <div style={{
                  height: '100%',
                  width: `${Math.min(100, (app.storage_used / (app.storage_quota || 1)) * 100)}%`,
                  background: (app.storage_used / (app.storage_quota || 1)) > 0.9 ? 'red' : 'green',
                }} />
              </div>
              <div style={{ fontSize: '10px', marginTop: '2px' }}>
                {formatBytes(app.storage_used)} / {formatBytes(app.storage_quota)}
              </div>
            </div>
            {app.update_available && (
              <div style={{
                padding: '4px 8px',
                background: '#ffffcc',
                border: '1px solid #cccc00',
                marginTop: '8px',
              }}>
                Update available: v{app.update_available}
              </div>
            )}
          </div>
        )}

        {activeTab === 'runtime' && (
          <div>
            <div style={{ marginBottom: '8px' }}>
              <strong>Status:</strong>{' '}
              <span style={{ color: runtime?.running ? 'green' : 'gray' }}>
                {runtime?.running ? 'Running' : 'Stopped'}
              </span>
            </div>
            <div style={{ marginBottom: '8px' }}>
              <strong>Capabilities:</strong>
              {runtime?.capabilities && runtime.capabilities.length > 0 ? (
                <ul style={{ margin: '4px 0', paddingLeft: '20px' }}>
                  {runtime.capabilities.map((cap, i) => (
                    <li key={i} style={{ fontSize: '11px' }}>{cap}</li>
                  ))}
                </ul>
              ) : (
                <div style={{ color: '#666', marginTop: '4px' }}>None</div>
              )}
            </div>
            <div style={{ marginBottom: '8px' }}>
              <strong>Configured Secrets:</strong>
              {runtime?.secrets_configured && runtime.secrets_configured.length > 0 ? (
                <ul style={{ margin: '4px 0', paddingLeft: '20px' }}>
                  {runtime.secrets_configured.map((s, i) => (
                    <li key={i} style={{ fontSize: '11px', color: 'green' }}>{s}</li>
                  ))}
                </ul>
              ) : (
                <div style={{ color: '#666', marginTop: '4px' }}>None</div>
              )}
            </div>
            {runtime?.secrets_missing && runtime.secrets_missing.length > 0 && (
              <div style={{ marginBottom: '8px' }}>
                <strong style={{ color: 'red' }}>Missing Secrets:</strong>
                <ul style={{ margin: '4px 0', paddingLeft: '20px' }}>
                  {runtime.secrets_missing.map((s, i) => (
                    <li key={i} style={{ fontSize: '11px', color: 'red' }}>{s}</li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}

        {activeTab === 'permissions' && (
          <div>
            <div style={{ marginBottom: '12px' }}>
              <strong>Granted:</strong>
              {permissions?.granted && permissions.granted.length > 0 ? (
                <div style={{ marginTop: '4px' }}>
                  {permissions.granted.map((p, i) => (
                    <div key={i} style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      padding: '4px',
                      background: '#ccffcc',
                      border: '1px solid #009900',
                      marginBottom: '2px',
                    }}>
                      <span style={{ fontSize: '11px' }}>{p}</span>
                      <button
                        onClick={() => handleRevokePermission(p)}
                        style={{
                          fontSize: '9px',
                          padding: '2px 4px',
                          cursor: 'pointer',
                        }}
                      >
                        Revoke
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <div style={{ color: '#666', marginTop: '4px' }}>None</div>
              )}
            </div>
            <div style={{ marginBottom: '12px' }}>
              <strong>Denied:</strong>
              {permissions?.denied && permissions.denied.length > 0 ? (
                <div style={{ marginTop: '4px' }}>
                  {permissions.denied.map((p, i) => (
                    <div key={i} style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      padding: '4px',
                      background: '#ffcccc',
                      border: '1px solid #990000',
                      marginBottom: '2px',
                    }}>
                      <span style={{ fontSize: '11px' }}>{p}</span>
                      <button
                        onClick={() => handleGrantPermission(p)}
                        style={{
                          fontSize: '9px',
                          padding: '2px 4px',
                          cursor: 'pointer',
                        }}
                      >
                        Grant
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <div style={{ color: '#666', marginTop: '4px' }}>None</div>
              )}
            </div>
            <div>
              <strong>Pending:</strong>
              {permissions?.pending && permissions.pending.length > 0 ? (
                <div style={{ marginTop: '4px' }}>
                  {permissions.pending.map((p, i) => (
                    <div key={i} style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      padding: '4px',
                      background: '#ffffcc',
                      border: '1px solid #cccc00',
                      marginBottom: '2px',
                    }}>
                      <span style={{ fontSize: '11px' }}>{p}</span>
                      <div>
                        <button
                          onClick={() => handleGrantPermission(p)}
                          style={{
                            fontSize: '9px',
                            padding: '2px 4px',
                            cursor: 'pointer',
                            marginRight: '4px',
                          }}
                        >
                          Grant
                        </button>
                        <button
                          onClick={() => handleDenyPermission(p)}
                          style={{
                            fontSize: '9px',
                            padding: '2px 4px',
                            cursor: 'pointer',
                          }}
                        >
                          Deny
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div style={{ color: '#666', marginTop: '4px' }}>None</div>
              )}
            </div>
          </div>
        )}

        {activeTab === 'secrets' && (
          <div>
            {secrets.length > 0 ? (
              secrets.map((secret, i) => (
                <div key={i} style={{
                  padding: '8px',
                  border: '1px solid black',
                  marginBottom: '8px',
                }}>
                  <div style={{ fontWeight: 'bold', marginBottom: '4px' }}>
                    {secret.name}
                    {secret.required && <span style={{ color: 'red' }}> *</span>}
                  </div>
                  <div style={{ fontSize: '11px', marginBottom: '4px' }}>
                    {secret.description}
                  </div>
                  <div style={{
                    fontSize: '10px',
                    color: secret.configured ? 'green' : 'red',
                  }}>
                    {secret.configured ? '✓ Configured' : '✗ Not Set'}
                  </div>
                </div>
              ))
            ) : (
              <div style={{ color: '#666' }}>No secrets required</div>
            )}
          </div>
        )}

        {activeTab === 'settings' && (
          <div>
            {settings && Object.keys(settings).length > 0 ? (
              <div style={{
                fontFamily: 'monospace',
                fontSize: '11px',
                background: '#f0f0f0',
                padding: '8px',
                border: '1px solid black',
              }}>
                {Object.entries(settings).map(([key, value]) => (
                  <div key={key} style={{ marginBottom: '4px' }}>
                    <strong>{key}:</strong>{' '}
                    {typeof value === 'object' ? JSON.stringify(value) : String(value)}
                  </div>
                ))}
              </div>
            ) : (
              <div style={{ color: '#666' }}>No settings</div>
            )}
          </div>
        )}
      </div>

      {/* Action Buttons */}
      <div style={{
        padding: '8px',
        borderTop: '1px solid black',
        display: 'flex',
        gap: '4px',
        flexWrap: 'wrap',
      }}>
        {app.status === 'running' ? (
          <Button onClick={handleStop} disabled={actionLoading}>Stop</Button>
        ) : (
          <Button onClick={handleStart} disabled={actionLoading}>Start</Button>
        )}
        {app.status === 'running' && (
          <Button onClick={handleRestart} disabled={actionLoading}>Restart</Button>
        )}
        {app.update_available && (
          <Button onClick={handleUpdate} disabled={actionLoading}>Update</Button>
        )}
        <Button onClick={() => setShowConfirmDialog(true)} disabled={actionLoading}>
          Clear Data
        </Button>
      </div>

      {/* Confirm Dialog */}
      {showConfirmDialog && (
        <div style={{
          position: 'fixed',
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          background: 'rgba(0,0,0,0.5)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          zIndex: 10000,
        }}>
          <div style={{
            background: 'white',
            border: '2px solid black',
            padding: '16px',
            maxWidth: '300px',
            boxShadow: '4px 4px 0 black',
          }}>
            <div style={{ marginBottom: '12px', fontWeight: 'bold' }}>
              Clear App Data?
            </div>
            <div style={{ marginBottom: '16px' }}>
              Clear all data for {app.name}? This cannot be undone.
            </div>
            <div style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end' }}>
              <Button onClick={() => setShowConfirmDialog(false)}>Cancel</Button>
              <Button onClick={handleClearData} disabled={actionLoading}>Clear</Button>
            </div>
          </div>
        </div>
      )}
    </>
  );
};

const AppsManagerApp = () => {
  const { data: backendApps, isInitialLoading, error, refetch } = useApps();
  const { isAppOpen } = useWindows();
  const [selectedAppId, setSelectedAppId] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<TabType>('info');

  // Merge system apps with backend apps, avoiding duplicates
  const apps = useMemo(() => {
    const backendAppIds = new Set((backendApps || []).map(a => a.id));
    const uniqueSystemApps = SYSTEM_APPS.filter(a => !backendAppIds.has(a.id));
    return [...uniqueSystemApps, ...(backendApps || [])];
  }, [backendApps]);

  const selectedApp = apps.find(app => app.id === selectedAppId);

  // Get effective status (check if window is open for system apps)
  const getEffectiveStatus = (app: App): string => {
    // For system apps, check if window is open
    if (app.author_iid === 'system') {
      return isAppOpen(app.id) ? 'running' : 'installed';
    }
    return app.status;
  };

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'running': return 'green';
      case 'error': return 'red';
      case 'disabled': return 'gray';
      default: return 'blue';
    }
  };

  // Show loading only if we have no apps at all (shouldn't happen with system apps)
  if (isInitialLoading && apps.length === 0) {
    return (
      <div style={{ padding: '12px' }}>
        Loading applications...
      </div>
    );
  }

  // Show error only if we have no apps (shouldn't happen with system apps)
  if (error && apps.length === 0) {
    return (
      <div style={{ padding: '12px', color: 'red' }}>
        Error loading apps: {error.message}
      </div>
    );
  }

  return (
    <div style={{
      display: 'flex',
      height: '100%',
      minWidth: '600px',
      minHeight: '400px',
    }}>
      {/* Left Panel - App List */}
      <div style={{
        width: '180px',
        borderRight: '1px solid black',
        background: 'white',
        overflow: 'auto',
      }}>
        {apps?.map(app => (
          <div
            key={app.id}
            onClick={() => {
              setSelectedAppId(app.id);
              setActiveTab('info');
            }}
            style={{
              padding: '8px',
              cursor: 'pointer',
              background: selectedAppId === app.id ? 'black' : 'white',
              color: selectedAppId === app.id ? 'white' : 'black',
              borderBottom: '1px solid #ddd',
              fontSize: '11px',
              fontFamily: 'var(--font-chicago)',
            }}
          >
            <div style={{ marginBottom: '2px' }}>{app.name}</div>
            <div style={{
              fontSize: '9px',
              color: selectedAppId === app.id ? 'white' : getStatusColor(getEffectiveStatus(app)),
              fontWeight: 'bold',
            }}>
              {getEffectiveStatus(app).toUpperCase()}
            </div>
          </div>
        ))}
      </div>

      {/* Right Panel - App Inspector */}
      <div style={{
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        background: 'white',
      }}>
        {!selectedApp ? (
          <div style={{
            padding: '20px',
            textAlign: 'center',
            color: '#666',
            fontFamily: 'var(--font-chicago)',
            fontSize: '11px',
          }}>
            Select an app to view details
          </div>
        ) : (
          <AppInspector
            app={selectedApp}
            activeTab={activeTab}
            setActiveTab={setActiveTab}
            refetchApps={refetch}
          />
        )}
      </div>
    </div>
  );
};

export default AppsManagerApp;
