import React, { useRef, useCallback, useEffect, useState } from "react";

type WindowProps = {
  id: string;
  title: string;
  active?: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  zIndex: number;
  isMaximized?: boolean;
  children: React.ReactNode;
  onClose?: () => void;
  onFocus?: () => void;
  onMove?: (x: number, y: number) => void;
  onResize?: (width: number, height: number) => void;
  onMaximize?: () => void;
  onMinimize?: () => void;
};

const Window = ({
  id,
  title,
  active = true,
  x,
  y,
  width,
  height,
  zIndex,
  isMaximized = false,
  children,
  onClose,
  onFocus,
  onMove,
  onResize,
  onMaximize,
  onMinimize,
}: WindowProps) => {
  const windowRef = useRef<HTMLDivElement>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [isResizing, setIsResizing] = useState(false);
  const dragStartPos = useRef({ x: 0, y: 0, windowX: 0, windowY: 0 });
  const resizeStartPos = useRef({ x: 0, y: 0, width: 0, height: 0 });

  // Handle titlebar mousedown for dragging
  const handleTitlebarMouseDown = useCallback(
    (e: React.MouseEvent) => {
      // Ignore if clicking on window controls
      if ((e.target as HTMLElement).classList.contains("s7-window-control")) {
        return;
      }

      if (isMaximized) return; // Can't drag maximized windows

      e.preventDefault();
      setIsDragging(true);
      dragStartPos.current = {
        x: e.clientX,
        y: e.clientY,
        windowX: x,
        windowY: y,
      };

      onFocus?.();
    },
    [x, y, isMaximized, onFocus]
  );

  // Handle resize handle mousedown
  const handleResizeMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (isMaximized) return; // Can't resize maximized windows

      e.preventDefault();
      e.stopPropagation();
      setIsResizing(true);
      resizeStartPos.current = {
        x: e.clientX,
        y: e.clientY,
        width,
        height,
      };

      onFocus?.();
    },
    [width, height, isMaximized, onFocus]
  );

  // Handle window click to focus
  const handleWindowMouseDown = useCallback(() => {
    onFocus?.();
  }, [onFocus]);

  // Handle close button
  const handleClose = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      onClose?.();
    },
    [onClose]
  );

  // Handle maximize button
  const handleMaximize = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      onMaximize?.();
    },
    [onMaximize]
  );

  // Handle minimize button
  const handleMinimize = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      onMinimize?.();
    },
    [onMinimize]
  );

  // Handle mouse move for dragging and resizing
  useEffect(() => {
    if (!isDragging && !isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      if (isDragging) {
        const deltaX = e.clientX - dragStartPos.current.x;
        const deltaY = e.clientY - dragStartPos.current.y;
        const newX = dragStartPos.current.windowX + deltaX;
        const newY = dragStartPos.current.windowY + deltaY;

        // Constrain to viewport (allow some off-screen for flexibility)
        const constrainedX = Math.max(-width + 100, Math.min(window.innerWidth - 100, newX));
        const constrainedY = Math.max(0, Math.min(window.innerHeight - 100, newY));

        onMove?.(constrainedX, constrainedY);
      }

      if (isResizing) {
        const deltaX = e.clientX - resizeStartPos.current.x;
        const deltaY = e.clientY - resizeStartPos.current.y;
        const newWidth = Math.max(200, resizeStartPos.current.width + deltaX);
        const newHeight = Math.max(150, resizeStartPos.current.height + deltaY);

        onResize?.(newWidth, newHeight);
      }
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      setIsResizing(false);
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);

    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDragging, isResizing, width, onMove, onResize]);

  // Prevent text selection while dragging
  useEffect(() => {
    if (isDragging || isResizing) {
      document.body.style.userSelect = "none";
      document.body.style.cursor = isDragging ? "move" : "nwse-resize";
    } else {
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
    }

    return () => {
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
    };
  }, [isDragging, isResizing]);

  return (
    <div
      ref={windowRef}
      className={`s7-window ${active ? "is-active" : "is-inactive"}`}
      style={{
        position: "absolute",
        left: `${x}px`,
        top: `${y}px`,
        width: `${width}px`,
        height: `${height}px`,
        zIndex,
      }}
      onMouseDown={handleWindowMouseDown}
      data-window-id={id}
    >
      <div
        className="s7-titlebar"
        onMouseDown={handleTitlebarMouseDown}
        style={{ cursor: isMaximized ? "default" : "move" }}
      >
        <button
          className="s7-window-control s7-window-close"
          aria-label="Close window"
          onClick={handleClose}
        />
        <button
          className="s7-window-control s7-window-minimize"
          aria-label="Minimize window"
          onClick={handleMinimize}
        />
        <div className="s7-title" aria-label={title}>
          {title}
        </div>
        <button
          className="s7-window-control s7-window-zoom"
          aria-label="Zoom window"
          onClick={handleMaximize}
        />
      </div>
      <div className="s7-window-content">{children}</div>
      {/* Resize handle in bottom-right corner */}
      <div
        className="s7-resize-handle"
        aria-hidden
        onMouseDown={handleResizeMouseDown}
        style={{
          position: 'absolute',
          bottom: 0,
          right: 0,
          width: '16px',
          height: '16px',
          cursor: isMaximized ? "default" : "nwse-resize",
          background: 'linear-gradient(135deg, transparent 50%, #888 50%, #888 60%, transparent 60%, transparent 70%, #888 70%, #888 80%, transparent 80%)',
        }}
      />
    </div>
  );
};

export default Window;
