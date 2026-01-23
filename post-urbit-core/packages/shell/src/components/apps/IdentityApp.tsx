import React, { useState, useEffect } from 'react';
import { useIdentity } from '../../api/hooks';
import { apiClient, ApiClientError } from '../../api/client';
import TextInput from '../system7/TextInput';
import Button from '../system7/Button';
import { useAlert } from '../../context/AlertContext';

/**
 * IdentityApp - View and edit your Post-Urbit identity
 *
 * Features:
 * - View full IID and key fingerprints
 * - Edit display name and bio
 * - View connection endpoints
 */
const IdentityApp = () => {
  const { data: identity, isInitialLoading, error, refetch } = useIdentity();
  const { showAlert } = useAlert();

  const [displayName, setDisplayName] = useState('');
  const [bio, setBio] = useState('');
  const [saving, setSaving] = useState(false);
  const [hasChanges, setHasChanges] = useState(false);

  // Initialize form values when identity loads
  useEffect(() => {
    if (identity) {
      setDisplayName(identity.profile?.display_name || '');
      setBio(identity.profile?.bio || '');
    }
  }, [identity]);

  // Track changes
  useEffect(() => {
    if (identity) {
      const nameChanged = displayName !== (identity.profile?.display_name || '');
      const bioChanged = bio !== (identity.profile?.bio || '');
      setHasChanges(nameChanged || bioChanged);
    }
  }, [displayName, bio, identity]);

  const handleSave = async () => {
    setSaving(true);
    try {
      await apiClient.patch('/admin/v1/identity/profile', {
        display_name: displayName,
        bio: bio || undefined,
      });
      await refetch();
      showAlert('note', 'Profile Updated', 'Your profile has been saved.');
      setHasChanges(false);
    } catch (err) {
      const message = err instanceof ApiClientError ? err.message : 'Failed to save profile';
      showAlert('stop', 'Error', message);
    } finally {
      setSaving(false);
    }
  };

  const handleReset = () => {
    if (identity) {
      setDisplayName(identity.profile?.display_name || '');
      setBio(identity.profile?.bio || '');
    }
  };

  const copyToClipboard = (text: string, label: string) => {
    navigator.clipboard.writeText(text).then(() => {
      showAlert('note', 'Copied', `${label} copied to clipboard.`);
    }).catch(() => {
      showAlert('stop', 'Error', 'Failed to copy to clipboard.');
    });
  };

  if (isInitialLoading) {
    return (
      <div style={{ padding: '12px' }}>
        Loading identity...
      </div>
    );
  }

  if (error) {
    return (
      <div style={{ padding: '12px', color: 'red' }}>
        Error loading identity: {error.message}
      </div>
    );
  }

  if (!identity) {
    return (
      <div style={{ padding: '12px' }}>
        No identity found.
      </div>
    );
  }

  return (
    <div style={{ padding: '8px', minWidth: '420px', maxWidth: '500px' }}>
      {/* IID Section */}
      <div style={{ marginBottom: '16px' }}>
        <h3 style={{
          margin: '0 0 8px 0',
          fontFamily: 'var(--font-chicago)',
          fontSize: '12px',
        }}>
          Your Identity
        </h3>
        <div style={{
          border: '1px solid black',
          background: 'white',
          padding: '8px',
        }}>
          <div style={{ marginBottom: '8px' }}>
            <label style={{ display: 'block', marginBottom: '2px', fontSize: '10px', fontWeight: 'bold' }}>
              IID (Identity ID):
            </label>
            <div style={{
              display: 'flex',
              alignItems: 'center',
              gap: '8px',
            }}>
              <code style={{
                flex: 1,
                fontSize: '10px',
                background: '#f0f0f0',
                padding: '4px 6px',
                border: '1px inset #ccc',
                fontFamily: 'Monaco, monospace',
                wordBreak: 'break-all',
              }}>
                {identity.iid}
              </code>
              <Button onClick={() => copyToClipboard(identity.iid, 'IID')}>
                Copy
              </Button>
            </div>
          </div>
          <div style={{ fontSize: '10px', color: '#666' }}>
            Created: {new Date(identity.created_at).toLocaleDateString()}
          </div>
        </div>
      </div>

      {/* Profile Section */}
      <div style={{ marginBottom: '16px' }}>
        <h3 style={{
          margin: '0 0 8px 0',
          fontFamily: 'var(--font-chicago)',
          fontSize: '12px',
        }}>
          Profile
        </h3>
        <div style={{
          border: '1px solid black',
          background: 'white',
          padding: '8px',
        }}>
          <div style={{ marginBottom: '8px' }}>
            <label style={{ display: 'block', marginBottom: '4px', fontSize: '11px' }}>
              Display Name:
            </label>
            <TextInput
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="Enter your display name"
            />
          </div>
          <div style={{ marginBottom: '4px' }}>
            <label style={{ display: 'block', marginBottom: '4px', fontSize: '11px' }}>
              Bio:
            </label>
            <textarea
              value={bio}
              onChange={(e) => setBio(e.target.value)}
              placeholder="Tell others about yourself..."
              style={{
                width: '100%',
                height: '60px',
                fontFamily: 'var(--font-chicago)',
                fontSize: '12px',
                padding: '4px',
                border: '2px inset #888',
                resize: 'vertical',
                boxSizing: 'border-box',
              }}
            />
          </div>
        </div>
      </div>

      {/* Keys Section */}
      <div style={{ marginBottom: '16px' }}>
        <h3 style={{
          margin: '0 0 8px 0',
          fontFamily: 'var(--font-chicago)',
          fontSize: '12px',
        }}>
          Keys
        </h3>
        <div style={{
          border: '1px solid black',
          background: 'white',
          padding: '8px',
          fontSize: '10px',
        }}>
          <div style={{ marginBottom: '4px' }}>
            <strong>Genesis Key:</strong>{' '}
            <code style={{ fontFamily: 'Monaco, monospace' }}>
              {identity.genesis_key_fingerprint.slice(0, 16)}...
            </code>
          </div>
          <div style={{ marginBottom: '4px' }}>
            <strong>Signing Key:</strong>{' '}
            <code style={{ fontFamily: 'Monaco, monospace' }}>
              {identity.current_signing_key_fingerprint.slice(0, 16)}...
            </code>
          </div>
          <div style={{ marginBottom: '4px' }}>
            <strong>Encryption Key:</strong>{' '}
            <code style={{ fontFamily: 'Monaco, monospace' }}>
              {identity.current_encryption_key_fingerprint.slice(0, 16)}...
            </code>
          </div>
          {identity.last_key_rotation && (
            <div style={{ color: '#666' }}>
              Last rotation: {new Date(identity.last_key_rotation).toLocaleDateString()}
            </div>
          )}
        </div>
      </div>

      {/* Endpoints Section */}
      {identity.endpoints.length > 0 && (
        <div style={{ marginBottom: '16px' }}>
          <h3 style={{
            margin: '0 0 8px 0',
            fontFamily: 'var(--font-chicago)',
            fontSize: '12px',
          }}>
            Endpoints
          </h3>
          <div style={{
            border: '1px solid black',
            background: 'white',
            padding: '8px',
            fontSize: '10px',
          }}>
            {identity.endpoints.map((endpoint, i) => (
              <div key={i} style={{ marginBottom: i < identity.endpoints.length - 1 ? '4px' : 0 }}>
                <strong>{endpoint.type}:</strong>{' '}
                <code style={{ fontFamily: 'Monaco, monospace' }}>{endpoint.url}</code>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Action Buttons */}
      <div style={{
        display: 'flex',
        gap: '8px',
        justifyContent: 'flex-end',
      }}>
        <Button onClick={handleReset} disabled={!hasChanges}>
          Reset
        </Button>
        <Button isDefault onClick={handleSave} disabled={!hasChanges || saving}>
          {saving ? 'Saving...' : 'Save'}
        </Button>
      </div>
    </div>
  );
};

export default IdentityApp;
