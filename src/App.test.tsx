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

const snapshot: AppSnapshot = {
  buttons: { 0: "hello", 6: "six" },
  configPath: "/tmp/vibe-tool/config.yaml",
  connection: { state: "searching", port: null },
  configError: null,
};

let onRuntimeEvent: ((event: { payload: RuntimeEvent }) => void) | undefined;
let unlisten: UnlistenFn;

beforeEach(() => {
  vi.clearAllMocks();
  unlisten = vi.fn();
  vi.mocked(invoke).mockImplementation(async (command, arguments_) => {
    if (command === "save_mappings") {
      const saveArguments = arguments_ as { buttons: Record<number, string> };
      return { ...snapshot, buttons: saveArguments.buttons };
    }
    return snapshot;
  });
  vi.mocked(listen).mockImplementation(async (_event, handler) => {
    onRuntimeEvent = handler as (event: { payload: RuntimeEvent }) => void;
    return unlisten;
  });
});

test("loads edits and saves mappings", async () => {
  const user = userEvent.setup();
  render(<App />);

  const editor = await screen.findByRole("textbox", { name: "GPIO0 mapping" });
  expect(editor).toHaveValue("hello");
  expect(screen.getByText(/download mode/i)).toBeInTheDocument();
  expect(screen.getByText("/tmp/vibe-tool/config.yaml")).toBeInTheDocument();
  const save = screen.getByRole("button", { name: "Save mappings" });
  expect(save).toBeDisabled();

  await user.clear(editor);
  await user.type(editor, "你好");
  expect(save).toBeEnabled();
  await user.click(save);

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_mappings", {
      buttons: expect.objectContaining({ 0: "你好" }),
    }),
  );
  await waitFor(() => expect(save).toBeDisabled());
});

test("keeps edited text when save fails", async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "save_mappings") {
      throw new Error("disk full");
    }
    return snapshot;
  });
  render(<App />);
  const editor = await screen.findByRole("textbox", { name: "GPIO0 mapping" });

  await user.clear(editor);
  await user.type(editor, "unsaved");
  await user.click(screen.getByRole("button", { name: "Save mappings" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
  expect(editor).toHaveValue("unsaved");
  expect(screen.getByRole("button", { name: "Save mappings" })).toBeEnabled();
});

test("shows runtime events and current connection", async () => {
  render(<App />);
  await screen.findByRole("textbox", { name: "GPIO0 mapping" });

  act(() => {
    onRuntimeEvent?.({
      payload: {
        timestampMs: 1_722_222_222_000,
        level: "info",
        message: "GPIO6: PASTE 12",
        connection: { state: "connected", port: "/dev/cu.usbmodem" },
      },
    });
  });

  expect(screen.getByText("GPIO6: PASTE 12")).toBeInTheDocument();
  expect(screen.getByText("Connected")).toBeInTheDocument();
  expect(screen.getByText("/dev/cu.usbmodem")).toBeInTheDocument();
});

test("unsubscribes from runtime events on unmount", async () => {
  const view = render(<App />);
  await screen.findByRole("textbox", { name: "GPIO0 mapping" });
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
  await screen.findByRole("textbox", { name: "GPIO0 mapping" });

  expect(calls.slice(0, 2)).toEqual(["listen", "invoke"]);
});
