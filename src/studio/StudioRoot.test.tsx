import { fireEvent, render, screen } from "@testing-library/react";
import { useEffect } from "react";
import { expect, test, vi } from "vitest";
import StudioRoot from "./StudioRoot";

const { release, install } = vi.hoisted(() => ({ release: vi.fn(), install: vi.fn() }));
vi.mock("../shared/useReleaseUpdate", () => ({
  useReleaseUpdate: () => ({ phase: "idle", latestVersion: "1.0.0", checkedAt: 1,
    downloaded: 0, total: null, error: null, check: vi.fn(), install, native: true, hasUpdate: true }),
}));
vi.mock("./StudioApp", () => ({
  default: ({ onUpdateBlockedChange }: { onUpdateBlockedChange: (blocked: boolean) => void }) =>
    <input aria-label="产品名称" defaultValue="" onChange={event => onUpdateBlockedChange(event.target.value !== "")} />,
}));
vi.mock("./Toolbox", () => ({
  default: function Monitor() {
    useEffect(() => release, []);
    return <h1>GPIO 测试</h1>;
  },
}));

test("preserves product drafts while releasing hardware tests when changing views", async () => {
  render(<StudioRoot />);
  fireEvent.change(screen.getByLabelText("产品名称"), {
    target: { value: "我的键盘" },
  });
  fireEvent.click(screen.getByRole("button", { name: "工具台" }));
  await screen.findByRole("heading", { name: "GPIO 测试" });
  expect(screen.getByLabelText("产品名称")).not.toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "产品定义" }));
  expect(screen.getByLabelText("产品名称")).toHaveValue("我的键盘");
  expect(release).toHaveBeenCalledOnce();
});

test("keeps unsaved product work from being interrupted by an update", () => {
  render(<StudioRoot />);
  fireEvent.change(screen.getByLabelText("产品名称"), { target: { value: "未保存的键盘" } });
  fireEvent.click(screen.getByRole("button", { name: /应用更新/ }));
  expect(screen.getByRole("button", { name: "更新并重启" })).toBeDisabled();
  expect(screen.getByLabelText("产品名称")).toHaveValue("未保存的键盘");
  fireEvent.change(screen.getByLabelText("产品名称"), { target: { value: "" } });
  expect(screen.getByRole("button", { name: "更新并重启" })).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "更新并重启" }));
  expect(install).toHaveBeenCalledOnce();
});
