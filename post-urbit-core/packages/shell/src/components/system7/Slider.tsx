import React from "react";

type SliderProps = {
  label: string;
};

const Slider = ({ label }: SliderProps) => {
  return (
    <div className="s7-slider">
      <div className="s7-slider-label">{label}</div>
      <div className="s7-slider-track">
        <div className="s7-slider-thumb" />
      </div>
    </div>
  );
};

export default Slider;
