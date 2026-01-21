import React from 'react';

/**
 * AppStoreApp - Marketplace for discovering and installing apps
 *
 * Will show available apps from configured repositories.
 * Currently not implemented - requires app repository API.
 */
const AppStoreApp = () => {
  return (
    <div style={{ padding: '12px', minWidth: '350px' }}>
      <div style={{
        marginBottom: '12px',
        paddingBottom: '8px',
        borderBottom: '1px solid black',
      }}>
        <h3 style={{
          margin: '0 0 8px 0',
          fontFamily: 'var(--font-chicago)',
          fontSize: '12px',
        }}>
          App Store
        </h3>
        <p style={{ margin: 0, fontSize: '10px', color: '#666' }}>
          Discover and install apps for your Post-Urbit node
        </p>
      </div>

      <div style={{
        border: '1px solid black',
        background: 'white',
        padding: '16px',
        textAlign: 'center',
        minHeight: '150px',
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
      }}>
        <p style={{
          margin: '0 0 8px 0',
          color: '#666',
          fontSize: '11px',
        }}>
          No app repositories configured.
        </p>
        <p style={{
          margin: 0,
          color: '#888',
          fontSize: '10px',
        }}>
          Add a repository in Settings to browse available apps.
        </p>
      </div>
    </div>
  );
};

export default AppStoreApp;
