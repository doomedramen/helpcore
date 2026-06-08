"use client";

import { memo, useMemo } from "react";
import { MessageResponse } from "@/components/ai-elements/message";
import { findContentBlocks, RENDERERS } from "./content-renderers";
import type { ContentBlock } from "./content-renderers";

const DATA_IMAGE_REGEX = /!\[([^\]]*)\]\((data:image\/[^)]+)\)/g;
const DATA_AUDIO_REGEX = /!\[([^\]]*)\]\((data:audio\/[^)]+)\)/g;
const DATA_VIDEO_REGEX = /!\[([^\]]*)\]\((data:video\/[^)]+)\)/g;

interface AssetMatch {
  index: number;
  endIndex: number;
  alt: string;
  uri: string;
  kind: "image" | "audio" | "video";
}

function findAssets(content: string): AssetMatch[] {
  const matches: AssetMatch[] = [];

  for (const regex of [DATA_IMAGE_REGEX, DATA_AUDIO_REGEX, DATA_VIDEO_REGEX]) {
    let m: RegExpExecArray | null;
    const re = new RegExp(regex.source, "g");
    while ((m = re.exec(content)) !== null) {
      const kind =
        regex === DATA_IMAGE_REGEX ? "image" : regex === DATA_AUDIO_REGEX ? "audio" : "video";
      matches.push({
        index: m.index,
        endIndex: m.index + m[0].length,
        alt: m[1],
        uri: m[2],
        kind,
      });
    }
  }

  // Also detect standalone data: URIs that aren't inside markdown image syntax
  // Use a unified approach to avoid double-matching
  const standaloneRe = /(?:^|\s)(data:(image|audio|video)\/[^;]+;base64,[A-Za-z0-9+/=]+)/g;
  let m: RegExpExecArray | null;
  while ((m = standaloneRe.exec(content)) !== null) {
    const start = m.index + (m[0].startsWith(" ") ? 1 : 0);
    const end = start + m[1].length;
    // Skip if this position is already covered by a markdown image match
    if (!matches.some((existing) => start >= existing.index && start < existing.endIndex)) {
      // The regex already restricts the capture to image|audio|video
      const kind = m[2] as AssetMatch["kind"];
      matches.push({
        index: start,
        endIndex: end,
        alt: "",
        uri: m[1],
        kind,
      });
    }
  }

  return matches.sort((a, b) => a.index - b.index);
}

interface Segment {
  type: "text" | "image" | "audio" | "video" | "content";
  content: string;
  alt?: string;
  block?: ContentBlock;
}

function splitContent(
  content: string,
  assets: AssetMatch[],
  contentBlocks: ContentBlock[],
): Segment[] {
  // Merge asset matches and content blocks by index, then build segments
  const allMatches = [
    ...assets.map((a) => ({
      index: a.index,
      endIndex: a.endIndex,
      kind: "asset" as const,
      asset: a,
    })),
    ...contentBlocks.map((b) => ({
      index: b.index,
      endIndex: b.endIndex,
      kind: "block" as const,
      block: b,
    })),
  ].sort((a, b) => a.index - b.index);

  const segments: Segment[] = [];
  let lastEnd = 0;

  for (const match of allMatches) {
    if (match.index > lastEnd) {
      segments.push({ type: "text", content: content.slice(lastEnd, match.index) });
    }
    if (match.kind === "block") {
      segments.push({
        type: "content",
        content: match.block!.content,
        block: match.block,
      });
    } else if (match.kind === "asset") {
      segments.push({
        type: match.asset!.kind,
        content: match.asset!.uri,
        alt: match.asset!.alt || undefined,
      });
    }
    lastEnd = match.endIndex;
  }

  if (lastEnd < content.length) {
    segments.push({ type: "text", content: content.slice(lastEnd) });
  }

  return segments.length > 0 ? segments : [{ type: "text", content }];
}

interface MessageContentWithAssetsProps {
  children: string;
  className?: string;
  onContentError?: (type: string, message: string) => void;
}

export const MessageContentWithAssets = memo(
  ({ className, children, onContentError }: MessageContentWithAssetsProps) => {
    const segments = useMemo(() => {
      const blocks = findContentBlocks(children);
      const hasDataUri = children.includes("data:");
      if (blocks.length === 0 && !hasDataUri) return null;
      const assets = hasDataUri ? findAssets(children) : [];
      if (blocks.length === 0 && assets.length === 0) return null;
      return splitContent(children, assets, blocks);
    }, [children]);

    if (!segments) {
      return <MessageResponse className={className}>{children}</MessageResponse>;
    }

    return (
      <>
        {segments.map((segment, i) => {
          if (segment.type === "text") {
            return (
              <MessageResponse key={i} className={className}>
                {segment.content}
              </MessageResponse>
            );
          }
          if (segment.type === "content" && segment.block) {
            const Renderer = RENDERERS[segment.block.type];
            if (Renderer) {
              return (
                <Renderer
                  key={i}
                  variant={segment.block.variant}
                  content={segment.block.content}
                  onError={(msg: string) => onContentError?.(segment.block!.type, msg)}
                />
              );
            }
            return (
              <MessageResponse key={i} className={className}>
                {"```" + segment.block.type + "\n" + segment.block.content + "\n```"}
              </MessageResponse>
            );
          }
          if (segment.type === "image") {
            return (
              <img
                key={i}
                src={segment.content}
                alt={segment.alt ?? ""}
                className="h-auto max-w-full overflow-hidden rounded-md"
              />
            );
          }
          if (segment.type === "audio") {
            return (
              // eslint-disable-next-line jsx-a11y/media-has-caption
              <audio
                key={i}
                src={segment.content}
                controls
                preload="metadata"
                className="my-2 max-w-full"
                aria-label={segment.alt || "Audio attachment"}
              />
            );
          }
          if (segment.type === "video") {
            return (
              // eslint-disable-next-line jsx-a11y/media-has-caption
              <video
                key={i}
                src={segment.content}
                controls
                preload="metadata"
                className="my-2 h-auto max-w-full rounded-md"
                aria-label={segment.alt || "Video attachment"}
              />
            );
          }
          return null;
        })}
      </>
    );
  },
);

MessageContentWithAssets.displayName = "MessageContentWithAssets";
