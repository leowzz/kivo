import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { SharedProfileEditDialog } from "./SharedProfileEditDialog";

test("names the keyboard, profile, and affected devices before choosing an edit scope", async () => {
  const user = userEvent.setup();
  const onChoose = vi.fn();
  const onCancel = vi.fn();
  render(
    <SharedProfileEditDialog
      language="zh-CN"
      deviceName="前台键盘"
      profileName="碳膜电话键盘"
      affectedDeviceCount={2}
      allowDeviceScope
      onChoose={onChoose}
      onCancel={onCancel}
    />,
  );

  expect(screen.getByRole("heading", { name: "选择修改范围" })).toBeInTheDocument();
  expect(screen.getByText(/前台键盘/)).toBeInTheDocument();
  expect(screen.getByText(/碳膜电话键盘/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "仅修改这台键盘" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "同步修改 2 台键盘" })).toBeInTheDocument();
  expect(screen.getByText("修改会影响使用此设置的其他键盘")).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "仅修改这台键盘" }));
  expect(onChoose).toHaveBeenCalledWith("device");
  expect(onCancel).not.toHaveBeenCalled();
});

test("only offers the shared scope when the profile is not assigned to the current keyboard", () => {
  render(
    <SharedProfileEditDialog
      language="zh-CN"
      deviceName="前台键盘"
      profileName="备用配置"
      affectedDeviceCount={2}
      allowDeviceScope={false}
      onChoose={vi.fn()}
      onCancel={vi.fn()}
    />,
  );

  expect(screen.queryByRole("button", { name: "仅修改这台键盘" })).toBeNull();
  expect(screen.getByRole("button", { name: "同步修改 2 台键盘" })).toBeInTheDocument();
});

test("disables every scope choice and dismissal control while submitting", () => {
  render(
    <SharedProfileEditDialog
      language="zh-CN"
      deviceName="前台键盘"
      profileName="碳膜电话键盘"
      affectedDeviceCount={2}
      allowDeviceScope
      submitting
      onChoose={vi.fn()}
      onCancel={vi.fn()}
    />,
  );

  expect(screen.getByRole("dialog")).toHaveAttribute("aria-busy", "true");
  for (const button of screen.getAllByRole("button")) expect(button).toBeDisabled();
});

test("keeps focus inside and closes with Escape", async () => {
  const user = userEvent.setup();
  const onCancel = vi.fn();
  render(
    <SharedProfileEditDialog language="zh-CN" deviceName="前台键盘" profileName="共享配置" affectedDeviceCount={2} allowDeviceScope onChoose={vi.fn()} onCancel={onCancel} />,
  );
  const buttons = screen.getAllByRole("button");
  expect(buttons[0]).toHaveFocus();
  await user.tab({ shift: true });
  expect(buttons[buttons.length - 1]).toHaveFocus();
  await user.tab();
  expect(buttons[0]).toHaveFocus();
  fireEvent.keyDown(window, { key: "Escape" });
  expect(onCancel).toHaveBeenCalledOnce();
});
