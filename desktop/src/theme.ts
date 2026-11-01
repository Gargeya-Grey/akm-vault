import { useEffect, useState } from "react";
import type { Theme } from "./types";

function initialTheme(): Theme {
  try {
    const saved = localStorage.getItem("akm-theme");
    if (saved === "light" || saved === "dark") return saved;
  } catch {
    // Storage can be refused; fall back to the system setting.
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(initialTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem("akm-theme", theme);
    } catch {
      // Not fatal: the page still follows the chosen theme for this session.
    }
  }, [theme]);

  return {
    theme,
    toggle: () => setTheme((current) => (current === "dark" ? "light" : "dark")),
  };
}
