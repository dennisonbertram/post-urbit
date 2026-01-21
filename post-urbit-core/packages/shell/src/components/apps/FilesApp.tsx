import React from 'react';

/**
 * FilesApp - File browser
 *
 * Shows the app storage contents from the backend.
 * Currently not implemented - requires backend file storage API.
 */
const FilesApp = () => {
  return (
    <div style={{ padding: '12px' }}>
      <div style={{
        border: '1px solid black',
        background: 'white',
        padding: '16px',
        textAlign: 'center',
      }}>
        <p style={{
          margin: '0 0 8px 0',
          fontFamily: 'var(--font-chicago)',
          fontWeight: 'bold',
        }}>
          Files
        </p>
        <p style={{
          margin: 0,
          color: '#666',
          fontSize: '11px',
        }}>
          File browsing is not yet available.
        </p>
        <p style={{
          margin: '8px 0 0 0',
          color: '#888',
          fontSize: '10px',
        }}>
          This feature requires the storage API to be implemented.
        </p>
      </div>
    </div>
  );
};

export default FilesApp;
