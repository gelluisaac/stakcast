"use client"; 
import React from "react";

const Spinner = () => {
  return (
    <div className="fixed inset-0 flex items-center justify-center bg-white bg-opacity-80 z-[9999]">
      <div className="border-t-4 border-blue-500 border-solid rounded-full w-10 h-10 animate-spin"></div>
    </div>
  );
};

export default Spinner;