import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { UpdateSettings, UpdateProgressDialog } from "./UpdateSettings";
import { initialUpdateState } from "./releaseUpdate";
import type { useReleaseUpdate } from "./useReleaseUpdate";

function update(overrides: Partial<ReturnType<typeof useReleaseUpdate>> = {}): ReturnType<typeof useReleaseUpdate> {
  return { ...initialUpdateState, check: vi.fn(), install: vi.fn(), native: true, hasUpdate: false, ...overrides };
}

test("preview does not offer native checks or installation", () => {
  render(<UpdateSettings update={update({ native: false })} language="zh-CN" />);
  expect(screen.getByText("请在桌面应用中检查更新。")).toBeVisible();
  expect(screen.getByRole("button", { name: "检查更新" })).toBeDisabled();
  expect(screen.queryByRole("button", { name: "更新并重启" })).not.toBeInTheDocument();
});

test("offers one-click installation and blocks it for unsaved work", () => {
  const state = update({ hasUpdate: true, latestVersion: "0.10.0" });
  const view = render(<UpdateSettings update={state} language="zh-CN" />);
  fireEvent.click(screen.getByRole("button", { name: "检查更新" }));
  expect(state.check).toHaveBeenCalledWith(true);
  fireEvent.click(screen.getByRole("button", { name: "更新并重启" }));
  expect(state.install).toHaveBeenCalledOnce();
  view.rerender(<UpdateSettings update={state} language="zh-CN" blocked />);
  expect(screen.getByRole("button", { name: "更新并重启" })).toBeDisabled();
  expect(screen.getByText(/请先保存当前修改/)).toBeVisible();
});

test("shows indeterminate progress for unknown sizes and restart recovery in English", () => {
  const state = update({ hasUpdate: true, latestVersion: "0.10.0", phase: "downloading", downloaded: 1048576 });
  const view = render(<UpdateProgressDialog update={state} language="en-US" />);
  expect(screen.getByRole("dialog")).toBeVisible();
  expect(screen.getByText("Downloading update… 1.0 MB")).toBeVisible();
  expect(screen.getByRole("progressbar")).not.toHaveAttribute("value");
  view.rerender(<UpdateSettings update={{ ...state, phase: "ready", error: "restart" }} language="en-US" />);
  expect(screen.getByRole("button", { name: "Restart app" })).toBeEnabled();
  expect(screen.getByText(/The update is installed/)).toBeVisible();
});

test("never says up to date after a failed check, even after an earlier successful check", () => {
  render(<UpdateSettings update={update({ checkedAt: 100, error: "check" })} language="zh-CN" />);
  expect(screen.getByText("检查更新失败")).toBeVisible();
  expect(screen.queryByText("当前已是最新版本")).not.toBeInTheDocument();
  expect(screen.getByRole("link", { name: "前往下载" })).toHaveAttribute("href", "https://github.com/leowzz/kivo/releases/latest");
});
