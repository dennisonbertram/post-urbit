import React, { createContext, useContext, useState, useCallback, useMemo } from "react";

export type WindowState = {
  id: string;
  title: string;
  content: React.ReactNode;
  x: number;
  y: number;
  width: number;
  height: number;
  zIndex: number;
  isMinimized: boolean;
  isMaximized: boolean;
  // Store original position/size for restoring from maximized
  originalBounds?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
};

type WindowContextType = {
  windows: WindowState[];
  openWindow: (id: string, title: string, content: React.ReactNode, options?: Partial<WindowState>) => void;
  closeWindow: (id: string) => void;
  focusWindow: (id: string) => void;
  moveWindow: (id: string, x: number, y: number) => void;
  resizeWindow: (id: string, width: number, height: number) => void;
  minimizeWindow: (id: string) => void;
  maximizeWindow: (id: string) => void;
  restoreWindow: (id: string) => void;
  isAppOpen: (id: string) => boolean;
};

const WindowContext = createContext<WindowContextType | null>(null);

export const useWindows = () => {
  const context = useContext(WindowContext);
  if (!context) {
    throw new Error("useWindows must be used within WindowProvider");
  }
  return context;
};

type WindowProviderProps = {
  children: React.ReactNode;
};

export const WindowProvider = ({ children }: WindowProviderProps) => {
  const [windows, setWindows] = useState<WindowState[]>([]);
  const [nextZIndex, setNextZIndex] = useState(100);

  // Open a new window or bring existing one to front
  const openWindow = useCallback((
    id: string,
    title: string,
    content: React.ReactNode,
    options?: Partial<WindowState>
  ) => {
    setWindows(prev => {
      // Check if window already exists
      const existing = prev.find(w => w.id === id);
      if (existing) {
        // Bring to front
        return prev.map(w =>
          w.id === id
            ? { ...w, zIndex: nextZIndex, isMinimized: false }
            : w
        );
      }

      // Create new window with cascading position
      const offset = prev.length * 30;
      const newWindow: WindowState = {
        id,
        title,
        content,
        x: options?.x ?? 100 + offset,
        y: options?.y ?? 100 + offset,
        width: options?.width ?? 500,
        height: options?.height ?? 400,
        zIndex: nextZIndex,
        isMinimized: false,
        isMaximized: false,
        ...options,
      };

      return [...prev, newWindow];
    });
    setNextZIndex(z => z + 1);
  }, [nextZIndex]);

  // Close window
  const closeWindow = useCallback((id: string) => {
    setWindows(prev => prev.filter(w => w.id !== id));
  }, []);

  // Bring window to front
  const focusWindow = useCallback((id: string) => {
    setWindows(prev => {
      const window = prev.find(w => w.id === id);
      if (!window) return prev;

      return prev.map(w =>
        w.id === id
          ? { ...w, zIndex: nextZIndex }
          : w
      );
    });
    setNextZIndex(z => z + 1);
  }, [nextZIndex]);

  // Move window to new position
  const moveWindow = useCallback((id: string, x: number, y: number) => {
    setWindows(prev =>
      prev.map(w =>
        w.id === id && !w.isMaximized
          ? { ...w, x, y }
          : w
      )
    );
  }, []);

  // Resize window
  const resizeWindow = useCallback((id: string, width: number, height: number) => {
    setWindows(prev =>
      prev.map(w =>
        w.id === id && !w.isMaximized
          ? { ...w, width, height }
          : w
      )
    );
  }, []);

  // Minimize window
  const minimizeWindow = useCallback((id: string) => {
    setWindows(prev =>
      prev.map(w =>
        w.id === id
          ? { ...w, isMinimized: true }
          : w
      )
    );
  }, []);

  // Maximize window (fullscreen minus menu bar and status bar)
  const maximizeWindow = useCallback((id: string) => {
    setWindows(prev =>
      prev.map(w => {
        if (w.id !== id) return w;

        if (w.isMaximized) {
          // Restore from maximized
          return {
            ...w,
            isMaximized: false,
            x: w.originalBounds?.x ?? w.x,
            y: w.originalBounds?.y ?? w.y,
            width: w.originalBounds?.width ?? w.width,
            height: w.originalBounds?.height ?? w.height,
            originalBounds: undefined,
          };
        } else {
          // Maximize
          return {
            ...w,
            isMaximized: true,
            originalBounds: {
              x: w.x,
              y: w.y,
              width: w.width,
              height: w.height,
            },
            x: 0,
            y: 0,
            // Account for menu bar (20px) and status bar (20px)
            width: window.innerWidth,
            height: window.innerHeight - 40,
          };
        }
      })
    );
  }, []);

  // Restore window from minimized
  const restoreWindow = useCallback((id: string) => {
    setWindows(prev =>
      prev.map(w =>
        w.id === id
          ? { ...w, isMinimized: false }
          : w
      )
    );
    focusWindow(id);
  }, [focusWindow]);

  // Check if an app/window is open
  const isAppOpen = useCallback((id: string) => {
    return windows.some(w => w.id === id);
  }, [windows]);

  const value = useMemo(
    () => ({
      windows,
      openWindow,
      closeWindow,
      focusWindow,
      moveWindow,
      resizeWindow,
      minimizeWindow,
      maximizeWindow,
      restoreWindow,
      isAppOpen,
    }),
    [windows, openWindow, closeWindow, focusWindow, moveWindow, resizeWindow, minimizeWindow, maximizeWindow, restoreWindow, isAppOpen]
  );

  return (
    <WindowContext.Provider value={value}>
      {children}
    </WindowContext.Provider>
  );
};
