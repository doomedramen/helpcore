"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/app/components/ui/popover";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandList,
} from "@/app/components/ui/command";
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

  const commandDefs = useMemo(
    () => commands.map(({ slash, label, description }) => ({ slash, label, description })),
    [commands],
  );

  // Determine whether the menu should be visible based on the current text.
  const show = useMemo(() => shouldShowMenu(value, commandDefs), [value, commandDefs]);

  // Sync popover open state with visibility.
  useEffect(() => {
    setOpen(show);
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
      <PopoverContent side="top" align="start" sideOffset={8} className={cn("w-72 p-0", className)}>
        <Command>
          <CommandList>
            <CommandGroup heading="Commands">
              {filtered.map((cmd) => (
                <CommandItem
                  key={cmd.slash}
                  value={cmd.slash}
                  onSelect={() => handleSelect(cmd.slash)}
                >
                  <span className="font-medium">{cmd.slash}</span>
                  <span className="ml-2 truncate text-muted-foreground">{cmd.description}</span>
                </CommandItem>
              ))}
            </CommandGroup>
            <CommandEmpty>No matching commands</CommandEmpty>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
