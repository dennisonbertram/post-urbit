import React, { useMemo } from "react";
import Window from "../system7/Window";
import { useWindows } from "../../context/WindowContext";

/**
 * WindowManager renders all open windows and manages their z-order.
 * It connects the Window components to the WindowContext state.
 */
const WindowManager = () => {
  const {
    windows,
    closeWindow,
    focusWindow,
    moveWindow,
    resizeWindow,
    maximizeWindow,
  } = useWindows();

  // Get the highest z-index to determine active window
  const maxZIndex = useMemo(() => {
    return windows.reduce((max, w) => Math.max(max, w.zIndex), 0);
  }, [windows]);

  // Filter out minimized windows
  const visibleWindows = useMemo(() => {
    return windows.filter(w => !w.isMinimized);
  }, [windows]);

  return (
    <>
      {visibleWindows.map(window => (
        <Window
          key={window.id}
          id={window.id}
          title={window.title}
          x={window.x}
          y={window.y}
          width={window.width}
          height={window.height}
          zIndex={window.zIndex}
          isMaximized={window.isMaximized}
          active={window.zIndex === maxZIndex}
          onClose={() => closeWindow(window.id)}
          onFocus={() => focusWindow(window.id)}
          onMove={(x, y) => moveWindow(window.id, x, y)}
          onResize={(width, height) => resizeWindow(window.id, width, height)}
          onMaximize={() => maximizeWindow(window.id)}
        >
          {window.content}
        </Window>
      ))}
    </>
  );
};

export default WindowManager;
