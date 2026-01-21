import React from "react";

const MenuBar = () => {
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
        <span className="s7-menu-status">Network: OK</span>
        <span className="s7-menu-clock">3:45 PM</span>
      </div>
    </div>
  );
};

export default MenuBar;
