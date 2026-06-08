import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Merge class names with Tailwind conflict resolution.
 * Combines `clsx` for conditional classes and `tailwind-merge` to deduplicate
 * Tailwind utility classes (the last one wins).
 * @param inputs - Any number of class values (strings, objects, arrays).
 * @returns A single merged class string.
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
