"use client";

import { useEffect, useRef } from "react";
import { toast } from "sonner";

/**
 * Detects when the service worker is replaced by a new version and shows a
 * persistent toast prompting the user to reload.
 *
 * Works by watching for the native `controllerchange` event. Because the SW
 * is configured with `skipWaiting: true` + `clientsClaim: true`, the new SW
 * takes over immediately — there is no "waiting" phase. The event fires on
 * every client that was already open when the new SW activated.
 *
 * We guard against firing on first install (when there was no previous
 * controller) by checking `navigator.serviceWorker.controller` at mount time.
 */
export function SwUpdateToast() {
  const shown = useRef(false);

  useEffect(() => {
    if (typeof navigator === "undefined" || !("serviceWorker" in navigator)) return;
    if (!navigator.serviceWorker.controller) return;

    const handleControllerChange = () => {
      if (shown.current) return;
      shown.current = true;
      toast("Update available", {
        description: "A new version of helpcore has been installed.",
        action: { label: "Reload", onClick: () => window.location.reload() },
        duration: Infinity,
        id: "sw-update",
      });
    };

    navigator.serviceWorker.addEventListener("controllerchange", handleControllerChange);
    return () =>
      navigator.serviceWorker.removeEventListener("controllerchange", handleControllerChange);
  }, []);

  return null;
}
