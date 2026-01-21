import React from "react";

type IconName =
  | "folder"
  | "mail"
  | "notes"
  | "browser"
  | "settings"
  | "trash"
  | "activity"
  | "install";

type IconProps = {
  name: IconName;
  selected?: boolean;
  size?: number;
};

const Icon = ({ name, selected = false, size = 32 }: IconProps) => {
  return (
    <svg
      className={`s7-icon s7-icon-${name} ${selected ? 's7-icon-selected' : ''}`}
      width={size}
      height={size}
      viewBox="0 0 32 32"
      role="img"
      aria-label={name}
    >
      <rect
        x="1"
        y="1"
        width="30"
        height="30"
        fill={selected ? "#000000" : "#FFFFFF"}
        stroke="#000000"
      />
      {name === "folder" && (
        <>
          <rect x="3" y="10" width="26" height="17" fill={selected ? "#FFFFFF" : "#6699CC"} />
          <rect x="3" y="7" width="12" height="5" fill={selected ? "#DDDDDD" : "#99BBDD"} />
          <rect x="3" y="10" width="26" height="17" fill="none" stroke={selected ? "#FFFFFF" : "#000000"} />
        </>
      )}
      {name === "mail" && (
        <>
          <rect x="5" y="9" width="22" height="15" fill={selected ? "#000000" : "#FFFFFF"} stroke={selected ? "#FFFFFF" : "#000000"} />
          <path d="M5 9 L16 18 L27 9" fill="none" stroke={selected ? "#FFFFFF" : "#000000"} />
        </>
      )}
      {name === "notes" && (
        <>
          <rect x="7" y="6" width="18" height="20" fill={selected ? "#000000" : "#FFFFFF"} stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="9" y1="11" x2="23" y2="11" stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="9" y1="15" x2="23" y2="15" stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="9" y1="19" x2="21" y2="19" stroke={selected ? "#FFFFFF" : "#000000"} />
        </>
      )}
      {name === "browser" && (
        <>
          <rect x="6" y="7" width="20" height="18" fill={selected ? "#000000" : "#FFFFFF"} stroke={selected ? "#FFFFFF" : "#000000"} />
          <rect x="6" y="7" width="20" height="4" fill={selected ? "#333333" : "#EEEEEE"} />
          <circle cx="10" cy="9" r="1" fill={selected ? "#FFFFFF" : "#000000"} />
          <line x1="9" y1="16" x2="23" y2="16" stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="9" y1="20" x2="19" y2="20" stroke={selected ? "#FFFFFF" : "#000000"} />
        </>
      )}
      {name === "settings" && (
        <>
          <circle cx="16" cy="16" r="7" fill={selected ? "#666666" : "#DDDDDD"} stroke={selected ? "#FFFFFF" : "#000000"} />
          <circle cx="16" cy="16" r="3" fill={selected ? "#000000" : "#FFFFFF"} stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="16" y1="5" x2="16" y2="9" stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="16" y1="23" x2="16" y2="27" stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="5" y1="16" x2="9" y2="16" stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="23" y1="16" x2="27" y2="16" stroke={selected ? "#FFFFFF" : "#000000"} />
        </>
      )}
      {name === "trash" && (
        <>
          <rect x="10" y="9" width="12" height="16" fill={selected ? "#000000" : "#FFFFFF"} stroke={selected ? "#FFFFFF" : "#000000"} />
          <rect x="9" y="7" width="14" height="3" fill={selected ? "#333333" : "#EEEEEE"} stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="13" y1="12" x2="13" y2="22" stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="19" y1="12" x2="19" y2="22" stroke={selected ? "#FFFFFF" : "#000000"} />
        </>
      )}
      {name === "activity" && (
        <>
          <rect x="6" y="9" width="20" height="15" fill={selected ? "#000000" : "#FFFFFF"} stroke={selected ? "#FFFFFF" : "#000000"} />
          <polyline
            points="8,20 12,16 16,18 20,12 24,15"
            fill="none"
            stroke={selected ? "#FFFFFF" : "#000000"}
          />
        </>
      )}
      {name === "install" && (
        <>
          <rect x="8" y="8" width="16" height="16" fill={selected ? "#000000" : "#FFFFFF"} stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="16" y1="11" x2="16" y2="21" stroke={selected ? "#FFFFFF" : "#000000"} />
          <line x1="11" y1="16" x2="21" y2="16" stroke={selected ? "#FFFFFF" : "#000000"} />
        </>
      )}
    </svg>
  );
};

export default Icon;
