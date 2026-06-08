"use client";

/**
 * Re-exported from next-themes.
 * @returns The current theme and a setter (light / dark / system).
 */
export { useTheme } from "next-themes";

/**
 * Re-exported from the local theme-provider wrapper.
 * Wraps next-themes' ThemeProvider with attribute-based dark mode.
 */
export { ThemeProvider } from "./theme-provider";
