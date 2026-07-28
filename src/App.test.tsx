import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { beforeEach, expect, test, vi } from "vitest";
import App from "./App";
import * as KeypadModule from "./Keypad";
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
            { id: "DIGIT_3", label: "3" },
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

function deferred() {
  let resolve!: () => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<void>((resolvePromise, rejectPromise) => {
    resolve = () => resolvePromise();
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  vi.clearAllMocks();
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
  onRuntimeEvent = undefined;
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

test("edits the active layout and saves the staged models", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Edit layout" }));

  const columns = screen.getByLabelText("Columns for top");
  await user.clear(columns);
  await user.type(columns, "5");
  expect(screen.getByLabelText("Button ID BACK_OUT")).toHaveValue("BACK_OUT");
  expect(screen.getByLabelText("Button ID BACK_OUT")).toHaveAttribute("readonly");
  const label = screen.getByLabelText("Label for BACK_OUT");
  await user.clear(label);
  await user.type(label, "GO BACK");
  await user.click(screen.getByRole("button", { name: "Move BACK_OUT up" }));
  await user.click(screen.getByRole("button", { name: "Apply layout" }));

  expect(screen.getByRole("button", { name: "Configure GO BACK" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Save workspace" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_workspace", {
    activeModel: snapshot.activeModel,
    ioMaps: snapshot.ioMaps,
    actions: snapshot.actions,
    models: [{
      ...snapshot.models[0],
      groups: [{
        ...snapshot.models[0].groups[0],
        columns: 5,
        buttons: [
          snapshot.models[0].groups[0].buttons[0],
          { id: "BACK_OUT", label: "GO BACK" },
          snapshot.models[0].groups[0].buttons[1],
          snapshot.models[0].groups[0].buttons[3],
        ],
      }, ...snapshot.models[0].groups.slice(1)],
    }],
  }));
});

test("rejects a duplicate normalized new button ID", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Edit layout" }));
  await user.click(screen.getByRole("button", { name: "Add button to top" }));
  await user.type(screen.getByLabelText("New button ID"), "back out");
  await user.type(screen.getByLabelText("Label for new button"), "Duplicate");

  expect(screen.getByRole("alert")).toHaveTextContent("Button IDs must be unique");
  expect(screen.getByRole("button", { name: "Apply layout" })).toBeDisabled();
});

test("removes deleted buttons only from the active model IO map", async () => {
  const user = userEvent.setup();
  const otherModel = {
    id: "other-phone",
    name: "Other Phone",
    groups: [{
      id: "keys",
      columns: 1,
      buttons: [{ id: "OTHER", label: "Other" }],
    }],
  };
  const models = [...snapshot.models, otherModel];
  const ioMaps = {
    "red-phone-v1": { 5: "DIGIT_3", 6: "DIGIT_2" },
    "other-phone": { 4: "OTHER" },
  };
  vi.mocked(invoke).mockResolvedValueOnce({ ...snapshot, models, ioMaps });
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Edit layout" }));
  await user.click(screen.getByRole("button", { name: "Delete DIGIT_2" }));
  await user.click(screen.getByRole("button", { name: "Apply layout" }));
  await user.click(screen.getByRole("button", { name: "Save workspace" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_workspace", {
    activeModel: snapshot.activeModel,
    ioMaps: {
      "red-phone-v1": { 5: "DIGIT_3" },
      "other-phone": { 4: "OTHER" },
    },
    actions: snapshot.actions,
    models: [{
      ...snapshot.models[0],
      groups: snapshot.models[0].groups.map((group) => ({
        ...group,
        buttons: group.buttons.filter((button) => button.id !== "DIGIT_2"),
      })),
    }, otherModel],
  }));
});

test("preserves existing layout IDs byte for byte", async () => {
  const user = userEvent.setup();
  const whitespaceLayout = {
    ...snapshot.models[0],
    groups: [{
      id: " media ",
      columns: 1,
      buttons: [{ id: "PLAY PAUSE ", label: "PLAY/PAUSE" }],
    }],
  };
  vi.mocked(invoke).mockResolvedValueOnce({
    ...snapshot,
    models: [whitespaceLayout],
    ioMaps: { "red-phone-v1": { 6: "PLAY PAUSE " } },
    actions: {},
  });
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Edit layout" }));
  const columns = screen.getByLabelText("Columns for media");
  await user.clear(columns);
  await user.type(columns, "2");
  await user.click(screen.getByRole("button", { name: "Apply layout" }));
  await user.click(screen.getByRole("button", { name: "Save workspace" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_workspace", {
    activeModel: snapshot.activeModel,
    ioMaps: { "red-phone-v1": { 6: "PLAY PAUSE " } },
    actions: {},
    models: [{
      ...whitespaceLayout,
      groups: [{ ...whitespaceLayout.groups[0], columns: 2 }],
    }],
  }));
});

test("does not save or reset a local layout draft on Command+S", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockResolvedValueOnce({ ...snapshot, activeModel: "missing-model" });
  render(<App />);
  await user.selectOptions(
    await screen.findByRole("combobox", { name: "Device model" }),
    "red-phone-v1",
  );
  await user.click(screen.getByRole("button", { name: "Edit layout" }));
  const label = screen.getByLabelText("Label for BACK_OUT");
  await user.clear(label);
  await user.type(label, "LOCAL DRAFT");

  expect(screen.getByRole("button", { name: "Save workspace" })).toBeDisabled();
  act(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", metaKey: true })));

  expect(invoke).not.toHaveBeenCalledWith("save_workspace", expect.anything());
  expect(screen.getByLabelText("Label for BACK_OUT")).toHaveValue("LOCAL DRAFT");
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

  expect(tooltipId).toBe("key-summary-red-phone-v1-0-2");
  expect(document.getElementById(tooltipId!)).toHaveAttribute("role", "tooltip");
});

test("uses a DOM-safe tooltip ID when the button ID contains whitespace", async () => {
  vi.mocked(invoke).mockResolvedValueOnce({
    ...snapshot,
    models: [{
      ...snapshot.models[0],
      groups: [{
        id: "media",
        columns: 1,
        buttons: [{ id: "PLAY PAUSE", label: "PLAY/PAUSE" }],
      }],
    }],
    ioMaps: { "red-phone-v1": {} },
    actions: {},
  });
  render(<App />);
  const button = await screen.findByRole("button", { name: "Configure PLAY/PAUSE" });
  const tooltip = screen.getByRole("tooltip");

  expect(button).toHaveAttribute("aria-describedby", tooltip.id);
  expect(tooltip.id).toBe("key-summary-red-phone-v1-0-0");
  expect(tooltip.id).not.toMatch(/\s/);
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

test("configures multiline paste text and saves the staged action", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Behavior" }));
  await user.click(screen.getByRole("button", { name: "Configure 2" }));
  await user.selectOptions(screen.getByLabelText("Action type for 2"), "paste");
  const text = screen.getByLabelText("Paste text for 2");
  await user.clear(text);
  await user.type(text, "你好{enter}second line");
  await user.click(screen.getByRole("button", { name: "Apply behavior" }));

  expect(screen.getByRole("tooltip", { name: "你好 second line" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Save workspace" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_workspace", {
    activeModel: snapshot.activeModel,
    ioMaps: snapshot.ioMaps,
    actions: {
      ...snapshot.actions,
      DIGIT_2: { type: "paste", text: "你好\nsecond line" },
    },
    models: snapshot.models,
  }));
});

test("records a shortcut and saves backend-compatible keys", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Behavior" }));
  const digitTwo = screen.getByRole("button", { name: "Configure 2" });
  await user.click(digitTwo);
  await user.selectOptions(screen.getByLabelText("Action type for 2"), "hotkey");
  await user.click(screen.getByRole("button", { name: "Record shortcut" }));

  act(() => window.dispatchEvent(new KeyboardEvent("keydown", {
    code: "MetaLeft",
    key: "Meta",
    metaKey: true,
  })));
  expect(screen.getByLabelText("Shortcut for 2")).toHaveTextContent("Press shortcut");
  act(() => window.dispatchEvent(new KeyboardEvent("keydown", {
    code: "KeyK",
    key: "k",
    metaKey: true,
    shiftKey: true,
  })));

  expect(screen.getByLabelText("Shortcut for 2")).toHaveTextContent("Command + Shift + K");
  await user.click(screen.getByRole("button", { name: "Apply behavior" }));
  expect(document.getElementById(digitTwo.getAttribute("aria-describedby")!))
    .toHaveTextContent("Command + Shift + K");
  await user.click(screen.getByRole("button", { name: "Save workspace" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_workspace", {
    activeModel: snapshot.activeModel,
    ioMaps: snapshot.ioMaps,
    actions: {
      ...snapshot.actions,
      DIGIT_2: { type: "hotkey", keys: ["cmd", "shift", "k"] },
    },
    models: snapshot.models,
  }));
});

test("rejects an unsupported recorded shortcut", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Behavior" }));
  await user.click(screen.getByRole("button", { name: "Configure 3" }));
  await user.selectOptions(screen.getByLabelText("Action type for 3"), "hotkey");
  await user.click(screen.getByRole("button", { name: "Record shortcut" }));
  act(() => window.dispatchEvent(new KeyboardEvent("keydown", { code: "NumpadAdd" })));

  expect(screen.getByRole("alert")).toHaveTextContent("Unsupported shortcut key: NumpadAdd");
  expect(screen.getByRole("button", { name: "Apply behavior" })).toBeDisabled();
});

test("deletes only the selected button action", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Behavior" }));
  const digitTwo = screen.getByRole("button", { name: "Configure 2" });
  await user.click(digitTwo);
  await user.click(screen.getByRole("button", { name: "Delete behavior" }));
  expect(document.getElementById(digitTwo.getAttribute("aria-describedby")!))
    .toHaveTextContent("No action");

  await user.click(screen.getByRole("button", { name: "Save workspace" }));
  const { DIGIT_2: _deleted, ...remainingActions } = snapshot.actions;
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_workspace", {
    activeModel: snapshot.activeModel,
    ioMaps: snapshot.ioMaps,
    actions: remainingActions,
    models: snapshot.models,
  }));
});

test("shortcut recording captures Command+S and cleans up on cancel", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Behavior" }));
  await user.click(screen.getByRole("button", { name: "Configure 2" }));
  const text = screen.getByLabelText("Paste text for 2");
  await user.clear(text);
  await user.type(text, "dirty");
  await user.click(screen.getByRole("button", { name: "Apply behavior" }));
  await user.click(screen.getByRole("button", { name: "Configure 3" }));
  await user.selectOptions(screen.getByLabelText("Action type for 3"), "hotkey");
  await user.click(screen.getByRole("button", { name: "Record shortcut" }));
  act(() => window.dispatchEvent(new KeyboardEvent("keydown", {
    code: "KeyS",
    key: "s",
    metaKey: true,
  })));

  expect(screen.getByLabelText("Shortcut for 3")).toHaveTextContent("Command + S");
  expect(invoke).not.toHaveBeenCalledWith("save_workspace", expect.anything());
  await user.click(screen.getByRole("button", { name: "Record shortcut" }));
  await user.click(screen.getByRole("button", { name: "Cancel behavior" }));
  window.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyS", key: "s", metaKey: true }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_workspace", expect.anything()));
});

test("binds the selected button from only the next physical press", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 2" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: true }),
  );

  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 1,
    level: "info",
    message: "GPIO7: captured",
    gpio: 7,
    connection: { state: "connected", port: "/dev/cu.test" },
  } }));
  expect(screen.getByLabelText("GPIO for 2")).toHaveValue("7");

  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 2,
    level: "info",
    message: "GPIO5: normal press",
    gpio: 5,
    connection: { state: "connected", port: "/dev/cu.test" },
  } }));
  expect(screen.getByLabelText("GPIO for 2")).toHaveValue("7");
});

test("ignores GPIO until capture enable is acknowledged", async () => {
  const user = userEvent.setup();
  const enable = deferred();
  vi.mocked(invoke).mockImplementation(async (command, arguments_) => {
    if (command === "set_io_capture" && (arguments_ as { enabled: boolean }).enabled) {
      return enable.promise;
    }
    return snapshot;
  });
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 2" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: true }),
  );

  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 1,
    level: "info",
    message: "GPIO7: before acknowledgement",
    gpio: 7,
    connection: { state: "connected", port: "/dev/cu.test" },
  } }));
  expect(screen.getByLabelText("GPIO for 2")).toHaveValue("6");

  await act(async () => enable.resolve());
  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 2,
    level: "info",
    message: "GPIO7: captured",
    gpio: 7,
    connection: { state: "connected", port: "/dev/cu.test" },
  } }));
  expect(screen.getByLabelText("GPIO for 2")).toHaveValue("7");
});

test("keeps capture inactive when enable fails", async () => {
  const user = userEvent.setup();
  const enable = deferred();
  vi.mocked(invoke).mockImplementation(async (command, arguments_) => {
    if (command === "set_io_capture" && (arguments_ as { enabled: boolean }).enabled) {
      return enable.promise;
    }
    return snapshot;
  });
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 3" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: true }),
  );

  await act(async () => enable.reject(new Error("capture unavailable")));
  expect(await screen.findByRole("alert")).toHaveTextContent("capture unavailable");
  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 1,
    level: "info",
    message: "GPIO5: normal press",
    gpio: 5,
    connection: { state: "connected", port: "/dev/cu.test" },
  } }));
  expect(screen.getByLabelText("GPIO for 3")).toHaveValue("");

  await user.click(screen.getByRole("button", { name: "Cancel IO mapping" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: false }),
  );
});

test("serializes rapid capture cancel and reselection", async () => {
  const user = userEvent.setup();
  const firstEnable = deferred();
  const disable = deferred();
  const secondEnable = deferred();
  const captureCalls: boolean[] = [];
  let enableCount = 0;
  vi.mocked(invoke).mockImplementation(async (command, arguments_) => {
    if (command !== "set_io_capture") return snapshot;
    const enabled = (arguments_ as { enabled: boolean }).enabled;
    captureCalls.push(enabled);
    if (!enabled) return disable.promise;
    enableCount += 1;
    return enableCount === 1 ? firstEnable.promise : secondEnable.promise;
  });
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 2" }));
  await waitFor(() => expect(captureCalls).toEqual([true]));
  await user.click(screen.getByRole("button", { name: "Cancel IO mapping" }));
  await user.click(screen.getByRole("button", { name: "Configure 3" }));

  expect(captureCalls).toEqual([true]);
  await act(async () => firstEnable.resolve());
  await waitFor(() => expect(captureCalls).toEqual([true, false]));
  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 1,
    level: "info",
    message: "GPIO4: stale capture",
    gpio: 4,
    connection: { state: "connected", port: "/dev/cu.test" },
  } }));
  expect(screen.getByLabelText("GPIO for 3")).toHaveValue("");

  await act(async () => disable.resolve());
  await waitFor(() => expect(captureCalls).toEqual([true, false, true]));
  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 2,
    level: "info",
    message: "GPIO5: before second acknowledgement",
    gpio: 5,
    connection: { state: "connected", port: "/dev/cu.test" },
  } }));
  expect(screen.getByLabelText("GPIO for 3")).toHaveValue("");

  await act(async () => secondEnable.resolve());
  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 3,
    level: "info",
    message: "GPIO5: captured",
    gpio: 5,
    connection: { state: "connected", port: "/dev/cu.test" },
  } }));
  expect(screen.getByLabelText("GPIO for 3")).toHaveValue("5");
});

test("rejects a GPIO already assigned to another button", async () => {
  const user = userEvent.setup();
  render(<App />);
  const button = await screen.findByRole("button", { name: "Configure 3" });
  await user.click(button);
  await user.selectOptions(screen.getByLabelText("GPIO for 3"), "6");

  expect(screen.getByRole("alert")).toHaveTextContent("GPIO6 is assigned to 2");
  expect(screen.getByRole("button", { name: "Apply IO mapping" })).toBeDisabled();
  expect(document.getElementById(button.getAttribute("aria-describedby")!))
    .toHaveTextContent("Unmapped");
});

test("rebinds manually and saves the staged IO map", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 2" }));
  await user.selectOptions(screen.getByLabelText("GPIO for 2"), "5");
  await user.click(screen.getByRole("button", { name: "Apply IO mapping" }));

  expect(screen.getByRole("tooltip", { name: "GPIO 5" })).toBeInTheDocument();
  expect(screen.queryByRole("tooltip", { name: "GPIO 6" })).not.toBeInTheDocument();
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: false }),
  );

  await user.click(screen.getByRole("button", { name: "Save workspace" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_workspace", {
      activeModel: snapshot.activeModel,
      ioMaps: { "red-phone-v1": { 5: "DIGIT_2" } },
      actions: snapshot.actions,
      models: snapshot.models,
    }),
  );
});

test("stops IO capture when the popover is cancelled", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 2" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: true }),
  );

  await user.click(screen.getByRole("button", { name: "Cancel IO mapping" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: false }),
  );
  expect(screen.queryByLabelText("GPIO for 2")).not.toBeInTheDocument();
});

test("restarts IO capture when the selected button changes", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 2" }));
  await user.click(screen.getByRole("button", { name: "Configure 3" }));

  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: false });
    expect(vi.mocked(invoke).mock.calls.filter(([command, args]) =>
      command === "set_io_capture" && (args as { enabled: boolean }).enabled
    )).toHaveLength(2);
  });
  expect(screen.getByLabelText("GPIO for 3")).toBeInTheDocument();
});

test("stops IO capture when the configuration mode changes", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 2" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: true }),
  );

  await user.click(screen.getByRole("button", { name: "Behavior" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: false }),
  );
  expect(screen.queryByLabelText("GPIO for 2")).not.toBeInTheDocument();
});

test("positions the IO popover beside its anchor and clamps both axes", () => {
  type Position = (
    anchor: Pick<DOMRect, "left" | "right" | "top">,
    width: number,
    height: number,
    viewportWidth: number,
    viewportHeight: number,
  ) => { left: number; top: number };
  const position = (KeypadModule as { popoverPosition?: Position }).popoverPosition;

  expect(position).toBeTypeOf("function");
  expect(position!({ left: 100, right: 150, top: 30 }, 200, 100, 500, 400))
    .toEqual({ left: 162, top: 30 });
  expect(position!({ left: 400, right: 450, top: 390 }, 200, 100, 500, 400))
    .toEqual({ left: 188, top: 288 });
  expect(position!({ left: 8, right: 190, top: -10 }, 180, 100, 200, 400))
    .toEqual({ left: 12, top: 12 });
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

test("stops IO capture on unmount", async () => {
  const user = userEvent.setup();
  const view = render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 2" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: true }),
  );

  view.unmount();

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: false }),
  );
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
