import React from "react";

/**
 * Dialog - A non-interactive window component for modal dialogs.
 * Use this for login prompts, alerts, and other modal overlays.
 * For draggable, interactive windows, use Window component with WindowManager.
 */

type DialogProps = {
  title: string;
  active?: boolean;
  children: React.ReactNode;
};

const Dialog = ({ title, active = true, children }: DialogProps) => {
  return (
    <div className={`s7-window ${active ? "is-active" : "is-inactive"}`}>
      <div className="s7-titlebar">
        <button
          className="s7-window-control s7-window-close"
          aria-label="Close window"
          disabled
          style={{ cursor: 'default' }}
        />
        <div className="s7-title" aria-label={title}>
          {title}
        </div>
        <button
          className="s7-window-control s7-window-zoom"
          aria-label="Zoom window"
          disabled
          style={{ cursor: 'default' }}
        />
      </div>
      <div className="s7-window-content">{children}</div>
      <div className="s7-window-footer">
        <div className="s7-scrollbar horizontal">
          <div className="s7-scrollbar-button">&lt;</div>
          <div className="s7-scrollbar-track">
            <div className="s7-scrollbar-thumb" />
          </div>
          <div className="s7-scrollbar-button">&gt;</div>
        </div>
        <div className="s7-resize-handle" aria-hidden />
      </div>
    </div>
  );
};

export default Dialog;
