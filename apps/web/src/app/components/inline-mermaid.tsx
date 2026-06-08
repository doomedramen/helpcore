"use client";

import * as React from "react";
import type { RendererProps } from "./content-renderers";

function ErrorBox({ message }: { message: string }) {
  return (
    <div className="rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs">
      <span className="font-semibold text-destructive">Diagram error:</span>{" "}
      <span className="text-muted-foreground">{message}</span>
    </div>
  );
}

export function InlineMermaid({ variant, content, onError }: RendererProps) {
  const [svg, setSvg] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const reportedRef = React.useRef<string | null>(null);
  const idRef = React.useRef(`mermaid-${Math.random().toString(36).slice(2, 9)}`);

  // Report rendering errors upward so the AI gets feedback
  React.useEffect(() => {
    if (error && error !== reportedRef.current && onError) {
      reportedRef.current = error;
      onError(error);
    }
  }, [error, onError]);

  React.useEffect(() => {
    let cancelled = false;
    setError(null);
    setSvg(null);

    if (!content.trim()) {
      setError("Mermaid definition is empty.");
      return;
    }

    import("mermaid")
      .then(async ({ default: mermaid }) => {
        mermaid.initialize({
          startOnLoad: false,
          theme: (variant || "default") as "default" | "dark" | "neutral" | "forest",
          securityLevel: "strict",
        });
        try {
          const { svg: rendered } = await mermaid.render(idRef.current, content);
          if (!cancelled) {
            setSvg(rendered);
          }
        } catch (e) {
          if (!cancelled) {
            setError(`Syntax error: ${(e as Error).message}`);
          }
        }
      })
      .catch(() => {
        if (!cancelled) setError("Mermaid renderer could not be loaded.");
      });

    return () => {
      cancelled = true;
    };
  }, [content, variant]);

  if (error) {
    return <ErrorBox message={error} />;
  }

  if (!svg) return null;

  return (
    <div className="my-2 flex justify-center overflow-x-auto">
      <div
        className="[&_svg]:max-w-full [&_svg]:h-auto"
        dangerouslySetInnerHTML={{ __html: svg }}
      />
    </div>
  );
}
