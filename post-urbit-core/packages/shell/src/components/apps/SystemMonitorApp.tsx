import React, { useState, useMemo } from 'react';
import { useNodeStatus, useHealth, useLogs } from '../../api/hooks';
import type { LogLevel } from '../../api/types';
import Button from '../system7/Button';
import Checkbox from '../system7/Checkbox';
import TextInput from '../system7/TextInput';

/**
 * SystemMonitorApp - System monitoring and logs viewer
 * Features:
 * - Overview: Node status, network stats, storage, apps
 * - Health: Detailed subsystem health checks
 * - Logs: Filterable log viewer with auto-refresh
 *
 * Styling: System 7 aesthetic with Chicago font, black borders, white backgrounds
 */

type TabType = 'overview' | 'health' | 'logs';

const SystemMonitorApp = () => {
  const [activeTab, setActiveTab] = useState<TabType>('overview');
  const { data: status, loading: statusLoading } = useNodeStatus();
  const { data: health, loading: healthLoading } = useHealth();

  // Logs state
  const [logLevel, setLogLevel] = useState<LogLevel | 'all'>('all');
  const [logSearch, setLogSearch] = useState('');
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [isPaused, setIsPaused] = useState(false);
  const [logCursor, setLogCursor] = useState<string | undefined>(undefined);

  // Build query params for logs - memoized to prevent unnecessary refetches
  const logsParams = useMemo(() => ({
    limit: 50,
    level: logLevel === 'all' ? undefined : logLevel,
    search: logSearch || undefined,
    cursor: logCursor,
  }), [logLevel, logSearch, logCursor]);

  // Poll interval: 5000ms if auto-refresh enabled and not paused, otherwise 0 (no polling)
  const pollInterval = (activeTab === 'logs' && autoRefresh && !isPaused) ? 5000 : 0;

  const { data: logs, loading: logsLoading, refetch: refetchLogs, hasMore } = useLogs(
    logsParams,
    pollInterval
  );

  // Format uptime as Xh Xm Xs
  const formatUptime = (seconds: number): string => {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = Math.floor(seconds % 60);
    return `${h}h ${m}m ${s}s`;
  };

  // Format bytes to human readable
  const formatBytes = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
    return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
  };

  // Truncate IID for display
  const truncateIID = (iid: string): string => {
    return iid.length > 16 ? `${iid.slice(0, 16)}...` : iid;
  };

  // Status badge component
  const StatusBadge = ({ status }: { status: 'healthy' | 'unhealthy' }) => (
    <span style={{
      display: 'inline-block',
      padding: '2px 6px',
      border: '1px solid black',
      background: status === 'healthy' ? '#00ff00' : '#ff0000',
      fontFamily: 'var(--font-chicago)',
      fontSize: '10px',
      fontWeight: 'bold',
    }}>
      {status.toUpperCase()}
    </span>
  );

  // Progress bar component
  const ProgressBar = ({ used, total }: { used: number; total: number }) => {
    const percentage = total > 0 ? (used / total) * 100 : 0;
    return (
      <div style={{
        width: '100%',
        height: '16px',
        border: '1px solid black',
        background: 'white',
        position: 'relative',
      }}>
        <div style={{
          width: `${percentage}%`,
          height: '100%',
          background: percentage > 90 ? '#ff0000' : percentage > 70 ? '#ffff00' : '#00ff00',
        }} />
        <div style={{
          position: 'absolute',
          top: '50%',
          left: '50%',
          transform: 'translate(-50%, -50%)',
          fontSize: '9px',
          fontWeight: 'bold',
        }}>
          {percentage.toFixed(1)}%
        </div>
      </div>
    );
  };

  // Log level badge with color
  const LogLevelBadge = ({ level }: { level: LogLevel }) => {
    const colors: Record<LogLevel, string> = {
      debug: '#888888',
      info: '#0000ff',
      warn: '#ffaa00',
      error: '#ff0000',
    };
    return (
      <span style={{
        display: 'inline-block',
        padding: '1px 4px',
        background: colors[level],
        color: 'white',
        fontFamily: 'var(--font-chicago)',
        fontSize: '9px',
        fontWeight: 'bold',
        minWidth: '40px',
        textAlign: 'center',
      }}>
        {level.toUpperCase()}
      </span>
    );
  };

  // Tab buttons
  const TabButton = ({ tab, label }: { tab: TabType; label: string }) => (
    <button
      onClick={() => setActiveTab(tab)}
      style={{
        padding: '4px 12px',
        border: '1px solid black',
        background: activeTab === tab ? 'black' : 'white',
        color: activeTab === tab ? 'white' : 'black',
        fontFamily: 'var(--font-chicago)',
        fontSize: '11px',
        cursor: 'pointer',
        marginRight: '4px',
      }}
    >
      {label}
    </button>
  );

  return (
    <div style={{ padding: '8px', minWidth: '500px', minHeight: '400px' }}>
      {/* Tab Navigation */}
      <div style={{ marginBottom: '12px' }}>
        <TabButton tab="overview" label="Overview" />
        <TabButton tab="health" label="Health" />
        <TabButton tab="logs" label="Logs" />
      </div>

      {/* Overview Tab */}
      {activeTab === 'overview' && (
        <div>
          {statusLoading ? (
            <div style={{ padding: '12px' }}>Loading overview...</div>
          ) : status ? (
            <>
              {/* Node Info */}
              <div style={{ marginBottom: '16px' }}>
                <h3 style={{
                  margin: '0 0 8px 0',
                  fontFamily: 'var(--font-chicago)',
                  fontSize: '12px',
                }}>
                  Node Information
                </h3>
                <div style={{
                  border: '1px solid black',
                  background: 'white',
                  padding: '8px',
                }}>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Version:</strong> {status.version}
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Uptime:</strong> {formatUptime(status.uptime_seconds)}
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Status:</strong> <StatusBadge status={status.status} />
                  </div>
                </div>
              </div>

              {/* Network Stats */}
              <div style={{ marginBottom: '16px' }}>
                <h3 style={{
                  margin: '0 0 8px 0',
                  fontFamily: 'var(--font-chicago)',
                  fontSize: '12px',
                }}>
                  Network
                </h3>
                <div style={{
                  border: '1px solid black',
                  background: 'white',
                  padding: '8px',
                }}>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Connections:</strong> {status.network.connections_active} active
                    ({status.network.connections_direct} direct, {status.network.connections_relay} relay)
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Relays:</strong> {status.network.relays_connected} connected
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Sent:</strong> {formatBytes(status.network.bytes_sent)}
                    {' | '}
                    <strong>Received:</strong> {formatBytes(status.network.bytes_received)}
                  </div>
                </div>
              </div>

              {/* Storage Stats */}
              <div style={{ marginBottom: '16px' }}>
                <h3 style={{
                  margin: '0 0 8px 0',
                  fontFamily: 'var(--font-chicago)',
                  fontSize: '12px',
                }}>
                  Storage
                </h3>
                <div style={{
                  border: '1px solid black',
                  background: 'white',
                  padding: '8px',
                }}>
                  <div style={{ marginBottom: '8px' }}>
                    <ProgressBar
                      used={status.storage.data_used_bytes}
                      total={status.storage.data_used_bytes + status.storage.data_free_bytes}
                    />
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Used:</strong> {formatBytes(status.storage.data_used_bytes)} / {formatBytes(status.storage.data_used_bytes + status.storage.data_free_bytes)}
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Messages:</strong> {status.storage.messages_count}
                  </div>
                  <div>
                    <strong>Documents:</strong> {status.storage.documents_count}
                  </div>
                </div>
              </div>

              {/* Apps Stats */}
              <div style={{ marginBottom: '16px' }}>
                <h3 style={{
                  margin: '0 0 8px 0',
                  fontFamily: 'var(--font-chicago)',
                  fontSize: '12px',
                }}>
                  Applications
                </h3>
                <div style={{
                  border: '1px solid black',
                  background: 'white',
                  padding: '8px',
                }}>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Installed:</strong> {status.apps.installed}
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Running:</strong> {status.apps.running}
                  </div>
                  <div>
                    <strong>Storage Used:</strong> {formatBytes(status.apps.total_storage_used)}
                  </div>
                </div>
              </div>
            </>
          ) : (
            <div style={{ padding: '12px' }}>Failed to load status</div>
          )}
        </div>
      )}

      {/* Health Tab */}
      {activeTab === 'health' && (
        <div>
          {healthLoading ? (
            <div style={{ padding: '12px' }}>Loading health checks...</div>
          ) : health ? (
            <>
              {/* Identity Check */}
              <div style={{ marginBottom: '12px' }}>
                <h3 style={{
                  margin: '0 0 8px 0',
                  fontFamily: 'var(--font-chicago)',
                  fontSize: '12px',
                }}>
                  Identity
                </h3>
                <div style={{
                  border: '1px solid black',
                  background: 'white',
                  padding: '8px',
                }}>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Status:</strong> <StatusBadge status={health.checks.identity.status} />
                  </div>
                  <div style={{ marginBottom: '4px', fontSize: '10px' }}>
                    <strong>IID:</strong> {truncateIID(health.checks.identity.iid)}
                  </div>
                  {health.checks.identity.last_published && (
                    <div style={{ fontSize: '10px' }}>
                      <strong>Last Published:</strong> {new Date(health.checks.identity.last_published).toLocaleString()}
                    </div>
                  )}
                </div>
              </div>

              {/* Transport Check */}
              <div style={{ marginBottom: '12px' }}>
                <h3 style={{
                  margin: '0 0 8px 0',
                  fontFamily: 'var(--font-chicago)',
                  fontSize: '12px',
                }}>
                  Transport
                </h3>
                <div style={{
                  border: '1px solid black',
                  background: 'white',
                  padding: '8px',
                }}>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Status:</strong> <StatusBadge status={health.checks.transport.status} />
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Connections:</strong> {health.checks.transport.connections}
                  </div>
                  <div>
                    <strong>Relays Connected:</strong> {health.checks.transport.relays_connected}
                  </div>
                </div>
              </div>

              {/* Messaging Check */}
              <div style={{ marginBottom: '12px' }}>
                <h3 style={{
                  margin: '0 0 8px 0',
                  fontFamily: 'var(--font-chicago)',
                  fontSize: '12px',
                }}>
                  Messaging
                </h3>
                <div style={{
                  border: '1px solid black',
                  background: 'white',
                  padding: '8px',
                }}>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Status:</strong> <StatusBadge status={health.checks.messaging.status} />
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Queue Depth:</strong> {health.checks.messaging.queue_depth}
                  </div>
                  <div>
                    <strong>Active Sessions:</strong> {health.checks.messaging.sessions_active}
                  </div>
                </div>
              </div>

              {/* Storage Check */}
              <div style={{ marginBottom: '12px' }}>
                <h3 style={{
                  margin: '0 0 8px 0',
                  fontFamily: 'var(--font-chicago)',
                  fontSize: '12px',
                }}>
                  Storage
                </h3>
                <div style={{
                  border: '1px solid black',
                  background: 'white',
                  padding: '8px',
                }}>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Status:</strong> <StatusBadge status={health.checks.storage.status} />
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Used:</strong> {formatBytes(health.checks.storage.disk_used_bytes)}
                  </div>
                  <div>
                    <strong>Free:</strong> {formatBytes(health.checks.storage.disk_free_bytes)}
                  </div>
                </div>
              </div>

              {/* Apps Check */}
              <div style={{ marginBottom: '12px' }}>
                <h3 style={{
                  margin: '0 0 8px 0',
                  fontFamily: 'var(--font-chicago)',
                  fontSize: '12px',
                }}>
                  Applications
                </h3>
                <div style={{
                  border: '1px solid black',
                  background: 'white',
                  padding: '8px',
                }}>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Status:</strong> <StatusBadge status={health.checks.apps.status} />
                  </div>
                  <div style={{ marginBottom: '4px' }}>
                    <strong>Installed:</strong> {health.checks.apps.installed}
                  </div>
                  <div>
                    <strong>Running:</strong> {health.checks.apps.running}
                  </div>
                </div>
              </div>
            </>
          ) : (
            <div style={{ padding: '12px' }}>Failed to load health data</div>
          )}
        </div>
      )}

      {/* Logs Tab */}
      {activeTab === 'logs' && (
        <div>
          {/* Filter Bar */}
          <div style={{
            marginBottom: '12px',
            padding: '8px',
            border: '1px solid black',
            background: 'white',
          }}>
            <div style={{ display: 'flex', gap: '8px', marginBottom: '8px', alignItems: 'center' }}>
              <label style={{ fontFamily: 'var(--font-chicago)', fontSize: '11px' }}>
                Level:
              </label>
              <select
                value={logLevel}
                onChange={(e) => setLogLevel(e.target.value as LogLevel | 'all')}
                style={{
                  fontFamily: 'var(--font-chicago)',
                  fontSize: '11px',
                  padding: '2px 4px',
                  border: '1px solid black',
                }}
              >
                <option value="all">All</option>
                <option value="debug">Debug</option>
                <option value="info">Info</option>
                <option value="warn">Warn</option>
                <option value="error">Error</option>
              </select>

              <label style={{ fontFamily: 'var(--font-chicago)', fontSize: '11px', marginLeft: '16px' }}>
                Search:
              </label>
              <TextInput
                value={logSearch}
                onChange={(e) => setLogSearch(e.target.value)}
                placeholder="Filter logs..."
                style={{ flex: 1 }}
              />
            </div>

            <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
              <Checkbox
                label="Auto-refresh"
                checked={autoRefresh}
                onChange={setAutoRefresh}
              />
              <Button onClick={() => setIsPaused(!isPaused)}>
                {isPaused ? 'Resume' : 'Pause'}
              </Button>
              <Button onClick={() => refetchLogs()}>
                Refresh Now
              </Button>
            </div>
          </div>

          {/* Log Entries */}
          <div style={{
            border: '1px solid black',
            background: 'white',
            padding: '8px',
            maxHeight: '400px',
            overflowY: 'auto',
          }}>
            {logsLoading && !logs ? (
              <div>Loading logs...</div>
            ) : logs && logs.entries.length > 0 ? (
              <>
                {logs.entries.map((entry, idx) => (
                  <div
                    key={`${entry.timestamp}-${idx}`}
                    style={{
                      marginBottom: '8px',
                      padding: '6px',
                      border: '1px solid #ccc',
                      fontFamily: 'monospace',
                      fontSize: '10px',
                    }}
                  >
                    <div style={{ marginBottom: '2px', color: '#666' }}>
                      {new Date(entry.timestamp).toLocaleString()} <LogLevelBadge level={entry.level} /> {entry.target}
                    </div>
                    <div style={{ wordBreak: 'break-word' }}>
                      {/* Render message as plain text only (XSS prevention) */}
                      {entry.message}
                    </div>
                  </div>
                ))}

                {hasMore && logs.cursor && (
                  <div style={{ marginTop: '8px', textAlign: 'center' }}>
                    <Button onClick={() => setLogCursor(logs.cursor || undefined)}>
                      Load More
                    </Button>
                  </div>
                )}
              </>
            ) : (
              <div style={{ textAlign: 'center', padding: '20px', color: '#666' }}>
                No logs to display
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
};

export default SystemMonitorApp;
