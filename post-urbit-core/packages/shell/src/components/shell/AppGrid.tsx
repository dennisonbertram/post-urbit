import React from "react";
import Icon from "../system7/Icon";

const apps = [
  { name: "Files", icon: "folder" },
  { name: "Mail", icon: "mail" },
  { name: "Notes", icon: "notes" },
  { name: "Browser", icon: "browser" },
  { name: "Settings", icon: "settings" },
  { name: "Trash", icon: "trash" },
  { name: "Activity", icon: "activity" },
  { name: "Install App", icon: "install" }
] as const;

const AppGrid = () => {
  return (
    <div className="s7-app-grid">
      {apps.map((app) => (
        <div className="s7-app-tile" key={app.name}>
          <div className="s7-app-icon">
            <Icon name={app.icon} />
          </div>
          <div className="s7-app-label">{app.name}</div>
          <div className="s7-app-status">Running</div>
        </div>
      ))}
    </div>
  );
};

export default AppGrid;
