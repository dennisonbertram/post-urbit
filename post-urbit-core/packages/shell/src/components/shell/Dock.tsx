import React from "react";
import { useWindows } from "../../context/WindowContext";

/**
 * Dock component - taskbar showing all open windows at the bottom of the screen
 *
 * Features:
 * - Shows all windows (minimized and visible)
 * - Click to focus visible windows or restore minimized ones
 * - Visual distinction for focused, minimized, and regular windows
 * - Truncates long titles to prevent overflow
 * - System 7 aesthetic with light gray background
 *
 * Usage:
 * <Dock />
 */

const Dock: React.FC = () => {
  const { windows, focusWindow, restoreWindow } = useWindows();

  // Handle click on a dock item
  const handleDockItemClick = (windowId: string, isMinimized: boolean) => {
    if (isMinimized) {
      restoreWindow(windowId);
    } else {
      focusWindow(windowId);
    }
  };

  // Truncate long titles
  const truncateTitle = (title: string, maxLength: number = 15): string => {
    if (title.length <= maxLength) return title;
    return title.substring(0, maxLength - 1) + "…";
  };

  // Get the focused window (highest zIndex)
  const focusedWindowId = windows.length > 0
    ? windows.reduce((prev, current) => (prev.zIndex > current.zIndex ? prev : current)).id
    : null;

  if (windows.length === 0) {
    return null; // Don't show dock if no windows
  }

  return (
    <div
      style={{
        position: "fixed",
        bottom: 0,
        left: 0,
        right: 0,
        height: "32px",
        backgroundColor: "#c0c0c0",
        borderTop: "2px solid #fff",
        display: "flex",
        alignItems: "center",
        padding: "0 4px",
        gap: "4px",
        flexWrap: "wrap",
        overflowX: "auto",
        overflowY: "hidden",
        zIndex: 9999,
        fontFamily: "Chicago, 'Courier New', monospace",
        fontSize: "12px",
      }}
      role="toolbar"
      aria-label="Window taskbar"
    >
      {windows.map((window) => {
        const isFocused = window.id === focusedWindowId && !window.isMinimized;
        const isMinimized = window.isMinimized;

        return (
          <button
            key={window.id}
            onClick={() => handleDockItemClick(window.id, isMinimized)}
            style={{
              minWidth: "80px",
              maxWidth: "140px",
              height: "24px",
              padding: "0 8px",
              backgroundColor: isFocused ? "#000" : "#fff",
              color: isFocused ? "#fff" : "#000",
              border: "1px solid #000",
              borderRadius: "0",
              cursor: "pointer",
              fontSize: "12px",
              fontFamily: "inherit",
              fontStyle: isMinimized ? "italic" : "normal",
              opacity: isMinimized ? 0.7 : 1,
              textAlign: "center",
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
              boxShadow: isFocused
                ? "inset 1px 1px 0 #000, inset -1px -1px 0 #000"
                : "inset 1px 1px 0 #dfdfdf, inset -1px -1px 0 #808080",
            }}
            title={window.title}
            aria-label={`${window.title}${isMinimized ? " (minimized)" : ""}${isFocused ? " (focused)" : ""}`}
            aria-pressed={isFocused}
          >
            {truncateTitle(window.title)}
          </button>
        );
      })}
    </div>
  );
};

export default Dock;
