import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test, vi } from "vitest";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { ActionDialog } from "./ActionDialog";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({ getCurrentWebviewWindow: vi.fn() }));
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks(); });

test("normalizes a website on save and rejects non-web schemes and credentials", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  render(<ActionDialog open mode="create" language="en-US" initial={{ trigger: "press", action: { type: "open_website", url: "" } }} onSave={onSave} onCancel={vi.fn()} />);
  const input = screen.getByLabelText("Website address");
  for (const invalid of ["javascript:alert(1)", "file:///tmp/test", "https://user:pass@example.com", "https://example.com/a b"]) {
    await user.clear(input);
    await user.type(input, invalid);
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText("Enter a valid HTTP or HTTPS website address")).toBeInTheDocument();
  }
  await user.clear(input);
  await user.type(input, " example.com/path?a=1&b=2 ");
  await user.selectOptions(screen.getByLabelText("Trigger"), "double_press");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(onSave).toHaveBeenCalledWith({ trigger: "double_press", action: { type: "open_website", url: "https://example.com/path?a=1&b=2" } });
});

test("requires an absolute app path and removes surrounding quotes on save", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  render(<ActionDialog open mode="create" language="en-US" initial={{ trigger: "press", action: { type: "open_app", path: "App.exe" } }} onSave={onSave} onCancel={vi.fn()} />);
  expect(screen.getByRole("button", { name: "Choose app" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(onSave).not.toHaveBeenCalled();
  await user.clear(screen.getByLabelText("Application path"));
  await user.type(screen.getByLabelText("Application path"), ' "C:\\Program Files\\My App\\App.exe" ');
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(onSave).toHaveBeenCalledWith({ trigger: "press", action: { type: "open_app", path: "C:\\Program Files\\My App\\App.exe" } });
});

test("chooses an app without committing and ignores a picker result after closing", async () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  vi.mocked(getCurrentWebviewWindow).mockReturnValue({ onDragDropEvent: vi.fn().mockResolvedValue(vi.fn()) } as unknown as ReturnType<typeof getCurrentWebviewWindow>);
  const user = userEvent.setup();
  const onSave = vi.fn();
  const props = { mode: "create" as const, language: "en-US" as const, initial: { trigger: "press" as const, action: { type: "open_app" as const, path: "" } }, onSave, onCancel: vi.fn() };
  const { rerender } = render(<ActionDialog {...props} open />);
  vi.mocked(open).mockResolvedValueOnce("/Applications/Test & Space.app");
  await user.click(screen.getByRole("button", { name: "Choose app" }));
  expect(screen.getByLabelText("Application path")).toHaveValue("/Applications/Test & Space.app");
  expect(onSave).not.toHaveBeenCalled();
  let resolvePicker!: (path: string) => void;
  vi.mocked(open).mockReturnValueOnce(new Promise((resolve) => { resolvePicker = resolve; }));
  await user.click(screen.getByRole("button", { name: "Choose app" }));
  rerender(<ActionDialog {...props} open={false} />);
  rerender(<ActionDialog {...props} open />);
  await act(async () => { resolvePicker("/Applications/Stale.app"); });
  expect(screen.getByLabelText("Application path")).toHaveValue("");
});

test("native drops validate one app inside the dialog and clean up the listener", async () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  type DropEvent = { payload: { type: "drop"; paths: string[]; position: { x: number; y: number } } };
  let listener!: (event: DropEvent) => void;
  const stop = vi.fn();
  vi.mocked(getCurrentWebviewWindow).mockReturnValue({ onDragDropEvent: vi.fn((callback) => { listener = callback; return Promise.resolve(stop); }) } as unknown as ReturnType<typeof getCurrentWebviewWindow>);
  const onSave = vi.fn();
  const { unmount } = render(<ActionDialog open mode="create" language="en-US" initial={{ trigger: "press", action: { type: "open_app", path: "" } }} onSave={onSave} onCancel={vi.fn()} />);
  vi.spyOn(screen.getByRole("dialog"), "getBoundingClientRect").mockReturnValue({ left: 0, top: 0, right: 100, bottom: 100 } as DOMRect);
  await act(async () => {
    listener({ payload: { type: "drop", paths: ["/Applications/Test.app"], position: { x: 200, y: 200 } } });
  });
  expect(screen.getByLabelText("Application path")).toHaveValue("");
  await act(async () => {
    listener({ payload: { type: "drop", paths: ["/Applications/Test.app", "/Applications/Other.app"], position: { x: 50, y: 50 } } });
  });
  expect(screen.getByText("Drag one app or shortcut at a time")).toBeInTheDocument();
  await act(async () => {
    listener({ payload: { type: "drop", paths: ["/Applications/Test.app"], position: { x: 50, y: 50 } } });
  });
  expect(screen.getByLabelText("Application path")).toHaveValue("/Applications/Test.app");
  expect(onSave).not.toHaveBeenCalled();
  unmount();
  expect(stop).toHaveBeenCalledOnce();
});

test("new Action defaults to Press and commits only on Save", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  render(<ActionDialog open mode="create" language="en-US" onSave={onSave} onCancel={vi.fn()} />);
  expect(screen.getByLabelText("Trigger")).toHaveValue("press");
  expect(screen.getByLabelText("Action type")).toHaveValue("hotkey");
  await user.click(screen.getByRole("checkbox", { name: "Command" }));
  expect(onSave).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(onSave).toHaveBeenCalledWith({ trigger: "press", action: { type: "hotkey", keys: ["cmd"] } });
});

test("Cancel discards local edits and Delete is available only in edit mode", async () => {
  const user = userEvent.setup();
  const onCancel = vi.fn();
  const onDelete = vi.fn();
  const initial = { trigger: "release" as const, action: { type: "paste" as const, text: "hello" } };
  const { rerender } = render(<ActionDialog open mode="edit" language="en-US" initial={initial} onSave={vi.fn()} onCancel={onCancel} onDelete={onDelete} />);
  expect(screen.getByRole("button", { name: "Cancel" })).toHaveClass("secondary-button");
  expect(screen.getByRole("button", { name: "Save" })).toHaveClass("primary-button");
  expect(screen.getByRole("button", { name: "Delete action" })).toHaveClass("danger-button");
  await user.clear(screen.getByLabelText("Text"));
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(onCancel).toHaveBeenCalledOnce();
  expect(screen.queryByRole("button", { name: "Delete action" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Delete action" }));
  expect(onDelete).toHaveBeenCalledOnce();
  rerender(<ActionDialog open mode="create" language="en-US" onSave={vi.fn()} onCancel={vi.fn()} />);
  expect(screen.queryByRole("button", { name: "Delete action" })).not.toBeInTheDocument();
});

test("trigger selector moves an edited action and validation blocks invalid text", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  render(<ActionDialog open mode="edit" language="en-US" initial={{ trigger: "press", action: { type: "paste", text: "" } }} onSave={onSave} onCancel={vi.fn()} />);
  await user.selectOptions(screen.getByLabelText("Trigger"), "long_press");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(onSave).not.toHaveBeenCalled();
  expect(screen.getByText("Enter text to paste")).toBeInTheDocument();
});

test("uses an edit title for an existing action", () => {
  render(<ActionDialog open mode="edit" language="en-US" initial={{ trigger: "press", action: { type: "delay", duration_ms: 100 } }} onSave={vi.fn()} onCancel={vi.fn()} />);
  expect(screen.getByRole("heading", { name: "Edit action" })).toBeInTheDocument();
});

test("translates hotkey validation errors before showing them", async () => {
  const user = userEvent.setup();
  render(<ActionDialog open mode="create" language="en-US" onSave={vi.fn()} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(screen.getByText("Select at least one key")).toBeInTheDocument();
});

test("rejects open targets containing NUL or longer than 2048 characters", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  const { rerender } = render(<ActionDialog open mode="create" language="en-US" initial={{ trigger: "press", action: { type: "open", target: "bad\u0000target" } }} onSave={onSave} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(onSave).not.toHaveBeenCalled();
  expect(screen.getByText("The target cannot contain NUL characters")).toBeInTheDocument();

  rerender(<ActionDialog open mode="create" language="en-US" initial={{ trigger: "press", action: { type: "open", target: "x".repeat(2049) } }} onSave={onSave} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(screen.getByText("The target must be 2048 characters or fewer")).toBeInTheDocument();
});

test("clears validation errors when delay, media, and open values are edited", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  render(<ActionDialog open mode="edit" language="en-US" initial={{ trigger: "press", action: { type: "delay", duration_ms: 0 } }} onSave={onSave} onCancel={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(screen.getByText("Enter 1 to 60000 milliseconds")).toBeInTheDocument();
  await user.clear(screen.getByLabelText("Wait time (milliseconds)"));
  await user.type(screen.getByLabelText("Wait time (milliseconds)"), "100");
  expect(screen.queryByText("Enter 1 to 60000 milliseconds")).not.toBeInTheDocument();

  await user.selectOptions(screen.getByLabelText("Action type"), "open");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(screen.getByText("Enter a target to open")).toBeInTheDocument();
  await user.type(screen.getByLabelText("Application, URL, file, or folder"), "https://example.com");
  expect(screen.queryByText("Enter a target to open")).not.toBeInTheDocument();

  await user.selectOptions(screen.getByLabelText("Action type"), "media");
  expect(screen.queryByText("Enter a target to open")).not.toBeInTheDocument();
});
