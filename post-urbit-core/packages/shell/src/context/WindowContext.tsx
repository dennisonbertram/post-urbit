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
  cascadeWindows: () => void;
  tileWindows: () => void;
  bringAllToFront: () => void;
  getMinimizedWindows: () => WindowState[];
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

  // Cascade all non-minimized windows
  const cascadeWindows = useCallback(() => {
    const nonMinimized = windows.filter(w => !w.isMinimized);
    if (nonMinimized.length === 0) return;

    let startX = 50;
    let startY = 50;
    const offsetStep = 30;

    setWindows(prev =>
      prev.map(w => {
        if (w.isMinimized) return w;

        const index = nonMinimized.findIndex(nw => nw.id === w.id);
        const newX = startX + (index * offsetStep);
        const newY = startY + (index * offsetStep);

        return {
          ...w,
          x: newX,
          y: newY,
          zIndex: nextZIndex + index,
          isMaximized: false,
          originalBounds: undefined,
        };
      })
    );
    setNextZIndex(z => z + nonMinimized.length);
  }, [windows, nextZIndex]);

  // Tile all non-minimized windows in a grid
  const tileWindows = useCallback(() => {
    const nonMinimized = windows.filter(w => !w.isMinimized);
    if (nonMinimized.length === 0) return;

    // Calculate grid dimensions
    const cols = Math.ceil(Math.sqrt(nonMinimized.length));
    const rows = Math.ceil(nonMinimized.length / cols);

    // Available space (account for menu bar and status bar)
    const availableWidth = window.innerWidth;
    const availableHeight = window.innerHeight - 40;

    const windowWidth = Math.floor(availableWidth / cols);
    const windowHeight = Math.floor(availableHeight / rows);

    setWindows(prev =>
      prev.map(w => {
        if (w.isMinimized) return w;

        const index = nonMinimized.findIndex(nw => nw.id === w.id);
        const col = index % cols;
        const row = Math.floor(index / cols);

        return {
          ...w,
          x: col * windowWidth,
          y: row * windowHeight,
          width: windowWidth,
          height: windowHeight,
          zIndex: nextZIndex + index,
          isMaximized: false,
          originalBounds: undefined,
        };
      })
    );
    setNextZIndex(z => z + nonMinimized.length);
  }, [windows, nextZIndex]);

  // Bring all non-minimized windows to front
  const bringAllToFront = useCallback(() => {
    const nonMinimized = windows.filter(w => !w.isMinimized);
    if (nonMinimized.length === 0) return;

    setWindows(prev =>
      prev.map(w => {
        if (w.isMinimized) return w;

        const index = nonMinimized.findIndex(nw => nw.id === w.id);
        return {
          ...w,
          zIndex: nextZIndex + index,
        };
      })
    );
    setNextZIndex(z => z + nonMinimized.length);
  }, [windows, nextZIndex]);

  // Memoized helper to get minimized windows
  const minimizedWindows = useMemo(() => {
    return windows.filter(w => w.isMinimized);
  }, [windows]);

  const getMinimizedWindows = useCallback(() => {
    return minimizedWindows;
  }, [minimizedWindows]);

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
      cascadeWindows,
      tileWindows,
      bringAllToFront,
      getMinimizedWindows,
    }),
    [windows, openWindow, closeWindow, focusWindow, moveWindow, resizeWindow, minimizeWindow, maximizeWindow, restoreWindow, isAppOpen, cascadeWindows, tileWindows, bringAllToFront, getMinimizedWindows]
  );

  return (
    <WindowContext.Provider value={value}>
      {children}
    </WindowContext.Provider>
  );
};
