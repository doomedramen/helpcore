"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/app/components/ui/popover";
import { useOptionalPromptInputController } from "@/components/ai-elements/prompt-input";
import { type SlashCommand, filterCommands, shouldShowMenu } from "@/lib/commands";
import { cn } from "@/lib/utils";

interface SlashCommandMenuProps {
  /** Full command registry with actions (used by the parent for dispatch). */
  commands: SlashCommand[];
  className?: string;
}

export function SlashCommandMenu({ commands, className }: SlashCommandMenuProps) {
  const controller = useOptionalPromptInputController();
  const value = controller?.textInput.value ?? "";
  const setInput = controller?.textInput.setInput;

  const [open, setOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);

  const commandDefs = useMemo(
    () => commands.map(({ slash, label, description }) => ({ slash, label, description })),
    [commands],
  );

  // Determine whether the menu should be visible based on the current text.
  const show = useMemo(() => shouldShowMenu(value, commandDefs), [value, commandDefs]);

  // Sync popover open state with visibility.
  useEffect(() => {
    setOpen(show);
    if (show) setSelectedIndex(0);
  }, [show]);

  // Filter commands based on typed prefix.
  const filtered = useMemo(() => filterCommands(value, commandDefs), [value, commandDefs]);

  const handleSelect = useCallback(
    (slash: string) => {
      // Fill the textarea with the full command + trailing space,
      // so the user can immediately type args or press Enter.
      setInput?.(slash + " ");
      setOpen(false);
    },
    [setInput],
  );

  const handleOpenChange = useCallback((newOpen: boolean) => {
    // Only allow external close (Escape, click outside).
    // Re-opening is driven by text value changes via the effect above.
    if (!newOpen) {
      setOpen(false);
    }
  }, []);

  // No controller? Nothing to render.
  if (!controller) return null;

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      {/* Hidden trigger — popover is opened/closed programmatically via
          the `open` prop, so we don't need a visible trigger element. */}
      <PopoverTrigger className="absolute inset-0 pointer-events-none" />
      <PopoverContent
        side="top"
        align="start"
        sideOffset={8}
        initialFocus={false}
        className={cn("w-72 p-0", className)}
      >
        <div className="overflow-hidden p-1">
          <div className="px-2 py-1.5 text-xs font-medium text-muted-foreground">Commands</div>
          {filtered.length > 0 ? (
            <div className="flex flex-col">
              {filtered.map((cmd, i) => (
                <button
                  key={cmd.slash}
                  type="button"
                  onMouseDown={(e) => {
                    // Prevent the popover trigger from stealing focus on click.
                    e.preventDefault();
                    handleSelect(cmd.slash);
                  }}
                  className={cn(
                    "flex items-center gap-2 rounded-sm px-2 py-1.5 text-sm text-left",
                    i === selectedIndex ? "bg-muted text-foreground" : "text-popover-foreground",
                  )}
                >
                  <span className="font-medium shrink-0">{cmd.slash}</span>
                  <span className="truncate text-muted-foreground">{cmd.description}</span>
                </button>
              ))}
            </div>
          ) : (
            <div className="py-6 text-center text-sm text-muted-foreground">
              No matching commands
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
