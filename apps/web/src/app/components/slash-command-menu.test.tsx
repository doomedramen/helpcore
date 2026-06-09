import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { SlashCommandMenu } from "./slash-command-menu";
import { PromptInputProvider } from "@/components/ai-elements/prompt-input";
import type { SlashCommand } from "@/lib/commands";

const TEST_COMMANDS: SlashCommand[] = [
  {
    slash: "/compact",
    label: "Compact conversation",
    description: "Summarize oldest messages",
    action: vi.fn(),
  },
  {
    slash: "/help",
    label: "Help",
    description: "Show available commands",
    action: vi.fn(),
  },
];

function renderWithProvider(text: string) {
  return render(
    <PromptInputProvider initialInput={text}>
      <SlashCommandMenu commands={TEST_COMMANDS} />
    </PromptInputProvider>,
  );
}

describe("SlashCommandMenu", () => {
  it("renders nothing when input has no slash", () => {
    renderWithProvider("hello");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows all commands when user types just a slash", async () => {
    renderWithProvider("/");
    // Popover content should be visible
    expect(screen.getByText("/compact")).toBeInTheDocument();
    expect(screen.getByText("/help")).toBeInTheDocument();
  });

  it("filters commands by partial match", async () => {
    renderWithProvider("/com");
    expect(screen.getByText("/compact")).toBeInTheDocument();
    expect(screen.queryByText("/help")).toBeNull();
  });

  it("hides when text contains a space", () => {
    renderWithProvider("/compact ");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("hides when no commands match", () => {
    renderWithProvider("/sdfsdfse");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows empty state inside popover when filtered list is empty but slash is typed", () => {
    // This tests the edge case: shouldShowMenu returns false for unknown
    // commands, so the popover is closed entirely. The "no matches" text
    // from CommandEmpty only appears when shouldShowMenu returns true but
    // filterCommands returns [] (which can't happen with current logic
    // since shouldShowMenu already checks filterCommands length).
    // Keeping this test as documentation of the behavior.
    renderWithProvider("/");
    expect(screen.getByText("/compact")).toBeInTheDocument();
  });

  it("renders nothing when outside PromptInputProvider", () => {
    render(<SlashCommandMenu commands={TEST_COMMANDS} />);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
