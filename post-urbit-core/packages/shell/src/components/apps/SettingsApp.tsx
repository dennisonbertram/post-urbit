import React from 'react';
import { useNodeStatus, useIdentity } from '../../api/hooks';
import TextInput from '../system7/TextInput';
import Checkbox from '../system7/Checkbox';
import Button from '../system7/Button';

/**
 * SettingsApp - Node configuration and settings
 * Shows real data from the API
 *
 * Features:
 * - Node status and uptime
 * - Identity information
 * - Network settings
 * - Storage info
 */
const SettingsApp = () => {
  const { data: status, isInitialLoading: statusLoading } = useNodeStatus();
  const { data: identity, isInitialLoading: identityLoading } = useIdentity();

  if (statusLoading || identityLoading) {
    return (
      <div style={{ padding: '12px' }}>
        Loading settings...
      </div>
    );
  }

  return (
    <div style={{ padding: '8px', minWidth: '400px' }}>
      {/* Node Status Section */}
      <div style={{ marginBottom: '16px' }}>
        <h3 style={{
          margin: '0 0 8px 0',
          fontFamily: 'var(--font-chicago)',
          fontSize: '12px',
        }}>
          Node Status
        </h3>
        <div style={{
          border: '1px solid black',
          background: 'white',
          padding: '8px',
        }}>
          {status && (
            <>
              <div style={{ marginBottom: '4px' }}>
                <strong>Version:</strong> {status.version}
              </div>
              <div style={{ marginBottom: '4px' }}>
                <strong>Uptime:</strong> {Math.floor(status.uptime_seconds / 3600)}h {Math.floor((status.uptime_seconds % 3600) / 60)}m
              </div>
              <div style={{ marginBottom: '4px' }}>
                <strong>Status:</strong>{' '}
                <span style={{
                  color: status.status === 'healthy' ? 'green' : 'red',
                  fontWeight: 'bold',
                }}>
                  {status.status.toUpperCase()}
                </span>
              </div>
            </>
          )}
        </div>
      </div>

      {/* Identity Section */}
      <div style={{ marginBottom: '16px' }}>
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
          {identity && (
            <>
              <div style={{ marginBottom: '8px' }}>
                <label style={{ display: 'block', marginBottom: '4px' }}>
                  Display Name:
                </label>
                <TextInput
                  value={identity.profile.display_name}
                  onChange={() => {}}
                  placeholder="Your name"
                />
              </div>
              <div style={{ marginBottom: '4px', fontSize: '10px' }}>
                <strong>IID:</strong> {identity.iid.slice(0, 16)}...
              </div>
            </>
          )}
        </div>
      </div>

      {/* Network Section */}
      {status && (
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
              <strong>Connections:</strong> {status.network.connections_active}
            </div>
            <div style={{ marginBottom: '4px' }}>
              <strong>Relays:</strong> {status.network.relays_connected}
            </div>
            <div style={{ marginBottom: '8px' }}>
              <strong>Sent:</strong> {(status.network.bytes_sent / 1024 / 1024).toFixed(2)} MB
              {' | '}
              <strong>Received:</strong> {(status.network.bytes_received / 1024 / 1024).toFixed(2)} MB
            </div>
            <Checkbox
              label="Enable port forwarding"
              checked={false}
              onChange={() => {}}
            />
          </div>
        </div>
      )}

      {/* Storage Section */}
      {status && (
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
            <div style={{ marginBottom: '4px' }}>
              <strong>Used:</strong> {(status.storage.data_used_bytes / 1024 / 1024).toFixed(2)} MB
            </div>
            <div style={{ marginBottom: '4px' }}>
              <strong>Free:</strong> {(status.storage.data_free_bytes / 1024 / 1024).toFixed(2)} MB
            </div>
            <div style={{ marginBottom: '4px' }}>
              <strong>Messages:</strong> {status.storage.messages_count}
            </div>
            <div>
              <strong>Documents:</strong> {status.storage.documents_count}
            </div>
          </div>
        </div>
      )}

      {/* Action Buttons */}
      <div style={{
        display: 'flex',
        gap: '8px',
        justifyContent: 'flex-end',
      }}>
        <Button>Cancel</Button>
        <Button isDefault>Save</Button>
      </div>
    </div>
  );
};

export default SettingsApp;
