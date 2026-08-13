import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, test, vi } from "vitest";
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

test("notifies an embedded parent outside the state updater", async () => {
  const user = userEvent.setup();
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
  function Harness() {
    const [current, setCurrent] = useState(layout);
    return <LayoutEditor layout={current} language="zh-CN" onChange={setCurrent} />;
  }

  render(<Harness />);
  await user.click(screen.getByRole("button", { name: "添加按键" }));

  expect(consoleError.mock.calls.flat().join(" ")).not.toContain("Cannot update a component");
  consoleError.mockRestore();
});
