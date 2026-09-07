import { render, screen } from "@testing-library/react";
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
