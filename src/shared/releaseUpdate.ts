import type { DownloadEvent, Update } from "@tauri-apps/plugin-updater";

export const releasesUrl = "https://github.com/leowzz/kivo/releases/latest";
export type UpdatePhase = "idle" | "checking" | "preparing" | "downloading" | "installing" | "ready" | "restarting";
export type UpdateState = {
  phase: UpdatePhase;
  latestVersion: string | null;
  checkedAt: number | null;
  downloaded: number;
  total: number | null;
  error: "check" | "install" | "restart" | "save" | null;
};
export const initialUpdateState: UpdateState = {
  phase: "idle", latestVersion: null, checkedAt: null,
  downloaded: 0, total: null, error: null,
};
export function updateIsBusy(phase: UpdatePhase) {
  return !["idle", "checking", "ready"].includes(phase);
}
type PendingUpdate = Pick<Update, "version" | "downloadAndInstall" | "close">;

export function createReleaseUpdater(onChange: (state: UpdateState) => void, {
  checkUpdate, relaunch, beforeInstall = async () => {}, now = Date.now,
}: {
  checkUpdate: () => Promise<PendingUpdate | null>;
  relaunch: () => Promise<void>;
  beforeInstall?: () => Promise<void>;
  now?: () => number;
}) {
  let state = { ...initialUpdateState };
  let pending: PendingUpdate | null = null;
  let disposed = false;
  let installing = false;
  let nextCheckAt = 0;
  const publish = () => { if (!disposed) onChange({ ...state }); };
  const close = (update: PendingUpdate | null) => { void update?.close().catch(() => {}); };

  return {
    async check(force = false) {
      if (disposed || state.phase !== "idle" || (!force && now() < nextCheckAt)) return;
      state = { ...state, phase: "checking", error: null };
      publish();
      try {
        const update = await checkUpdate();
        if (disposed) { close(update); return; }
        close(pending);
        pending = update;
        state = { ...state, latestVersion: update?.version ?? null, checkedAt: now() };
        nextCheckAt = now() + 60 * 60 * 1000;
      } catch {
        // Keep a discovered update available through transient network failures.
        state = { ...state, error: "check" };
        nextCheckAt = now() + 60 * 1000;
      } finally {
        state = { ...state, phase: "idle" };
        publish();
      }
    },
    async install() {
      if (disposed || installing || !["idle", "ready"].includes(state.phase)) return;
      if (state.phase === "idle" && !pending) return;
      installing = true;
      const alreadyInstalled = state.phase === "ready";
      state = { ...state, phase: "preparing", error: null };
      publish();
      try {
        try {
          await beforeInstall();
        } catch {
          state = { ...state, phase: alreadyInstalled ? "ready" : "idle", error: "save" };
          publish();
          return;
        }
        if (disposed) return;
        if (!alreadyInstalled) {
          state = { ...state, phase: "downloading", downloaded: 0, total: null };
          publish();
          await pending!.downloadAndInstall((event: DownloadEvent) => {
            if (event.event === "Started") state = { ...state, total: event.data.contentLength ?? null };
            if (event.event === "Progress") state = { ...state, downloaded: state.downloaded + event.data.chunkLength };
            if (event.event === "Finished") state = { ...state, phase: "installing" };
            publish();
          });
          state = { ...state, phase: "ready" };
        }
        if (disposed) return;
        state = { ...state, phase: "restarting" };
        publish();
        await relaunch();
      } catch {
        const installed = state.phase === "restarting";
        state = { ...state, phase: installed ? "ready" : "idle", error: installed ? "restart" : "install" };
        publish();
      } finally {
        installing = false;
        if (disposed) { close(pending); pending = null; }
      }
    },
    dispose() {
      disposed = true;
      if (!installing) { close(pending); pending = null; }
    },
  };
}
