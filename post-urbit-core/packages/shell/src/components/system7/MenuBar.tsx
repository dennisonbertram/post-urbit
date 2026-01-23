import React, { useState, useEffect } from "react";
import { useIdentity } from "../../api/hooks";
import { useWindows } from "../../context/WindowContext";
import IdentityApp from "../apps/IdentityApp";

interface MenuBarProps {
  onLogout?: () => void;
}

const MenuBar: React.FC<MenuBarProps> = ({ onLogout }) => {
  const { data: identity } = useIdentity();
  const { openWindow, windows, focusWindow, cascadeWindows, tileWindows, bringAllToFront } = useWindows();
  const [currentTime, setCurrentTime] = useState(new Date());
  const [windowMenuOpen, setWindowMenuOpen] = useState(false);

  // Update clock every second
  useEffect(() => {
    const interval = setInterval(() => {
      setCurrentTime(new Date());
    }, 1000);
    return () => clearInterval(interval);
  }, []);

  // Close window menu when clicking elsewhere
  useEffect(() => {
    if (!windowMenuOpen) return;

    const handleClickOutside = () => {
      setWindowMenuOpen(false);
    };

    // Small delay to prevent immediate close on open
    setTimeout(() => {
      document.addEventListener('click', handleClickOutside);
    }, 0);

    return () => {
      document.removeEventListener('click', handleClickOutside);
    };
  }, [windowMenuOpen]);

  const formatTime = (date: Date): string => {
    return date.toLocaleTimeString('en-US', {
      hour: 'numeric',
      minute: '2-digit',
      hour12: true,
    });
  };

  const shortIid = identity?.iid
    ? `${identity.iid.slice(0, 8)}...`
    : 'Loading...';

  const displayName = identity?.profile?.display_name || shortIid;

  const handleIdentityClick = () => {
    openWindow('identity', 'Identity', <IdentityApp />, {
      width: 450,
      height: 520,
    });
  };

  const handleWindowMenuClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setWindowMenuOpen(!windowMenuOpen);
  };

  const handleWindowAction = (action: () => void) => {
    action();
    setWindowMenuOpen(false);
  };

  const handleWindowSelect = (windowId: string) => {
    focusWindow(windowId);
    setWindowMenuOpen(false);
  };

  // Find the focused window (highest zIndex)
  const focusedWindowId = windows.reduce((maxId, w) => {
    const maxWindow = windows.find(win => win.id === maxId);
    return (maxWindow && w.zIndex > maxWindow.zIndex) ? w.id : maxId;
  }, windows[0]?.id);

  // Get visible (non-minimized) windows for the menu
  const visibleWindows = windows.filter(w => !w.isMinimized);

  return (
    <div className="s7-menu-bar">
      <div className="s7-menu-left">
        <div className="s7-menu-logo">PU</div>
        <button className="s7-menu-item">File</button>
        <button className="s7-menu-item">Edit</button>
        <button className="s7-menu-item">View</button>
        <button className="s7-menu-item">Apps</button>
        <div style={{ position: 'relative', display: 'inline-block' }}>
          <button
            className="s7-menu-item"
            onClick={handleWindowMenuClick}
            style={{ backgroundColor: windowMenuOpen ? '#000' : 'transparent', color: windowMenuOpen ? '#fff' : '#000' }}
          >
            Window
          </button>
          {windowMenuOpen && (
            <div
              style={{
                position: 'absolute',
                top: '100%',
                left: 0,
                backgroundColor: '#fff',
                border: '2px solid #000',
                boxShadow: '2px 2px 0 rgba(0,0,0,0.5)',
                minWidth: '200px',
                zIndex: 10000,
              }}
              onClick={(e) => e.stopPropagation()}
            >
              <div
                style={{
                  padding: '4px 12px',
                  cursor: 'pointer',
                  fontSize: '12px',
                  fontFamily: 'Chicago, "Helvetica Neue", sans-serif',
                }}
                onMouseEnter={(e) => e.currentTarget.style.backgroundColor = '#000'}
                onMouseLeave={(e) => e.currentTarget.style.backgroundColor = 'transparent'}
                onMouseOver={(e) => e.currentTarget.style.color = '#fff'}
                onMouseOut={(e) => e.currentTarget.style.color = '#000'}
                onClick={() => handleWindowAction(cascadeWindows)}
              >
                Cascade Windows
              </div>
              <div
                style={{
                  padding: '4px 12px',
                  cursor: 'pointer',
                  fontSize: '12px',
                  fontFamily: 'Chicago, "Helvetica Neue", sans-serif',
                }}
                onMouseEnter={(e) => e.currentTarget.style.backgroundColor = '#000'}
                onMouseLeave={(e) => e.currentTarget.style.backgroundColor = 'transparent'}
                onMouseOver={(e) => e.currentTarget.style.color = '#fff'}
                onMouseOut={(e) => e.currentTarget.style.color = '#000'}
                onClick={() => handleWindowAction(tileWindows)}
              >
                Tile Windows
              </div>
              <div
                style={{
                  padding: '4px 12px',
                  cursor: 'pointer',
                  fontSize: '12px',
                  fontFamily: 'Chicago, "Helvetica Neue", sans-serif',
                }}
                onMouseEnter={(e) => e.currentTarget.style.backgroundColor = '#000'}
                onMouseLeave={(e) => e.currentTarget.style.backgroundColor = 'transparent'}
                onMouseOver={(e) => e.currentTarget.style.color = '#fff'}
                onMouseOut={(e) => e.currentTarget.style.color = '#000'}
                onClick={() => handleWindowAction(bringAllToFront)}
              >
                Bring All to Front
              </div>
              {visibleWindows.length > 0 && (
                <>
                  <div
                    style={{
                      height: '1px',
                      backgroundColor: '#000',
                      margin: '4px 8px',
                    }}
                  />
                  {visibleWindows.map((window) => (
                    <div
                      key={window.id}
                      style={{
                        padding: '4px 12px 4px 24px',
                        cursor: 'pointer',
                        fontSize: '12px',
                        fontFamily: 'Chicago, "Helvetica Neue", sans-serif',
                        position: 'relative',
                      }}
                      onMouseEnter={(e) => e.currentTarget.style.backgroundColor = '#000'}
                      onMouseLeave={(e) => e.currentTarget.style.backgroundColor = 'transparent'}
                      onMouseOver={(e) => e.currentTarget.style.color = '#fff'}
                      onMouseOut={(e) => e.currentTarget.style.color = '#000'}
                      onClick={() => handleWindowSelect(window.id)}
                    >
                      {window.id === focusedWindowId && (
                        <span style={{ position: 'absolute', left: '8px' }}>✓</span>
                      )}
                      {window.title.length > 25 ? window.title.substring(0, 25) + '...' : window.title}
                    </div>
                  ))}
                </>
              )}
            </div>
          )}
        </div>
        <button className="s7-menu-item">Help</button>
      </div>
      <div className="s7-menu-right">
        <span
          className="s7-menu-status"
          title={identity?.iid ? `Click to view identity: ${identity.iid}` : 'Loading...'}
          onClick={handleIdentityClick}
          style={{ cursor: 'pointer' }}
        >
          {displayName}
        </span>
        {onLogout && (
          <button
            className="s7-menu-item"
            onClick={onLogout}
            style={{ padding: '0 8px', fontSize: '12px' }}
          >
            Logout
          </button>
        )}
        <span className="s7-menu-clock">{formatTime(currentTime)}</span>
      </div>
    </div>
  );
};

export default MenuBar;
