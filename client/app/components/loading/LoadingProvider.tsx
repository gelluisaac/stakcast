"use client";
import React, { createContext, useContext, useState, useCallback } from "react";
import Spinner from "./Spinner";

// Create the context
const LoadingContext = createContext({
  isLoading: false,
  showSpinner: () => {},
  hideSpinner: () => {},
});

// Custom hook to use the context
export const useLoading = () => useContext(LoadingContext);

// Provider component
const LoadingProvider = ({ children }: { children: React.ReactNode }) => {
  const [isLoading, setIsLoading] = useState(false);

  // Memoize showSpinner and hideSpinner
  const showSpinner = useCallback(() => setIsLoading(true), []);
  const hideSpinner = useCallback(() => setIsLoading(false), []);

  return (
    <LoadingContext.Provider value={{ isLoading, showSpinner, hideSpinner }}>
      {children}
      {isLoading && <Spinner />} {/* Render the spinner when isLoading is true */}
    </LoadingContext.Provider>
  );
};

export default LoadingProvider;