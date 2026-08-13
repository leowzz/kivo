import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { ActionEditor } from "./ActionEditor";
import type { Language, TriggerActions } from "./types";

const emptyGroups = (): TriggerActions => ({
  press: [],
  release: [],
  long_press: [],
  double_press: [],
});

function Harness({ initial = emptyGroups(), onChange, onRename, language = "en-US" }: {
  initial?: TriggerActions;
  onChange?: (actions: TriggerActions) => void;
  onRename?: (buttonId: string, label: string) => void;
  language?: Language;
}) {
  const [actions, setActions] = useState(initial);
  const [buttonLabel, setButtonLabel] = useState("A");
  const [, setRenderTick] = useState(0);
  const update = (next: TriggerActions) => {
    setActions(next);
    onChange?.(next);
  };
  return (
    <>
      <button type="button" onClick={() => setRenderTick((value) => value + 1)}>Rerender parent</button>
      <button type="button" onClick={() => setActions(emptyGroups())}>Clear actions</button>
      <ActionEditor
        language={language}
        button={{ id: "stable-id", label: buttonLabel }}
        actions={actions}
        onChange={update}
        onRename={(buttonId, label) => {
          onRename?.(buttonId, label);
          setButtonLabel(label);
        }}
      />
      <output data-testid="actions-json">{JSON.stringify(actions)}</output>
    </>
  );
}

function configuredActions() {
  return JSON.parse(screen.getByTestId("actions-json").textContent ?? "{}") as TriggerActions;
}

test("always shows Press and only configured advanced groups in trigger order", () => {
  render(<Harness initial={{
    ...emptyGroups(),
    press: [{ type: "paste", text: "Hello from Kivo" }],
    release: [{ type: "media", command: "play_pause" }],
    double_press: [{ type: "delay", duration_ms: 300 }],
  }} />);

  expect(screen.getAllByRole("heading", { level: 3 }).map((node) => node.textContent))
    .toEqual(["Press", "Release", "Double press"]);
  expect(screen.getByText("Paste - Hello from Kivo")).toBeInTheDocument();
  expect(screen.getByText("Wait - 300 ms")).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Long press", level: 3 })).not.toBeInTheDocument();
});

test("offers common Actions for an unconfigured key and commits Copy immediately", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<Harness language="zh-CN" onChange={onChange} />);

  expect(screen.getByRole("heading", { name: "按下时 · 未设置", level: 3 })).toBeInTheDocument();
  for (const label of ["复制", "粘贴", "输入文字", "快捷键", "打开应用", "媒体控制"]) {
    expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
  }

  await user.click(screen.getByRole("button", { name: "复制" }));
  expect(onChange).toHaveBeenCalledWith({
    press: [{ type: "hotkey", keys: ["primary", "c"] }],
    release: [], long_press: [], double_press: [],
  });
});

test("commits Paste immediately and opens draft-aware common Actions", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<Harness language="zh-CN" onChange={onChange} />);

  await user.click(screen.getByRole("button", { name: "粘贴" }));
  expect(onChange).toHaveBeenCalledWith({
    press: [{ type: "hotkey", keys: ["primary", "v"] }],
    release: [], long_press: [], double_press: [],
  });

  await user.click(screen.getByRole("button", { name: "Clear actions" }));

  for (const [label, type] of [["输入文字", "paste"], ["快捷键", "hotkey"], ["打开应用", "open"], ["媒体控制", "media"]] as const) {
    await user.click(screen.getByRole("button", { name: label }));
    expect(screen.getByLabelText("触发方式")).toHaveValue("press");
    expect(screen.getByLabelText("行为类型")).toHaveValue(type);
    await user.click(screen.getByRole("button", { name: "取消" }));
  }
});

test("keeps empty advanced trigger groups hidden until adding another Action", async () => {
  const user = userEvent.setup();
  render(<Harness />);

  expect(screen.getAllByRole("heading", { level: 3 }).map((node) => node.textContent)).toEqual(["Press - Not set"]);
  expect(screen.queryByRole("heading", { name: "Release", level: 3 })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Add another action" }));
  expect(screen.getByLabelText("Trigger")).toBeInTheDocument();
});

test("renames the selected button while preserving its stable id", async () => {
  const user = userEvent.setup();
  const onRename = vi.fn();
  render(<Harness onRename={onRename} />);

  await user.click(screen.getByRole("button", { name: "Rename button A" }));
  const label = screen.getByRole("textbox", { name: "Button name" });
  await user.clear(label);
  await user.type(label, "  Launch  ");
  await user.click(screen.getByRole("button", { name: "Confirm rename" }));

  expect(onRename).toHaveBeenCalledWith("stable-id", "Launch");
  expect(screen.getByRole("heading", { name: "Launch" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Rename button Launch" })).toBeInTheDocument();
});

test("cancels a button rename without changing the label", async () => {
  const user = userEvent.setup();
  const onRename = vi.fn();
  render(<Harness onRename={onRename} />);

  await user.click(screen.getByRole("button", { name: "Rename button A" }));
  const label = screen.getByRole("textbox", { name: "Button name" });
  await user.clear(label);
  await user.type(label, "Changed");
  await user.click(screen.getByRole("button", { name: "Cancel" }));

  expect(onRename).not.toHaveBeenCalled();
  expect(screen.getByRole("heading", { name: "A" })).toBeInTheDocument();
});

test("does not save a blank button name", async () => {
  const user = userEvent.setup();
  const onRename = vi.fn();
  render(<Harness onRename={onRename} />);

  await user.click(screen.getByRole("button", { name: "Rename button A" }));
  const label = screen.getByRole("textbox", { name: "Button name" });
  await user.clear(label);
  await user.type(label, "   ");

  expect(screen.getByRole("button", { name: "Confirm rename" })).toBeDisabled();
  expect(onRename).not.toHaveBeenCalled();
});

test("uses the picker technical key names in Action summaries", () => {
  render(<Harness initial={{
    ...emptyGroups(),
    press: [{ type: "hotkey", keys: ["primary", "alt", "left_alt", "right_alt"] }],
  }} />);

  expect(screen.getByText(
    "Hotkey - Primary (Command / Control) + Option / Alt + Left Option / Alt + Right Option / Alt",
  )).toBeInTheDocument();
});

test("localizes hotkey summaries without rewriting action tokens", () => {
  render(<Harness language="zh-CN" initial={{
    ...emptyGroups(),
    press: [{ type: "hotkey", keys: ["right_cmd", "left"] }],
  }} />);

  expect(screen.getByText("快捷键 - 右cmd + 方向左")).toBeInTheDocument();
  expect(configuredActions().press).toEqual([
    { type: "hotkey", keys: ["right_cmd", "left"] },
  ]);
});

test("changing trigger appends the Action to the destination group", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<Harness
    initial={{ ...emptyGroups(), press: [{ type: "paste", text: "A" }] }}
    onChange={onChange}
  />);

  await user.click(screen.getByRole("button", { name: "Edit Paste - A" }));
  await user.selectOptions(screen.getByLabelText("Trigger"), "release");
  await user.click(screen.getByRole("button", { name: "Save" }));

  expect(onChange).toHaveBeenCalledTimes(1);
  expect(onChange.mock.calls[0][0].press).toEqual([]);
  expect(onChange.mock.calls[0][0].release).toEqual([{ type: "paste", text: "A" }]);
});

test("keeps dialog edits local until save and supports cancel", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<Harness
    initial={{ ...emptyGroups(), press: [{ type: "paste", text: "A" }] }}
    onChange={onChange}
  />);

  await user.click(screen.getByRole("button", { name: "Edit Paste - A" }));
  const text = screen.getByRole("textbox", { name: "Text" });
  await user.clear(text);
  await user.type(text, "changed");
  expect(onChange).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(configuredActions().press).toEqual([{ type: "paste", text: "A" }]);
});

test("preserves an in-progress dialog draft when its parent rerenders", async () => {
  const user = userEvent.setup();
  render(<Harness initial={{ ...emptyGroups(), press: [{ type: "paste", text: "A" }] }} />);

  await user.click(screen.getByRole("button", { name: "Edit Paste - A" }));
  const text = screen.getByRole("textbox", { name: "Text" });
  await user.clear(text);
  await user.type(text, "draft survives");
  await user.click(screen.getByRole("button", { name: "Rerender parent" }));

  expect(screen.getByRole("textbox", { name: "Text" })).toHaveValue("draft survives");
});

test("does not apply an edit after its source action disappears", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<Harness
    initial={{ ...emptyGroups(), press: [{ type: "paste", text: "A" }] }}
    onChange={onChange}
  />);

  await user.click(screen.getByRole("button", { name: "Edit Paste - A" }));
  await user.click(screen.getByRole("button", { name: "Clear actions" }));
  await user.click(screen.getByRole("button", { name: "Save" }));

  expect(onChange).not.toHaveBeenCalled();
  expect(configuredActions().press).toEqual([]);
});

test("deletes an action from its trigger group", async () => {
  const user = userEvent.setup();
  render(<Harness initial={{ ...emptyGroups(), release: [{ type: "delay", duration_ms: 100 }] }} />);

  await user.click(screen.getByRole("button", { name: "Edit Wait - 100 ms" }));
  await user.click(screen.getByRole("button", { name: "Delete action" }));

  expect(configuredActions().release).toEqual([]);
  expect(configuredActions().press).toEqual([]);
});

test("moves actions only within the same trigger group", async () => {
  const user = userEvent.setup();
  render(<Harness initial={{
    ...emptyGroups(),
    press: [
      { type: "paste", text: "first" },
      { type: "paste", text: "second" },
    ],
    release: [{ type: "delay", duration_ms: 10 }],
  }} />);

  const moveUp = screen.getAllByRole("button", { name: "Move up" })[1];
  await user.click(moveUp);
  expect(configuredActions().press.map((action) => (action.type === "paste" ? action.text : "")))
    .toEqual(["second", "first"]);
  expect(configuredActions().release).toEqual([{ type: "delay", duration_ms: 10 }]);
});

test("creates a new action with the default press trigger", async () => {
  const user = userEvent.setup();
  render(<Harness />);

  await user.click(screen.getByRole("button", { name: "Add another action" }));
  expect(screen.getByLabelText("Trigger")).toHaveValue("press");
  expect(screen.getByLabelText("Action type")).toHaveValue("hotkey");
  await user.click(screen.getByLabelText("Action type"));
  await user.selectOptions(screen.getByLabelText("Action type"), "paste");
  await user.type(screen.getByRole("textbox", { name: "Text" }), "new");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(configuredActions().press).toEqual([{ type: "paste", text: "new" }]);
});
