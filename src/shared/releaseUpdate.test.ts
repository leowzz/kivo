import { expect, test, vi } from "vitest";
import { createReleaseUpdater, type UpdateState } from "./releaseUpdate";
import type { DownloadEvent } from "@tauri-apps/plugin-updater";

function setup(overrides: Partial<Parameters<typeof createReleaseUpdater>[1]> = {}) {
  const update = { version: "0.10.0", close: vi.fn().mockResolvedValue(undefined), downloadAndInstall: vi.fn().mockResolvedValue(undefined) };
  const states: UpdateState[] = [];
  const checkUpdate = vi.fn().mockResolvedValue(update);
  const relaunch = vi.fn().mockResolvedValue(undefined);
  const beforeInstall = vi.fn().mockResolvedValue(undefined);
  const service = createReleaseUpdater(state => states.push(state), { checkUpdate, relaunch, beforeInstall, ...overrides });
  return { service, states, update, checkUpdate, relaunch, beforeInstall };
}

test("throttles wake checks, permits manual checks, and closes superseded resources", async () => {
  let now = 0;
  const { service, checkUpdate, states, update } = setup({ now: () => now });
  await service.check();
  await service.check();
  expect(checkUpdate).toHaveBeenCalledTimes(1);
  expect(states.at(-1)).toMatchObject({ latestVersion: "0.10.0", checkedAt: 0 });
  checkUpdate.mockResolvedValueOnce(null);
  await service.check(true);
  expect(states.at(-1)?.latestVersion).toBeNull();
  expect(update.close).toHaveBeenCalledOnce();
  now = 3_600_000;
  await service.check();
  expect(checkUpdate).toHaveBeenCalledTimes(3);
  service.dispose();
});

test("preserves a discovered update on failed checks and retries after one minute", async () => {
  let now = 100;
  const { service, checkUpdate, states, update } = setup({ now: () => now });
  await service.check();
  checkUpdate.mockRejectedValueOnce(new Error("network"));
  await service.check(true);
  expect(states.at(-1)).toMatchObject({ error: "check", latestVersion: "0.10.0", checkedAt: 100 });
  expect(update.close).not.toHaveBeenCalled();
  await service.check();
  expect(checkUpdate).toHaveBeenCalledTimes(2);
  now += 60_000;
  await service.check();
  expect(checkUpdate).toHaveBeenCalledTimes(3);
  service.dispose();
});

test("saves before downloading, reports progress, and prevents concurrent installs and checks", async () => {
  let finish!: () => void;
  const { service, update, checkUpdate, states, relaunch, beforeInstall } = setup();
  update.downloadAndInstall.mockImplementation(async (report: (event: DownloadEvent) => void) => {
    report({ event: "Started", data: { contentLength: 200 } });
    report({ event: "Progress", data: { chunkLength: 50 } });
    report({ event: "Progress", data: { chunkLength: 75 } });
    await new Promise<void>(resolve => { finish = resolve; });
    report({ event: "Finished" });
  });
  await service.check();
  const installing = service.install();
  await vi.waitFor(() => expect(update.downloadAndInstall).toHaveBeenCalledOnce());
  expect(beforeInstall.mock.invocationCallOrder[0]).toBeLessThan(update.downloadAndInstall.mock.invocationCallOrder[0]);
  expect(states.at(-1)).toMatchObject({ phase: "downloading", total: 200, downloaded: 125 });
  await service.install();
  await service.check(true);
  expect(checkUpdate).toHaveBeenCalledOnce();
  expect(update.downloadAndInstall).toHaveBeenCalledOnce();
  finish();
  await installing;
  expect(states.map(state => state.phase)).toContain("installing");
  expect(states.at(-1)?.phase).toBe("restarting");
  expect(relaunch).toHaveBeenCalledOnce();
  service.dispose();
});

test("does not download or restart when saving fails", async () => {
  const { service, update, relaunch, states } = setup({ beforeInstall: async () => { throw new Error("save failed"); } });
  await service.check();
  await service.install();
  expect(update.downloadAndInstall).not.toHaveBeenCalled();
  expect(relaunch).not.toHaveBeenCalled();
  expect(states.at(-1)).toMatchObject({ phase: "idle", error: "save" });
  service.dispose();
});

test("retries failed installation, and retrying restart does not reinstall", async () => {
  const { service, update, relaunch, states } = setup();
  update.downloadAndInstall.mockRejectedValueOnce(new Error("signature mismatch"));
  relaunch.mockRejectedValueOnce(new Error("restart failed"));
  await service.check();
  await service.install();
  expect(states.at(-1)).toMatchObject({ phase: "idle", error: "install" });
  expect(relaunch).not.toHaveBeenCalled();
  await service.install();
  expect(states.at(-1)).toMatchObject({ phase: "ready", error: "restart" });
  await service.install();
  expect(update.downloadAndInstall).toHaveBeenCalledTimes(2);
  expect(relaunch).toHaveBeenCalledTimes(2);
  service.dispose();
});

test("disposal closes a late check result without publishing or installing", async () => {
  let resolve!: (value: ReturnType<typeof setup>["update"]) => void;
  const { service, states, update } = setup({ checkUpdate: () => new Promise(done => { resolve = done; }) });
  const checking = service.check();
  service.dispose();
  resolve(update);
  await checking;
  await service.install();
  expect(update.close).toHaveBeenCalledOnce();
  expect(update.downloadAndInstall).not.toHaveBeenCalled();
  expect(states).toHaveLength(1);
});

test("disposal during download retains the resource until finished and avoids restart", async () => {
  let finish!: () => void;
  const { service, update, relaunch } = setup();
  update.downloadAndInstall.mockImplementation(() => new Promise<void>(resolve => { finish = resolve; }));
  await service.check();
  const installing = service.install();
  await vi.waitFor(() => expect(update.downloadAndInstall).toHaveBeenCalledOnce());
  service.dispose();
  expect(update.close).not.toHaveBeenCalled();
  finish();
  await installing;
  expect(update.close).toHaveBeenCalledOnce();
  expect(relaunch).not.toHaveBeenCalled();
});
