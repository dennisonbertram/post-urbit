import React from "react";

type WindowProps = {
  title: string;
  active?: boolean;
  children: React.ReactNode;
};

const Window = ({ title, active = true, children }: WindowProps) => {
  return (
    <div className={`s7-window ${active ? "is-active" : "is-inactive"}`}>
      <div className="s7-titlebar">
        <button
          className="s7-window-control s7-window-close"
          aria-label="Close window"
        />
        <div className="s7-title" aria-label={title}>
          {title}
        </div>
        <button
          className="s7-window-control s7-window-zoom"
          aria-label="Zoom window"
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

export default Window;
