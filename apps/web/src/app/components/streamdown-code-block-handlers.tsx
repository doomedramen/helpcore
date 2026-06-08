"use client";

import { useEffect } from "react";

function getCodeBlockContent(button: HTMLElement): string | null {
  const codeBlock = button.closest("[data-streamdown='code-block']");
  if (!codeBlock) return null;
  const body = codeBlock.querySelector("[data-streamdown='code-block-body']");
  return body?.textContent ?? null;
}

function getFileName(button: HTMLElement): string {
  const codeBlock = button.closest("[data-streamdown='code-block']");
  const header = codeBlock?.querySelector("[data-streamdown='code-block-header']");
  const lang = header?.getAttribute("data-language") ?? "file";
  return `${lang}.txt`;
}

function handleCopy(button: HTMLElement) {
  const code = getCodeBlockContent(button);
  if (!code) return;
  void navigator.clipboard.writeText(code).catch(() => {});
  const initialTitle = button.getAttribute("title") ?? "";
  button.setAttribute("title", "Copied!");
  // Reset tooltip after a moment
  setTimeout(() => button.setAttribute("title", initialTitle), 1500);
}

function handleDownload(button: HTMLElement) {
  const code = getCodeBlockContent(button);
  if (!code) return;
  const filename = getFileName(button);
  const blob = new Blob([code], { type: "text/plain" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

export default function StreamdownCodeBlockHandlers() {
  useEffect(() => {
    function onClick(e: MouseEvent) {
      const target = e.target as HTMLElement | null;
      if (!target) return;

      const copyButton = target.closest(
        "[data-streamdown='code-block-copy-button']",
      ) as HTMLElement | null;
      if (copyButton) {
        handleCopy(copyButton);
        return;
      }

      const downloadButton = target.closest(
        "[data-streamdown='code-block-download-button']",
      ) as HTMLElement | null;
      if (downloadButton) {
        handleDownload(downloadButton);
      }
    }

    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  }, []);

  return null;
}
