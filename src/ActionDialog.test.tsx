import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { ActionDialog } from "./ActionDialog";

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
