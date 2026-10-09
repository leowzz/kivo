import { relaunch } from "@tauri-apps/plugin-process";
import { check as checkUpdate } from "@tauri-apps/plugin-updater";
import { useCallback, useEffect, useRef, useState } from "react";
import { createReleaseUpdater, initialUpdateState } from "./releaseUpdate";

export function useReleaseUpdate({ enabled = true, beforeInstall }: {
  enabled?: boolean;
  beforeInstall?: () => Promise<void>;
} = {}) {
  const native = "__TAURI_INTERNALS__" in window;
  const [state, setState] = useState(initialUpdateState);
  const checker = useRef<ReturnType<typeof createReleaseUpdater> | null>(null);
  const prepare = useRef(beforeInstall);
  prepare.current = beforeInstall;
  const check = useCallback((force = false) => { void checker.current?.check(force); }, []);
  const install = useCallback(() => { void checker.current?.install(); }, []);

  useEffect(() => {
    if (!native || !enabled) return;
    const service = createReleaseUpdater(setState, {
      checkUpdate: () => checkUpdate({ timeout: 15_000 }),
      relaunch,
      beforeInstall: async () => { await prepare.current?.(); },
    });
    checker.current = service;
    const onWake = () => check();
    const onVisible = () => { if (document.visibilityState === "visible") check(); };
    window.addEventListener("focus", onWake);
    window.addEventListener("online", onWake);
    document.addEventListener("visibilitychange", onVisible);
    check();
    return () => {
      service.dispose();
      checker.current = null;
      window.removeEventListener("focus", onWake);
      window.removeEventListener("online", onWake);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [check, enabled, native]);

  return { ...state, check, install, native, hasUpdate: state.latestVersion !== null };
}
