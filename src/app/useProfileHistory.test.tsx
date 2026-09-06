import { act, renderHook } from "@testing-library/react";
import { expect, test } from "vitest";
import { useProductConfigHistory, useProfileHistory } from "./useProfileHistory";
import type { DeviceProfile, ProductConfigurationProfile } from "./types";

function profile(id: string, name: string): DeviceProfile {
  return {
    schema_version: 3,
    profile: { id, name, groups: [] },
    trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
    hardware_profiles: [],
    actions: {},
  };
}

function nameOf(result: { current: ReturnType<typeof useProfileHistory> }, id: string) {
  return result.current.get(id)?.profile.name;
}

function config(version: string, text: string): ProductConfigurationProfile {
  return {
    id: `configuration-${text}`,
    name: text,
    product_version_id: version,
    trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
    actions: {
      ONE: { press: [{ type: "paste", text }], release: [], long_press: [], double_press: [] },
    },
  };
}

test("keeps immutable snapshots and independent timelines per profile", () => {
  const initialA = profile("a", "A0");
  const initialB = profile("b", "B0");
  const { result } = renderHook(() => useProfileHistory([initialA, initialB]));

  act(() => {
    result.current.update("a", (current) => ({
      ...current,
      profile: { ...current.profile, name: "A1" },
    }));
  });
  act(() => {
    result.current.update("b", (current) => ({
      ...current,
      profile: { ...current.profile, name: "B1" },
    }));
  });

  expect(nameOf(result, "a")).toBe("A1");
  expect(nameOf(result, "b")).toBe("B1");
  expect(result.current.canUndo("a")).toBe(true);
  expect(result.current.canUndo("b")).toBe(true);

  const exposed = result.current.get("a");
  exposed!.profile.name = "mutated outside history";
  expect(nameOf(result, "a")).toBe("A1");

  initialA.profile.name = "mutated initial input";
  act(() => { result.current.undo("a"); });
  expect(nameOf(result, "a")).toBe("A0");
  expect(nameOf(result, "b")).toBe("B1");
});

test("records updates without adjacent duplicates and clears redo on a new branch", () => {
  const { result } = renderHook(() => useProfileHistory([profile("a", "A0")]));

  act(() => { result.current.record(profile("a", "A0")); });
  expect(result.current.canUndo("a")).toBe(false);

  act(() => { result.current.update("a", (current) => ({ ...current })); });
  expect(result.current.canUndo("a")).toBe(false);

  act(() => {
    result.current.update("a", (current) => ({
      ...current,
      profile: { ...current.profile, name: "A1" },
    }));
  });
  act(() => { result.current.undo("a"); });
  expect(nameOf(result, "a")).toBe("A0");
  expect(result.current.canRedo("a")).toBe(true);

  act(() => {
    result.current.update("a", (current) => ({
      ...current,
      profile: { ...current.profile, name: "A2" },
    }));
  });
  expect(nameOf(result, "a")).toBe("A2");
  expect(result.current.canRedo("a")).toBe(false);
  act(() => { result.current.undo("a"); });
  expect(nameOf(result, "a")).toBe("A0");
});

test("bounds each timeline by total snapshot count", () => {
  const { result } = renderHook(() => useProfileHistory([profile("a", "A0")], { maxSnapshots: 3 }));

  act(() => { result.current.record(profile("a", "A1")); });
  act(() => { result.current.record(profile("a", "A2")); });
  act(() => { result.current.record(profile("a", "A3")); });

  expect(nameOf(result, "a")).toBe("A3");
  act(() => { result.current.undo("a"); });
  expect(nameOf(result, "a")).toBe("A2");
  act(() => { result.current.undo("a"); });
  expect(nameOf(result, "a")).toBe("A1");
  expect(result.current.canUndo("a")).toBe(false);

  act(() => { result.current.redo("a"); });
  act(() => { result.current.redo("a"); });
  expect(nameOf(result, "a")).toBe("A3");
  expect(result.current.canRedo("a")).toBe(false);
});

test("supports a one-snapshot timeline with no undo entries", () => {
  const { result } = renderHook(() => useProfileHistory([profile("a", "A0")], { maxSnapshots: 1 }));

  act(() => { result.current.record(profile("a", "A1")); });
  expect(nameOf(result, "a")).toBe("A1");
  expect(result.current.canUndo("a")).toBe(false);
  expect(result.current.undo("a")).toBeUndefined();
});

test("clear keeps current snapshots while reset installs authoritative snapshots", () => {
  const { result } = renderHook(() => useProfileHistory([
    profile("a", "A0"),
    profile("b", "B0"),
  ]));

  act(() => { result.current.record(profile("a", "A1")); });
  act(() => { result.current.record(profile("b", "B1")); });
  act(() => { result.current.clear("b"); });
  expect(nameOf(result, "b")).toBe("B1");
  expect(result.current.canUndo("b")).toBe(false);
  expect(result.current.canUndo("a")).toBe(true);

  const authoritative = profile("a", "server");
  act(() => { result.current.reset([authoritative, profile("c", "C0")]); });
  authoritative.profile.name = "mutated authoritative input";
  expect(result.current.profiles.map((item) => item.profile.id)).toEqual(["a", "c"]);
  expect(nameOf(result, "a")).toBe("server");
  expect(nameOf(result, "b")).toBeUndefined();
  expect(result.current.canUndo("a")).toBe(false);
  expect(result.current.canRedo("a")).toBe(false);
  expect(result.current.canUndo("c")).toBe(false);

  act(() => { result.current.record(profile("c", "C1")); });
  act(() => { result.current.clear(); });
  expect(result.current.canUndo("c")).toBe(false);
});

test("sync preserves unchanged profile timelines while resetting changed or new profiles", () => {
  const { result } = renderHook(() => useProfileHistory([
    profile("a", "A0"),
    profile("b", "B0"),
  ]));

  act(() => { result.current.record(profile("a", "A1")); });
  act(() => {
    result.current.sync([
      profile("a", "A1"),
      profile("b", "B1"),
      profile("c", "C0"),
    ]);
  });

  expect(nameOf(result, "a")).toBe("A1");
  expect(result.current.canUndo("a")).toBe(true);
  expect(result.current.canUndo("b")).toBe(false);
  expect(result.current.canUndo("c")).toBe(false);

  act(() => { result.current.sync([profile("b", "B1"), profile("a", "A1")]); });
  expect(result.current.profiles.map((item) => item.profile.id)).toEqual(["b", "a"]);
  expect(result.current.canUndo("a")).toBe(true);
  expect(result.current.canUndo("b")).toBe(false);
  expect(result.current.get("c")).toBeUndefined();
});

test("rejects a non-positive snapshot bound", () => {
  expect(() => renderHook(() => useProfileHistory([], { maxSnapshots: 0 }))).toThrow(
    "maxSnapshots must be a positive integer",
  );
});

test("keeps product config histories isolated by configuration and preserves immutable snapshots", () => {
  const initialA = config("product", "A0");
  const initialB = config("product", "B0");
  const { result } = renderHook(() => useProductConfigHistory([
    { configurationId: "device-a", config: initialA },
    { configurationId: "device-b", config: initialB },
  ]));

  act(() => { result.current.record("device-a", config("product", "A1")); });
  expect(result.current.get("device-a")?.actions.ONE.press[0]).toEqual({ type: "paste", text: "A1" });
  expect(result.current.get("device-b")?.actions.ONE.press[0]).toEqual({ type: "paste", text: "B0" });
  expect(result.current.canUndo("device-a")).toBe(true);
  expect(result.current.canUndo("device-b")).toBe(false);

  const exposed = result.current.get("device-a");
  exposed!.actions.ONE.press[0] = { type: "paste", text: "mutated outside history" };
  expect(result.current.get("device-a")?.actions.ONE.press[0]).toEqual({ type: "paste", text: "A1" });

  act(() => { result.current.undo("device-a"); });
  expect(result.current.get("device-a")?.actions.ONE.press[0]).toEqual({ type: "paste", text: "A0" });
  expect(result.current.canRedo("device-a")).toBe(true);
});

test("sync preserves unchanged product timelines while resetting changed or removed configurations", () => {
  const { result } = renderHook(() => useProductConfigHistory([
    { configurationId: "device-a", config: config("product", "A0") },
    { configurationId: "device-b", config: config("product", "B0") },
  ]));

  act(() => { result.current.record("device-a", config("product", "A1")); });
  act(() => {
    result.current.sync([
      { configurationId: "device-a", config: config("product", "A1") },
      { configurationId: "device-b", config: config("product", "B1") },
      { configurationId: "device-c", config: config("product", "C0") },
    ]);
  });

  expect(result.current.canUndo("device-a")).toBe(true);
  expect(result.current.canUndo("device-b")).toBe(false);
  expect(result.current.canUndo("device-c")).toBe(false);

  act(() => { result.current.sync([{ configurationId: "device-b", config: config("product", "B1") }]); });
  expect(result.current.get("device-a")).toBeUndefined();
  expect(result.current.get("device-b")?.actions.ONE.press[0]).toEqual({ type: "paste", text: "B1" });
});
