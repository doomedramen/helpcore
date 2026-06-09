import { describe, expect, it } from "vitest";
import { filterCommands, shouldShowMenu, type SlashCommand } from "@/lib/commands";

const EXTRA_COMMANDS: Omit<SlashCommand, "action">[] = [
  { slash: "/compact", label: "Compact conversation", description: "Summarize oldest messages" },
  { slash: "/help", label: "Help", description: "Show available commands" },
];

describe("shouldShowMenu", () => {
  it("returns false for empty text", () => {
    expect(shouldShowMenu("", EXTRA_COMMANDS)).toBe(false);
  });

  it("returns false for text without slash", () => {
    expect(shouldShowMenu("hello", EXTRA_COMMANDS)).toBe(false);
    expect(shouldShowMenu("compact", EXTRA_COMMANDS)).toBe(false);
  });

  it("returns true for just a slash (shows all commands)", () => {
    expect(shouldShowMenu("/", EXTRA_COMMANDS)).toBe(true);
  });

  it("returns true for a matching partial command", () => {
    expect(shouldShowMenu("/com", EXTRA_COMMANDS)).toBe(true);
    expect(shouldShowMenu("/comp", EXTRA_COMMANDS)).toBe(true);
    expect(shouldShowMenu("/compact", EXTRA_COMMANDS)).toBe(true);
  });

  it("returns true for partial match against label", () => {
    expect(shouldShowMenu("/help", EXTRA_COMMANDS)).toBe(true);
  });

  it("returns false when text contains a space (user typing args)", () => {
    expect(shouldShowMenu("/compact ", EXTRA_COMMANDS)).toBe(false);
    expect(shouldShowMenu("/compact arg", EXTRA_COMMANDS)).toBe(false);
  });

  it("returns false for unknown command with no matches", () => {
    expect(shouldShowMenu("/sdfsdfse", EXTRA_COMMANDS)).toBe(false);
    expect(shouldShowMenu("/xyz", EXTRA_COMMANDS)).toBe(false);
  });

  it("returns false for text that has slash but not at start", () => {
    expect(shouldShowMenu("hello /compact", EXTRA_COMMANDS)).toBe(false);
  });

  it("works with default COMMANDS (no override)", () => {
    expect(shouldShowMenu("/")).toBe(true);
    expect(shouldShowMenu("/compact")).toBe(true);
    expect(shouldShowMenu("/unknown")).toBe(false);
  });
});

describe("filterCommands", () => {
  it("returns all commands for just a slash", () => {
    const result = filterCommands("/", EXTRA_COMMANDS);
    expect(result).toHaveLength(EXTRA_COMMANDS.length);
    expect(result[0].slash).toBe("/compact");
    expect(result[1].slash).toBe("/help");
  });

  it("returns empty array for non-slash text", () => {
    expect(filterCommands("hello", EXTRA_COMMANDS)).toHaveLength(0);
    expect(filterCommands("compact", EXTRA_COMMANDS)).toHaveLength(0);
  });

  it("filters by slash prefix", () => {
    const result = filterCommands("/com", EXTRA_COMMANDS);
    expect(result).toHaveLength(1);
    expect(result[0].slash).toBe("/compact");
  });

  it("filters by label substring", () => {
    const result = filterCommands("/help", EXTRA_COMMANDS);
    expect(result).toHaveLength(1);
    expect(result[0].slash).toBe("/help");
  });

  it("is case-insensitive", () => {
    expect(filterCommands("/COMPACT", EXTRA_COMMANDS)).toHaveLength(1);
    expect(filterCommands("/Help", EXTRA_COMMANDS)).toHaveLength(1);
  });
});
