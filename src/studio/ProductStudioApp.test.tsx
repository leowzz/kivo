import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import ProductStudioApp from "./ProductStudioApp";

vi.mock("../App", () => ({
  default: ({ embedded }: { embedded?: boolean }) => (
    <label data-testid="debug-workspace">
      debug:{String(embedded)}
      <input aria-label="调试配置草稿" />
    </label>
  ),
}));

vi.mock("./StudioApp", () => ({
  default: () => <input aria-label="产品名称草稿" />,
}));

test("opens development tools first and preserves product definition drafts across tabs", async () => {
  const user = userEvent.setup();
  render(<ProductStudioApp />);

  expect(await screen.findByTestId("debug-workspace")).toHaveTextContent("debug:true");
  expect(screen.getByRole("tab", { name: "开发调试" })).toHaveAttribute("aria-selected", "true");
  expect(screen.queryByLabelText("产品名称草稿")).toBeNull();
  await user.type(screen.getByLabelText("调试配置草稿"), "GPIO 1");

  await user.click(screen.getByRole("tab", { name: "产品定义" }));
  const draft = await screen.findByLabelText("产品名称草稿");
  await user.type(draft, "Kivo Dev");

  await user.click(screen.getByRole("tab", { name: "开发调试" }));
  expect(screen.getByLabelText("调试配置草稿")).toHaveValue("GPIO 1");
  await user.click(screen.getByRole("tab", { name: "产品定义" }));

  expect(screen.getByLabelText("产品名称草稿")).toHaveValue("Kivo Dev");
});
