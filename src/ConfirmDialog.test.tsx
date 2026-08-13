import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { ConfirmDialog } from "./ConfirmDialog";

test("disables every dialog action while pending", () => {
  const onCancel = vi.fn();
  const onConfirm = vi.fn();
  render(
    <ConfirmDialog
      title="Confirm"
      body="Body"
      confirmLabel="Confirm"
      cancelLabel="Cancel"
      pending
      onCancel={onCancel}
      onConfirm={onConfirm}
    />,
  );

  expect(screen.getByRole("button", { name: "Confirm" })).toBeDisabled();
  expect(screen.getAllByRole("button", { name: "Cancel" })).toHaveLength(2);
  for (const button of screen.getAllByRole("button", { name: "Cancel" })) {
    expect(button).toBeDisabled();
  }
});

test("keeps keyboard focus inside, closes with Escape, and restores focus", async () => {
  const user = userEvent.setup();
  const onCancel = vi.fn();
  const background = document.createElement("button");
  document.body.appendChild(background);
  background.focus();
  const { unmount } = render(
    <ConfirmDialog title="Confirm" body="Body" confirmLabel="Confirm" cancelLabel="Cancel" onCancel={onCancel} onConfirm={vi.fn()} />,
  );
  const buttons = within(screen.getByRole("dialog")).getAllByRole("button");
  expect(buttons[0]).toHaveFocus();
  await user.tab({ shift: true });
  expect(buttons[buttons.length - 1]).toHaveFocus();
  await user.tab();
  expect(buttons[0]).toHaveFocus();
  fireEvent.keyDown(window, { key: "Escape" });
  expect(onCancel).toHaveBeenCalledOnce();
  unmount();
  expect(background).toHaveFocus();
  background.remove();
});
