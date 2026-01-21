import React from "react";

const StatusBar = () => {
  return (
    <div className="s7-status-bar">
      <div className="s7-status-left">
        <span>8 apps installed</span>
        <span>Memory: 245MB / 512MB</span>
      </div>
      <div className="s7-status-right">
        <span>Connected</span>
        <span>Secure</span>
      </div>
    </div>
  );
};

export default StatusBar;
