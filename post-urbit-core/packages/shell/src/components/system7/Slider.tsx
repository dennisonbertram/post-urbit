import React, { useState, useRef, useEffect } from "react";

type SliderProps = {
  label: string;
  value?: number;
  onChange?: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  showValue?: boolean;
};

// Usage:
// <Slider label="Volume" value={volume} onChange={setVolume} min={0} max={100} showValue />
const Slider = ({
  label,
  value = 50,
  onChange,
  min = 0,
  max = 100,
  step = 1,
  disabled = false,
  showValue = false,
}: SliderProps) => {
  const [isDragging, setIsDragging] = useState(false);
  const trackRef = useRef<HTMLDivElement>(null);

  // Calculate percentage for thumb position
  const percentage = ((value - min) / (max - min)) * 100;

  const updateValue = (clientX: number) => {
    if (!trackRef.current || disabled) return;

    const rect = trackRef.current.getBoundingClientRect();
    const clickX = clientX - rect.left;
    const width = rect.width;
    const percentage = Math.max(0, Math.min(100, (clickX / width) * 100));
    const rawValue = min + (percentage / 100) * (max - min);
    const steppedValue = Math.round(rawValue / step) * step;
    const clampedValue = Math.max(min, Math.min(max, steppedValue));

    onChange?.(clampedValue);
  };

  const handleMouseDown = (e: React.MouseEvent) => {
    if (disabled) return;
    setIsDragging(true);
    updateValue(e.clientX);
  };

  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      updateValue(e.clientX);
    };

    const handleMouseUp = () => {
      setIsDragging(false);
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);

    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDragging, min, max, step, disabled]);

  const disabledClass = disabled ? "is-disabled" : "";

  return (
    <div className={`s7-slider ${disabledClass}`}>
      <div className="s7-slider-label">
        {label}
        {showValue && <span> ({value})</span>}
      </div>
      <div
        className="s7-slider-track"
        ref={trackRef}
        onMouseDown={handleMouseDown}
        style={{
          cursor: disabled ? "default" : "pointer",
          position: "relative",
        }}
      >
        <div
          className="s7-slider-thumb"
          style={{
            left: `${percentage}%`,
            position: "absolute",
            transform: "translateX(-50%)",
            cursor: disabled ? "default" : isDragging ? "grabbing" : "grab",
          }}
        />
      </div>
    </div>
  );
};

export default Slider;
