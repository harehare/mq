import { useState, useEffect } from "react";
import { getStoredValue, setStoredValue } from "../utils";
import { STORAGE_KEYS } from "../constants";

export const useDarkMode = () => {
  const getSystemPreference = () => {
    return (
      window.matchMedia &&
      window.matchMedia("(prefers-color-scheme: dark)").matches
    );
  };

  const [isDarkMode, setIsDarkMode] = useState(() => {
    const savedDarkMode = getStoredValue(STORAGE_KEYS.DARK_MODE, null);
    return savedDarkMode !== null ? savedDarkMode : getSystemPreference();
  });

  useEffect(() => {
    const savedDarkMode = getStoredValue(STORAGE_KEYS.DARK_MODE, null);
    if (savedDarkMode === null) {
      setIsDarkMode(getSystemPreference());
    } else {
      setIsDarkMode(savedDarkMode);
    }
  }, []);

  const toggleDarkMode = () => {
    const newDarkMode = !isDarkMode;
    setIsDarkMode(newDarkMode);
    setStoredValue(STORAGE_KEYS.DARK_MODE, newDarkMode);
  };

  return {
    isDarkMode,
    toggleDarkMode,
  };
};
