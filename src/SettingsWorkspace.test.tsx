import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { SettingsWorkspace } from "./SettingsWorkspace";

test("keeps ordinary settings to language, backup, restore, and advanced settings", () => {
  render(<SettingsWorkspace language="zh-CN" onLanguageChange={vi.fn()} onBackup={vi.fn()} onRestore={vi.fn()} onOpenAdvanced={vi.fn()} />);

  expect(screen.getByRole("heading", { name: "设置" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "备份" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "恢复" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "高级设置" })).toBeInTheDocument();
  expect(screen.queryByText("I/O 映射")).not.toBeInTheDocument();
  expect(screen.queryByText("按键布局")).not.toBeInTheDocument();
  expect(screen.queryByText(/\/dev\//)).not.toBeInTheDocument();
  expect(screen.queryByText(/profile-/)).not.toBeInTheDocument();
});

test("emits language and transfer commands without coupling them", async () => {
  const user = userEvent.setup();
  const onLanguageChange = vi.fn();
  const onBackup = vi.fn();
  const onRestore = vi.fn();
  render(<SettingsWorkspace language="zh-CN" onLanguageChange={onLanguageChange} onBackup={onBackup} onRestore={onRestore} onOpenAdvanced={vi.fn()} />);

  await user.selectOptions(screen.getByRole("combobox", { name: "语言" }), "en-US");
  await user.click(screen.getByRole("button", { name: "备份" }));
  await user.click(screen.getByRole("button", { name: "恢复" }));
  expect(onLanguageChange).toHaveBeenCalledWith("en-US");
  expect(onBackup).toHaveBeenCalledTimes(1);
  expect(onRestore).toHaveBeenCalledTimes(1);
});
