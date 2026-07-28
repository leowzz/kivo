import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { beforeEach, expect, test, vi } from "vitest";
import App from "./App";
import type { AppSnapshot, RuntimeEvent } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const snapshot = {
  models: [
    {
      id: "red-phone-v1",
      name: "Red Phone v1",
      groups: [
        {
          id: "top",
          columns: 4,
          buttons: [
            { id: "UP", label: "UP" },
            { id: "DOWN", label: "DOWN" },
            { id: "BACK_OUT", label: "BACK/OUT" },
            { id: "DEL", label: "DEL" },
          ],
        },
        {
          id: "digits",
          columns: 3,
          buttons: [
            { id: "DIGIT_1", label: "1" },
            { id: "DIGIT_2", label: "2" },
          ],
        },
        {
          id: "bottom",
          columns: 5,
          buttons: [
            { id: "R", label: "R" },
            { id: "VOL", label: "VOL" },
          ],
        },
      ],
    },
  ],
  activeModel: "red-phone-v1",
  ioMaps: { "red-phone-v1": { 6: "DIGIT_2" } },
  actions: {
    UP: { type: "hotkey", keys: ["cmd", "shift", "k"] },
    DOWN: { type: "hotkey", keys: ["option", "page_up"] },
    BACK_OUT: { type: "paste", text: "This behavior preview is intentionally long" },
    DIGIT_2: { type: "paste", text: "six" },
  },
  supportedGpios: [0, 1, 2, 3, 4, 5, 6],
  configPath: "/tmp/vibe-tool/config.yaml",
  connection: { state: "searching", port: null },
  configError: null,
} satisfies AppSnapshot;

let onRuntimeEvent: ((event: { payload: RuntimeEvent }) => void) | undefined;
let unlisten: UnlistenFn;

beforeEach(() => {
  vi.clearAllMocks();
  unlisten = vi.fn();
  vi.mocked(invoke).mockImplementation(async (command, arguments_) => {
    if (command === "save_workspace") {
      return { ...snapshot, ...(arguments_ as Partial<AppSnapshot>) };
    }
    return snapshot;
  });
  vi.mocked(listen).mockImplementation(async (_event, handler) => {
    onRuntimeEvent = handler as (event: { payload: RuntimeEvent }) => void;
    return unlisten;
  });
});

test("renders the selected model as normalized groups", async () => {
  render(<App />);
  const backOut = await screen.findByRole("button", { name: "Configure BACK/OUT" });
  expect(backOut).toBeVisible();
  expect(screen.queryByRole("button", { name: "Configure BACK" })).not.toBeInTheDocument();
  expect(screen.getByTestId("group-top")).toHaveStyle({
    gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
  });
  expect(screen.getByTestId("group-digits")).toHaveStyle({
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
  });
});

test("associates a keypad button with its summary tooltip", async () => {
  render(<App />);
  const backOut = await screen.findByRole("button", { name: "Configure BACK/OUT" });
  const tooltipId = backOut.getAttribute("aria-describedby");

  expect(tooltipId).toBe("key-summary-red-phone-v1-BACK_OUT");
  expect(document.getElementById(tooltipId!)).toHaveAttribute("role", "tooltip");
});

test("shows mode summaries and selects a key", async () => {
  const user = userEvent.setup();
  render(<App />);

  const digitTwo = await screen.findByRole("button", { name: "Configure 2" });
  expect(screen.getByRole("tooltip", { name: "GPIO 6" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Behavior" }));
  expect(screen.getByRole("tooltip", { name: "six" })).toBeInTheDocument();
  expect(screen.getByRole("tooltip", { name: "Command + Shift + K" })).toBeInTheDocument();
  expect(screen.getByRole("tooltip", { name: "Option + Page Up" })).toBeInTheDocument();
  expect(screen.getByRole("tooltip", { name: /This behavior preview.*\.\.\./ })).toBeInTheDocument();
  await user.click(digitTwo);
  expect(digitTwo).toHaveClass("is-selected");
});

test("recovers an invalid active model and saves the selected catalog model", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockResolvedValueOnce({
    ...snapshot,
    activeModel: "missing-model",
    configError: "unknown active model missing-model",
  });
  render(<App />);

  const selector = await screen.findByRole("combobox", { name: "Device model" });
  expect(selector).toHaveValue("missing-model");
  expect(screen.getByRole("option", { name: "Missing: missing-model" })).toBeDisabled();
  expect(screen.getByRole("alert")).toHaveTextContent("unknown active model missing-model");
  expect(screen.queryByRole("button", { name: "Configure 2" })).not.toBeInTheDocument();

  await user.selectOptions(selector, "red-phone-v1");
  expect(await screen.findByRole("button", { name: "Configure 2" })).toBeVisible();
  expect(selector).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Save workspace" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_workspace", {
      activeModel: "red-phone-v1",
      ioMaps: snapshot.ioMaps,
      actions: snapshot.actions,
      models: snapshot.models,
    }),
  );
});

test("uses Command+S to save a dirty workspace", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockResolvedValueOnce({ ...snapshot, activeModel: "missing-model" });
  render(<App />);
  await user.selectOptions(
    await screen.findByRole("combobox", { name: "Device model" }),
    "red-phone-v1",
  );

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", metaKey: true }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_workspace", expect.anything()));
});

test("keeps the selected model when saving fails", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "save_workspace") throw new Error("disk full");
    return { ...snapshot, activeModel: "missing-model" };
  });
  render(<App />);
  await user.selectOptions(
    await screen.findByRole("combobox", { name: "Device model" }),
    "red-phone-v1",
  );

  await user.click(screen.getByRole("button", { name: "Save workspace" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
  expect(screen.getByRole("button", { name: "Configure 2" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Save workspace" })).toBeEnabled();
});

test("shows runtime events and current connection", async () => {
  render(<App />);
  await screen.findByRole("button", { name: "Configure 2" });

  act(() => {
    onRuntimeEvent?.({
      payload: {
        timestampMs: 1_722_222_222_000,
        level: "info",
        message: "GPIO6: PASTE 12",
        connection: { state: "connected", port: "/dev/cu.usbmodem" },
        gpio: 6,
      },
    });
  });

  expect(screen.getByText("GPIO6: PASTE 12")).toBeInTheDocument();
  expect(screen.getByText("Connected")).toBeInTheDocument();
  expect(screen.getByText("/dev/cu.usbmodem")).toBeInTheDocument();
});

test("unsubscribes from runtime events on unmount", async () => {
  const view = render(<App />);
  await screen.findByRole("button", { name: "Configure 2" });
  view.unmount();

  await waitFor(() => expect(unlisten).toHaveBeenCalledOnce());
});

test("subscribes before loading the snapshot", async () => {
  const calls: string[] = [];
  vi.mocked(listen).mockImplementation(async () => {
    calls.push("listen");
    return unlisten;
  });
  vi.mocked(invoke).mockImplementation(async () => {
    calls.push("invoke");
    return snapshot;
  });

  render(<App />);
  await screen.findByRole("button", { name: "Configure 2" });

  expect(calls.slice(0, 2)).toEqual(["listen", "invoke"]);
});
