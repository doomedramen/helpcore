"use client";

import { useCallback, useEffect, useId, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import type { ReactNode } from "react";
import { Button } from "@/app/components/ui/button";
import { CheckIcon, CopyIcon } from "lucide-react";

function CopyCodeButton({ code }: { code: string }) {
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef<number>(0);

  const copy = useCallback(async () => {
    if (typeof navigator?.clipboard?.writeText !== "function") return;
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      timeoutRef.current = window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* clipboard unavailable */
    }
  }, [code]);

  useEffect(() => () => clearTimeout(timeoutRef.current), []);

  const Icon = copied ? CheckIcon : CopyIcon;
  return (
    <Button
      size="icon"
      variant="ghost"
      className="shrink-0 size-6 opacity-0 group-hover:opacity-100 transition-opacity"
      onClick={copy}
      type="button"
    >
      <Icon size={14} />
    </Button>
  );
}

export default function CodeBlockInjector({ children }: { children: ReactNode }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const rootsRef = useRef<Map<Element, ReturnType<typeof createRoot>>>(new Map());
  const id = useId();

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const found = new Set<Element>();

    for (const pre of container.querySelectorAll("pre")) {
      found.add(pre);
      if (rootsRef.current.has(pre)) continue;

      const code = pre.querySelector("code");
      if (!code) continue;

      const hasClass = pre.dataset.codeCopyInjected === id;
      if (hasClass) continue;
      pre.setAttribute("data-code-copy-injected", id);

      pre.classList.add("group", "relative");

      const buttonWrap = document.createElement("div");
      buttonWrap.className = "absolute top-2 right-2 z-10 pointer-events-auto";

      pre.appendChild(buttonWrap);

      const root = createRoot(buttonWrap);
      root.render(<CopyCodeButton code={code.textContent ?? ""} />);
      rootsRef.current.set(pre, root);
    }

    const cleanupRoots = rootsRef.current;

    return () => {
      for (const [el] of cleanupRoots) {
        if (!found.has(el)) {
          const root = cleanupRoots.get(el);
          if (root) {
            root.unmount();
            cleanupRoots.delete(el);
          }
        }
      }
    };
  }, [children, id]);

  return <div ref={containerRef}>{children}</div>;
}
