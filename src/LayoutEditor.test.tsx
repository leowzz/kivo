import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import { LayoutEditor } from "./LayoutEditor";
import type { ModelLayout } from "./types";

const layout: ModelLayout = {
  id: "phone",
  name: "Phone",
  groups: [
    {
      id: "GROUP_1",
      columns: 1,
      buttons: [{ id: "KEY_1", label: "已有按键" }],
    },
  ],
};

test("generates unique IDs and labels for new groups and buttons", async () => {
  const user = userEvent.setup();
  render(<LayoutEditor layout={layout} language="zh-CN" />);

  await user.click(screen.getByRole("button", { name: "添加按键" }));
  expect(screen.getAllByRole("textbox", { name: "按键 ID" }).at(-1)).toHaveValue("KEY_2");
  expect(screen.getAllByRole("textbox", { name: "名称" }).at(-1)).toHaveValue("KEY_2");

  await user.click(screen.getByRole("button", { name: "添加按键组" }));
  expect(screen.getAllByRole("textbox", { name: "按键组 ID" }).at(-1)).toHaveValue("GROUP_2");
});
