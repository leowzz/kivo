import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { check } from "@tauri-apps/plugin-updater";
import { useReleaseUpdate } from "./useReleaseUpdate";

vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn().mockResolvedValue(undefined) }));
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  vi.clearAllMocks();
});

test("browser preview does not contact native updater", () => {
  const { result } = renderHook(() => useReleaseUpdate());
  act(() => { result.current.check(true); result.current.install(); });
  expect(check).not.toHaveBeenCalled();
  expect(result.current.native).toBe(false);
});

test("checks on startup with timeout and throttles wake events", async () => {
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  const { result } = renderHook(() => useReleaseUpdate());
  await waitFor(() => expect(result.current.checkedAt).not.toBeNull());
  expect(check).toHaveBeenCalledWith({ timeout: 15_000 });
  act(() => { window.dispatchEvent(new Event("focus")); window.dispatchEvent(new Event("online")); });
  expect(check).toHaveBeenCalledOnce();
  act(() => result.current.check(true));
  await waitFor(() => expect(check).toHaveBeenCalledTimes(2));
});

test("uses the latest save guard without restarting the service", async () => {
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  const pending = { version: "1.0.0", close: vi.fn().mockResolvedValue(undefined), downloadAndInstall: vi.fn().mockResolvedValue(undefined) };
  vi.mocked(check).mockResolvedValueOnce(pending as unknown as NonNullable<Awaited<ReturnType<typeof check>>>);
  const beforeInstall = vi.fn().mockResolvedValue(undefined);
  const blocked = vi.fn().mockRejectedValue(new Error("draft is unsaved"));
  const { result, rerender } = renderHook(({ prepare }) => useReleaseUpdate({ beforeInstall: prepare }), { initialProps: { prepare: beforeInstall } });
  await waitFor(() => expect(result.current.hasUpdate).toBe(true));
  rerender({ prepare: blocked });
  act(() => result.current.install());
  await waitFor(() => expect(result.current.error).toBe("save"));
  expect(blocked).toHaveBeenCalledOnce();
  expect(beforeInstall).not.toHaveBeenCalled();
  expect(pending.downloadAndInstall).not.toHaveBeenCalled();
  expect(check).toHaveBeenCalledOnce();
});
