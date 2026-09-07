import { fireEvent, render, screen } from "@testing-library/react";
import { useEffect } from "react";
import { expect, test, vi } from "vitest";
import StudioRoot from "./StudioRoot";

const { release } = vi.hoisted(() => ({ release: vi.fn() }));
vi.mock("./StudioApp", () => ({
  default: () => <input aria-label="产品名称" defaultValue="" />,
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
