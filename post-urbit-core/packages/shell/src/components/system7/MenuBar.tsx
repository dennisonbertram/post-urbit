import React, { useState, useEffect } from "react";
import { useIdentity } from "../../api/hooks";

interface MenuBarProps {
  onLogout?: () => void;
}

const MenuBar: React.FC<MenuBarProps> = ({ onLogout }) => {
  const { data: identity } = useIdentity();
  const [currentTime, setCurrentTime] = useState(new Date());

  // Update clock every second
  useEffect(() => {
    const interval = setInterval(() => {
      setCurrentTime(new Date());
    }, 1000);
    return () => clearInterval(interval);
  }, []);

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

  return (
    <div className="s7-menu-bar">
      <div className="s7-menu-left">
        <div className="s7-menu-logo">PU</div>
        <button className="s7-menu-item">File</button>
        <button className="s7-menu-item">Edit</button>
        <button className="s7-menu-item">View</button>
        <button className="s7-menu-item">Apps</button>
        <button className="s7-menu-item">Window</button>
        <button className="s7-menu-item">Help</button>
      </div>
      <div className="s7-menu-right">
        <span className="s7-menu-status" title={identity?.iid}>
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
