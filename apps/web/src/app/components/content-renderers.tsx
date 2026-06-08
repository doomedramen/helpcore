"use client";

import type { ComponentType } from "react";
import { InlineChart } from "./inline-chart";
import { InlineMermaid } from "./inline-mermaid";

export interface ContentBlock {
  type: string;
  variant: string;
  content: string;
  index: number;
  endIndex: number;
}

export interface RendererProps {
  variant: string;
  content: string;
  onError?: (message: string) => void;
}

export type ContentRenderer = ComponentType<RendererProps>;

export const RENDERERS: Record<string, ContentRenderer> = {
  chart: InlineChart,
  mermaid: InlineMermaid,
};

export const FENCE_REGEX = /```(chart|mermaid):?(\w+)?\n([\s\S]*?)```/g;

export function findContentBlocks(text: string): ContentBlock[] {
  const blocks: ContentBlock[] = [];
  const re = new RegExp(FENCE_REGEX.source, "g");
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    blocks.push({
      type: m[1],
      variant: m[2] || "",
      content: m[3].trim(),
      index: m.index,
      endIndex: m.index + m[0].length,
    });
  }
  return blocks;
}
