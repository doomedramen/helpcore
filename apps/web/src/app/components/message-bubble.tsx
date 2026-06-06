"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import { ChevronDown, ChevronRight, Volume2, VolumeX } from "lucide-react";
import { tts } from "@/lib/api";
import type { Message } from "@/lib/types";

import "highlight.js/styles/github.css";

function stripMarkdown(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, "")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/[*_~]{1,2}([^*_~]+)[*_~]{1,2}/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/^>+\s+/gm, "")
    .replace(/^\s*[-*+]\s+/gm, "")
    .replace(/^\s*\d+\.\s+/gm, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

function ToolMessage({ message }: { message: Message }) {
  const [expanded, setExpanded] = useState(false);
  const pretty = prettyJson(message.content);
  const isError = (() => {
    try {
      return (JSON.parse(message.content) as { ok?: boolean }).ok === false;
    } catch {
      return false;
    }
  })();

  return (
    <div className="flex justify-start">
      <div className="max-w-[85%] rounded-xl border border-slate-200 bg-slate-50 text-xs dark:border-slate-700 dark:bg-slate-900/50">
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="flex w-full items-center gap-1.5 px-3 py-2 text-left text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200"
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          <span
            className={`font-mono font-medium ${isError ? "text-red-500 dark:text-red-400" : "text-slate-500 dark:text-slate-400"}`}
          >
            {isError ? "tool error" : "tool result"}
          </span>
          {message.tool_call_id && (
            <span className="ml-1 truncate text-slate-400 dark:text-slate-500">
              · {message.tool_call_id}
            </span>
          )}
        </button>
        {expanded && (
          <pre className="overflow-x-auto border-t border-slate-200 px-3 py-2 font-mono text-slate-700 dark:border-slate-700 dark:text-slate-300">
            {pretty}
          </pre>
        )}
      </div>
    </div>
  );
}

interface Props {
  message: Message;
  onRetry?: (messageId: string) => void;
  retrying?: boolean;
  accessToken: string;
  hasAudio?: boolean;
}

export default function MessageBubble({
  message,
  onRetry,
  retrying = false,
  accessToken,
  hasAudio = false,
}: Props) {
  const isUser = message.role === "user";

  const [playing, setPlaying] = useState(false);
  const [playError, setPlayError] = useState(false);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const urlRef = useRef<string | null>(null);

  const handleSpeak = useCallback(async () => {
    if (playing) {
      audioRef.current?.pause();
      if (urlRef.current) {
        URL.revokeObjectURL(urlRef.current);
        urlRef.current = null;
      }
      audioRef.current = null;
      setPlaying(false);
      setPlayError(false);
      return;
    }

    setPlayError(false);
    try {
      const blob = await tts(stripMarkdown(message.content), accessToken);
      const url = URL.createObjectURL(blob);
      const audio = new Audio(url);

      audio.onended = () => {
        setPlaying(false);
        if (urlRef.current) {
          URL.revokeObjectURL(urlRef.current);
          urlRef.current = null;
        }
        audioRef.current = null;
      };
      audio.onerror = () => {
        setPlaying(false);
        setPlayError(true);
        if (urlRef.current) {
          URL.revokeObjectURL(urlRef.current);
          urlRef.current = null;
        }
        audioRef.current = null;
      };

      urlRef.current = url;
      audioRef.current = audio;
      setPlaying(true);
      await audio.play();
    } catch {
      setPlayError(true);
      setPlaying(false);
    }
  }, [message.content, accessToken, playing]);

  useEffect(() => {
    return () => {
      audioRef.current?.pause();
      if (urlRef.current) {
        URL.revokeObjectURL(urlRef.current);
      }
    };
  }, []);

  if (message.role === "tool") {
    return <ToolMessage message={message} />;
  }

  if (isUser) {
    return (
      <div className="flex justify-end">
        <div className="max-w-[80%] rounded-2xl rounded-tr-sm bg-blue-600 px-4 py-2.5 text-sm text-white shadow-sm">
          <p className="whitespace-pre-wrap leading-relaxed">{message.content}</p>
        </div>
      </div>
    );
  }

  const active = message.status === "pending" || message.status === "streaming";
  const retryable = message.status === "failed" || message.status === "interrupted";

  const canSpeak = hasAudio && message.status === "complete" && !!message.content;

  if (!isUser && !active && !retryable && !message.content) return null;

  return (
    <div className="flex justify-start">
      <div className="max-w-[85%] rounded-2xl rounded-tl-sm border border-slate-200 bg-white px-4 py-3 text-sm text-slate-900 shadow-sm dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100">
        {active && !message.content ? (
          <span className="flex gap-1 items-center py-0.5">
            <span className="w-1.5 h-1.5 rounded-full bg-slate-400 animate-bounce [animation-delay:0ms]" />
            <span className="w-1.5 h-1.5 rounded-full bg-slate-400 animate-bounce [animation-delay:150ms]" />
            <span className="w-1.5 h-1.5 rounded-full bg-slate-400 animate-bounce [animation-delay:300ms]" />
          </span>
        ) : (
          <div className="prose prose-sm max-w-none">
            <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]}>
              {message.content}
            </ReactMarkdown>
          </div>
        )}
        {active && message.content && (
          <p className="mt-2 text-xs text-slate-400 dark:text-slate-500">Still working…</p>
        )}
        {retryable && (
          <div className="mt-3 border-t border-slate-200 pt-3 dark:border-slate-700">
            <p className="text-xs text-red-600 dark:text-red-400">
              {message.error || "This response did not finish."}
            </p>
            {onRetry && (
              <button
                type="button"
                onClick={() => onRetry(message.id)}
                disabled={retrying}
                className="mt-2 rounded-lg border border-slate-300 px-2.5 py-1.5 text-xs font-medium text-slate-700 transition-colors hover:bg-slate-50 disabled:cursor-wait disabled:opacity-50 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
              >
                {retrying ? "Retrying…" : "Retry response"}
              </button>
            )}
          </div>
        )}
        {canSpeak && (
          <div className="mt-2 flex items-center gap-2">
            <button
              type="button"
              onClick={handleSpeak}
              className={`rounded-lg p-1.5 transition-colors ${
                playError
                  ? "text-red-400 hover:bg-red-50 dark:hover:bg-red-900/30"
                  : playing
                    ? "text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-900/30"
                    : "text-slate-400 hover:bg-slate-100 hover:text-slate-600 dark:hover:bg-slate-800 dark:hover:text-slate-300"
              }`}
              aria-label={playing ? "Stop" : "Read aloud"}
            >
              {playing ? <VolumeX size={14} /> : <Volume2 size={14} />}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
