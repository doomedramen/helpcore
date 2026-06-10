/**
 * Slash command definitions and matching logic.
 *
 * Extracted into a pure module so both the SlashCommandMenu component
 * and handlePromptSubmit can share the same registry, and so the
 * matching/visibility logic is independently testable.
 */

/** A registered slash command with its display metadata and action. */
export interface SlashCommand {
  /** The slash-prefixed command text, e.g. "/compact". */
  slash: string;
  /** Short label shown in the command menu. */
  label: string;
  /** Description shown in the command menu. */
  description: string;
  /** Executed when the command is dispatched via prompt submit.
   *  Receives the text after the command (if any). */
  action: (args?: string) => void | Promise<void>;
}

/**
 * All registered slash commands.
 * Add new entries here to make them available in both the
 * autocomplete menu and the prompt-submit handler.
 */
export const COMMANDS: Omit<SlashCommand, "action">[] = [
  {
    slash: "/compact",
    label: "Compact conversation",
    description: "Summarize oldest messages to free context",
  },
  {
    slash: "/rename",
    label: "Rename conversation",
    description: "Generate or set a conversation title",
  },
];

/**
 * Returns true when the slash-command menu should be shown.
 *
 * Visible when:
 * 1. Text starts with "/"
 * 2. No space yet (user is still selecting a command, not typing args)
 * 3. At least one registered command fuzzy-matches the prefix
 */
export function shouldShowMenu(
  text: string,
  commands: Omit<SlashCommand, "action">[] = COMMANDS,
): boolean {
  if (!text.startsWith("/")) return false;
  if (text.includes(" ")) return false;
  return filterCommands(text, commands).length > 0;
}

/**
 * Returns commands that match the current input.
 * Matches against both the slash prefix and the command label.
 *
 * When the input is just "/" (no filter), returns all commands.
 */
export function filterCommands(
  input: string,
  commands: Omit<SlashCommand, "action">[] = COMMANDS,
): Omit<SlashCommand, "action">[] {
  if (!input.startsWith("/")) return [];
  const query = input.slice(1).toLowerCase();
  if (!query) return commands;
  return commands.filter(
    (c) => c.slash.toLowerCase().includes(query) || c.label.toLowerCase().includes(query),
  );
}
