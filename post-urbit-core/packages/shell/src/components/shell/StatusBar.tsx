import React from "react";
import { useHealth } from "../../api/hooks";

const formatBytes = (bytes: number): string => {
  const mb = bytes / (1024 * 1024);
  return `${mb.toFixed(0)}MB`;
};

const StatusBar = () => {
  const { data: health, error } = useHealth(30000); // Poll every 30s

  if (error) {
    return (
      <div className="s7-status-bar">
        <div className="s7-status-left">
          <span>Backend unreachable</span>
        </div>
        <div className="s7-status-right">
          <span style={{ color: '#cc0000' }}>Disconnected</span>
        </div>
      </div>
    );
  }

  if (!health) {
    return (
      <div className="s7-status-bar">
        <div className="s7-status-left">
          <span>Loading...</span>
        </div>
        <div className="s7-status-right">
          <span>Connecting...</span>
        </div>
      </div>
    );
  }

  const appsInstalled = health.checks.apps.installed;
  const appsRunning = health.checks.apps.running;
  const diskUsed = formatBytes(health.checks.storage.disk_used_bytes);
  const diskFree = formatBytes(health.checks.storage.disk_free_bytes);
  const connections = health.checks.transport.connections;
  const isHealthy = health.status === 'healthy';

  return (
    <div className="s7-status-bar">
      <div className="s7-status-left">
        <span>{appsInstalled} apps ({appsRunning} running)</span>
        <span>Storage: {diskUsed} used / {diskFree} free</span>
        <span>{connections} connections</span>
      </div>
      <div className="s7-status-right">
        <span style={{ color: isHealthy ? '#00aa00' : '#cc0000' }}>
          {isHealthy ? 'Healthy' : 'Unhealthy'}
        </span>
        <span>v{health.version}</span>
      </div>
    </div>
  );
};

export default StatusBar;
